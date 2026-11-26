#!/usr/bin/env python3
"""PTY integration: scenes, PNG/paste editing, player arguments and process cleanup.
Uses only Python's standard library; fake players avoid network/audio in this test.
"""
import fcntl
import json
import os
from pathlib import Path
import pty
import signal
import struct
import subprocess
import tempfile
import termios
import zlib

from pty_support import read_output, terminate, wait_for, wait_output

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
import json,os,pathlib,socket,subprocess,sys,time
child=subprocess.Popen(['/usr/bin/sleep','120'])
target=pathlib.Path(os.environ['LOFIJI_TEST_PLAYER'])
ipc = next((arg.split('=', 1)[1] for arg in sys.argv if arg.startswith('--input-ipc-server=')), None)
if ipc:
    target=target.parent/f'sound-{os.getpid()}.json'
    server=socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(ipc)
    server.listen()
record=target.with_suffix('.tmp')
record.write_text(json.dumps({'pid':os.getpid(),'child':child.pid,'group':os.getpgrp(),'args':sys.argv[1:]}))
record.replace(target)
if ipc:
    while True:
        conn, _ = server.accept()
        with conn:
            message = json.loads(conn.makefile().readline())
        target.with_suffix('.volume').write_text(str(message['command'][2]))
else:
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
    def send(data):
        os.write(master, data)
        return read_output(master, .5)
    try:
        wait_output(master, b'\x1b[?2004h')
        assert not (tmp/'player.json').exists(),'music must start off'
        send(b'b')
        wait_for(master, lambda: config.exists() and 'background = "city"' in config.read_text())
        os.write(master,b's\r'+b'\x1b[B'*5+b'\r')
        wait_output(master, b'\x1b[?25h')
        send(b'\x1b[200~'+str(image).encode()+b'\x1b[201~')
        os.write(master,b'\r'+b'\x1b[A'+b' '*5+b'\x13')
        wait_for(master, lambda: 'background = "image"' in config.read_text() and 'café.png' in config.read_text())
        text=config.read_text()
        config.write_text(text.replace(str(image),image.name))
        subprocess.run([str(BIN),'--config',str(config),'--snapshot','40x12'],stdout=subprocess.DEVNULL,check=True)
        config.write_text(text)
        assert proc.poll() is None,'PNG selection crashed'
        send(b'sjj\r\r')
        send(b'\x1b');send(b'\x1b');send(b'\x1b')
        send(b'b')
        wait_for(master, lambda: 'background = "window"' in config.read_text())

        # Mix three local layers, including the new fireplace; changing one must preserve the other voice.
        send(b'a '+b'\x1b[B'+b' '+b'\x1b[B'+b' '+b'\r')
        wait_for(master, lambda: len(list(tmp.glob('sound-*.json'))) == 3)
        sound_records=[json.loads(p.read_text()) for p in tmp.glob('sound-*.json')]
        assert len(sound_records)==3 and all(alive(r['pid']) for r in sound_records),'layers did not overlap'
        sound_paths=[Path(r['args'][-1]) for r in sound_records]
        assert all(p.is_file() for p in sound_paths)
        for r in sound_records:
            assert r['pid']==r['group'] and '--loop-file=inf' in r['args']
        fire=next(r for r in sound_records if r['args'][-1].endswith('/0.ogg'))
        fireplace=next(r for r in sound_records if r['args'][-1].endswith('/10.ogg'))
        assert '--volume=25' in fireplace['args'],'fireplace level not applied'
        send(b'a'+b'\x1b[C')
        level=tmp/f"sound-{fire['pid']}.volume"
        wait_for(master, lambda: level.exists() and level.read_text() == '30')
        assert all(alive(r['pid']) for r in sound_records), 'audition restarted a voice'
        send(b'\x1b')
        wait_for(master, lambda: level.read_text() == '25')
        send(b'a'+b'\x1b[C'+b'\r')
        wait_for(master, lambda: level.read_text() == '30')
        assert all(alive(r['pid']) for r in sound_records), 'saving restarted a voice'
        send(b'z')
        wait_for(master, lambda: all(not alive(r['pid']) and not alive(r['child']) for r in sound_records))
        send(b'z')
        wait_for(master, lambda: len(list(tmp.glob('sound-*.json'))) == 6)
        resumed=[json.loads(p.read_text()) for p in tmp.glob('sound-*.json') if alive(json.loads(p.read_text())['pid'])]
        assert len(resumed)==3,'saved mix did not resume'

        for ending in ['toggle','quit']:
            record_path=tmp/'player.json'
            record_path.unlink(missing_ok=True)
            os.write(master,b'm')
            output=wait_output(master,b'Music playing', visible=True)
            record=json.loads(record_path.read_text())
            assert b'LOFIJI_AUDIO_READY' not in output, 'raw player output leaked'
            assert record['pid']==record['group'],'player must have its own process group'
            assert record['args'][-2:] == ['--','https://www.youtube.com/watch?v=DEWzT1geuPU']
            assert '--no-video' in record['args']
            output=send(b'm' if ending=='toggle' else b'q')
            output+=wait_for(master, lambda: not alive(record['pid']) and not alive(record['child']))
            if ending == 'quit' and b'\x1b[?2004l' not in output:
                output+=wait_output(master, b'\x1b[?2004l')
        assert proc.wait(timeout=2)==0
        assert all(not alive(r['pid']) and not alive(r['child']) for r in resumed),'quit left ambient audio playing'
        assert all(not p.exists() for p in sound_paths),'temporary sound assets were not removed'
        assert proc.wait(timeout=2)==0
        output+=read_output(master, .1)
        assert b'\x1b[?2004l' in output,'bracketed paste not restored'
        assert termios.tcgetattr(slave)==baseline,'terminal flags not restored'
        print('PASS: persistent scenes, PNG Unicode paste/editor, layered ambient audio, music readiness, owned-process cleanup')
    finally:
        terminate(master, proc)
        os.close(master);os.close(slave)
