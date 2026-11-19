#!/usr/bin/env python3
"""Real PTY: linked goals/tasks, text conversation, reopening reminders, corrupt-file protection."""
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time
import tomllib

ROOT=Path(__file__).resolve().parents[1]
BIN=ROOT/'target/debug/lofiji'
with tempfile.TemporaryDirectory(prefix='lofiji-planner-pty-') as tmp:
    config=Path(tmp)/'config.toml';data=config.with_suffix('.planner.toml')
    master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,80,0,0))
    baseline=termios.tcgetattr(slave)
    def launch():return subprocess.Popen([str(BIN),'--config',str(config)],stdin=slave,stdout=slave,stderr=slave)
    def drain(seconds=.15):
        end=time.monotonic()+seconds;out=bytearray()
        while time.monotonic()<end:
            if select.select([master],[],[],max(0,end-time.monotonic()))[0]:out.extend(os.read(master,65536))
        return bytes(out)
    def send(keys):os.write(master,keys);return drain()
    def redraw():
        # A full frame avoids matching phrases split across VT cell updates.
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',25,80,0,0))
        os.kill(proc.pid,signal.SIGWINCH);drain(.1)
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,80,0,0))
        os.kill(proc.pid,signal.SIGWINCH)
        return drain(.15)
    def paste(text):return send(b'\x1b[200~'+text.encode()+b'\x1b[201~')
    def entries():return tomllib.loads(data.read_text())['entries']
    proc=launch()
    try:
        drain(.3);send(b'+');paste('Review equations in 東京');send(b'\t')
        paste('Compare the proof with the café notes.');send(b'\r')
        assert entries()[0]['title']=='Review equations in 東京' and proc.poll() is None,'q or Unicode broke task entry'
        assert entries()[0]['description']=='Compare the proof with the café notes.'
        send(b'\x1b');send(b'g'+b'\x1b[C'*2+b'+');paste('Publish a thoughtful project');send(b'\r')
        send(b'\x1b[D'+b'+');paste('Finish the first release')
        send(b'\t'+b'\x1b[C'+b'\r')
        rows=entries();assert rows[2]['parent']==rows[1]['id'],'short goal was not linked to long goal'
        send(b'\x1b[D'+b'\r')
        out=re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]',b'',send(b'\t')+redraw())
        assert b'Compare the proof' in out, 'description did not reopen in the editor: '+repr(out[-2000:])
        send(b'\t'+b'\x1b[C'*2+b'\t');paste('2026-11-17')
        send(b'\t'+b'\x1b[D'+b'\r')
        rows=entries();assert rows[0]['parent']==rows[2]['id'] and rows[0]['priority']=='high' and rows[0]['due']=='2026-11-17'
        assert rows[0]['description']=='Compare the proof with the café notes.', 'editing other fields changed the description'
        send(b'\x1b');out=re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]',b'',send(b'c')+redraw());assert b'AI off' in out,'local mode was not disclosed: '+repr(out[-8000:])
        paste('task: Ask a question about café');send(b'\r');assert len(entries())==4
        paste('done #1');send(b'\r');assert entries()[0]['done']
        send(b'\x1b');send(b'h+');paste('Listen before reacting 東京');send(b'\t\r')
        assert entries()[4]['kind']=='character' and entries()[4]['title']=='Listen before reacting 東京'
        send(b' ');assert entries()[4]['done'],'reminder did not pause'
        send(b' ');assert not entries()[4]['done'],'reminder did not resume'
        send(b'x');assert entries()[4]['archived']
        send(b'u');assert not entries()[4]['archived']
        send(b'c');paste('reminder: Keep small promises');send(b'\r')
        assert entries()[5]['kind']=='character','local reminder command was not saved'
        paste('unsent draft for tomorrow')
        send(b'\x1b');send(b'g')
        send(b'\x1b');send(b'c')
        out=re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]',b'',redraw())
        assert b'unsent draft for tomorrow' in out and b'Keep small promises' in out,'leaving chat lost draft or history'
        send(b'\x0e')
        out=re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]',b'',redraw())
        assert b'unsent draft for tomorrow' not in out and b'Keep small promises' not in out,'new chat retained old draft or history'
        send(b'\x1b');send(b'h');send(b'g')
        # Changing menu must not lose records or let navigation letters edit them.
        assert len(entries())==6
        send(b'\x1b');send(b'q');assert proc.wait(timeout=2)==0
        assert termios.tcgetattr(slave)==baseline
        proc=launch();out=re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]',b'',drain(.3));assert b'Ask a question' in out,'opening reminder did not reload saved tasks: '+repr(out[-2000:])
        send(b'q');assert proc.wait(timeout=2)==0
        data.write_text('broken = [')
        proc=launch();drain(.3);send(b'+');paste('Do not overwrite');send(b'\r')
        assert data.read_text()=='broken = [','malformed planner was overwritten'
        send(b'\x1b');send(b'\x1b');send(b'q');assert proc.wait(timeout=2)==0
        assert termios.tcgetattr(slave)==baseline
        print('PASS: inline workspace, character reminders/pause/archive, linked goals, deadlines/priority, Unicode editing, local chat, persistence and corruption protection')
    finally:
        if proc.poll() is None:proc.terminate();proc.wait(timeout=3)
        os.close(master);os.close(slave)
