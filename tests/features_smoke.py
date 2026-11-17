#!/usr/bin/env python3
"""PTY integration: scenes, PNG/paste editing, player arguments and process cleanup.
Uses only Python's standard library; fake players avoid network/audio in this test.
"""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time
import zlib

ROOT=Path(__file__).resolve().parents[1]
BIN=ROOT/'target/debug/lofiji'

def png(path):
    def chunk(kind,data):return struct.pack('!I',len(data))+kind+data+struct.pack('!I',zlib.crc32(kind+data)&0xffffffff)
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!2I5B',2,2,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0'+bytes([50,60,90,255])*2+b'\0'+bytes([90,40,80,255])*2))+chunk(b'IEND',b''))

def alive(pid):
    p=Path(f'/proc/{pid}/stat')
    return p.exists() and p.read_text().split()[2]!='Z'

with tempfile.TemporaryDirectory(prefix='lofiji-features-') as tmp:
    tmp=Path(tmp);config=tmp/'config.toml';image=tmp/'café.png';png(image)
    helper=tmp/'yt-dlp';helper.write_text('#!/bin/sh\nexit 0\n');helper.chmod(0o755)
    player=tmp/'mpv';player.write_text('''#!/usr/bin/python3
import json,os,pathlib,subprocess,sys,time
child=subprocess.Popen(['/usr/bin/sleep','120'])
target=pathlib.Path(os.environ['LOFIJI_TEST_PLAYER'])
if sys.argv[-1].endswith('.ogg'): target=target.parent/f'sound-{os.getpid()}.json'
target.write_text(json.dumps({'pid':os.getpid(),'child':child.pid,'group':os.getpgrp(),'args':sys.argv[1:]}))
print('LOFIJI_AUDIO_READY', flush=True)
time.sleep(120)
''');player.chmod(0o755)
    env=os.environ.copy();env['PATH']=str(tmp)+':'+env['PATH'];env['LOFIJI_TEST_PLAYER']=str(tmp/'player.json')
    legacy_root=tmp/'legacy-config';legacy=(legacy_root/'nekoji');legacy.mkdir(parents=True)
    legacy_file=legacy/'config.toml';legacy_file.write_text('focus_minutes = 0\n')
    migration_env=env.copy();migration_env['XDG_CONFIG_HOME']=str(legacy_root)
    check=subprocess.run([str(BIN),'--check-config'],env=migration_env,capture_output=True,text=True)
    assert check.returncode==1 and 'duration' in check.stderr,'legacy settings were not imported'
    legacy_file.write_text('format_24h = false\n')
    check=subprocess.run([str(BIN),'--check-config'],env=migration_env,capture_output=True,text=True,check=True)
    assert str(legacy_root/'lofiji/config.toml') in check.stdout
    assert legacy_file.read_text()=='format_24h = false\n','legacy file was modified'
    master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,120,0,0))
    baseline=termios.tcgetattr(slave)
    proc=subprocess.Popen([str(BIN),'--config',str(config)],stdin=slave,stdout=slave,stderr=slave,env=env)
    def drain(seconds=.2):
        end=time.monotonic()+seconds;out=bytearray()
        while time.monotonic()<end:
            if select.select([master],[],[],max(0,end-time.monotonic()))[0]:
                try:out.extend(os.read(master,65536))
                except OSError:break
        return bytes(out)
    def send(data):os.write(master,data);return drain()
    try:
        assert b'\x1b[?2004h' in drain(.3),'bracketed paste not enabled'
        assert not (tmp/'player.json').exists(),'music must start off'
        send(b'b');assert 'background = "city"' in config.read_text()
        send(b's\r'+b'\x1b[B'*5+b'\r')
        send(b'\x1b[200~'+str(image).encode()+b'\x1b[201~')
        send(b'\r'+b'\x1b[A'+b' '*4+b'\x13')
        text=config.read_text();assert 'background = "image"' in text and 'café.png' in text,text
        config.write_text(text.replace(str(image),image.name))
        subprocess.run([str(BIN),'--config',str(config),'--snapshot','40x12'],stdout=subprocess.DEVNULL,check=True)
        config.write_text(text)
        assert proc.poll() is None,'PNG selection crashed'
        send(b'sjj\r\r')
        send(b'\x1b');send(b'\x1b');send(b'\x1b')
        send(b'b');assert 'background = "window"' in config.read_text(),'Escape must close editor and settings'

        # Mix three local layers, including the new fireplace; changing one must preserve the other voice.
        send(b'a '+b'\x1b[B'+b' '+b'\x1b[B'+b' '+b'\r');drain(.2)
        sound_records=[json.loads(p.read_text()) for p in tmp.glob('sound-*.json')]
        assert len(sound_records)==3 and all(alive(r['pid']) for r in sound_records),'layers did not overlap'
        sound_paths=[Path(r['args'][-1]) for r in sound_records]
        assert all(p.is_file() for p in sound_paths)
        for r in sound_records:
            assert r['pid']==r['group'] and '--loop-file=inf' in r['args']
        fire=next(r for r in sound_records if r['args'][-1].endswith('/0.ogg'))
        rain=next(r for r in sound_records if r['args'][-1].endswith('/1.ogg'))
        fireplace=next(r for r in sound_records if r['args'][-1].endswith('/10.ogg'))
        assert '--volume=25' in fireplace['args'],'fireplace level not applied'
        send(b'a'+b'\x1b[C'+b'\r');drain(.15)
        assert alive(rain['pid']) and alive(fireplace['pid']) and not alive(fire['pid']),'changing fire restarted rain or left the old voice'
        sound_records=[json.loads(p.read_text()) for p in tmp.glob('sound-*.json')]
        assert any(alive(r['pid']) and '--volume=30' in r['args'] for r in sound_records),'new layer volume not applied'
        send(b'z');assert all(not alive(r['pid']) and not alive(r['child']) for r in sound_records),'mute left an audio child'
        send(b'z');drain(.2)
        resumed=[json.loads(p.read_text()) for p in tmp.glob('sound-*.json') if alive(json.loads(p.read_text())['pid'])]
        assert len(resumed)==3,'saved mix did not resume'

        for ending in ['toggle','quit']:
            output=send(b'm');record=json.loads((tmp/'player.json').read_text())
            output+=drain(.15)
            assert b'Music playing' in output, 'audio readiness was not reported'
            assert b'LOFIJI_AUDIO_READY' not in output, 'raw player output leaked'
            assert record['pid']==record['group'],'player must have its own process group'
            assert record['args'][-2:] == ['--','https://www.youtube.com/watch?v=DEWzT1geuPU']
            assert '--no-video' in record['args']
            output=send(b'm' if ending=='toggle' else b'q')
            for _ in range(20):
                if not alive(record['pid']) and not alive(record['child']):break
                time.sleep(.05)
            assert not alive(record['pid']) and not alive(record['child']),'orphan player/helper'
        assert proc.wait(timeout=2)==0
        assert all(not alive(r['pid']) and not alive(r['child']) for r in resumed),'quit left ambient audio playing'
        assert all(not p.exists() for p in sound_paths),'temporary sound assets were not removed'
        assert proc.wait(timeout=2)==0
        output+=drain(.15)
        assert b'\x1b[?2004l' in output,'bracketed paste not restored'
        assert termios.tcgetattr(slave)==baseline,'terminal flags not restored'
        print('PASS: persistent scenes, PNG Unicode paste/editor, layered ambient audio, music readiness, owned-process cleanup')
    finally:
        if proc.poll() is None:proc.terminate();proc.wait(timeout=3)
        os.close(master);os.close(slave)
