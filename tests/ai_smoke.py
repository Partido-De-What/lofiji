#!/usr/bin/env python3
"""Provider-native HTTP protocols through real PTYs; no external requests or live keys."""
import fcntl
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
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
import threading
import time
import tomllib

ROOT=Path(__file__).resolve().parents[1]
BIN=ROOT/'target/debug/lofiji'
SECRET='mock-private-key-never-print'
ANSI=re.compile(rb'\x1b\[[0-?]*[ -/]*[@-~]')
class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_POST(self):
        size=int(self.headers.get('Content-Length',0));body=json.loads(self.rfile.read(size))
        self.server.records.append((self.path,dict(self.headers),body))
        provider=self.server.provider;mode=self.server.mode;self.server.started.set()
        if mode in ['slow','timeout','stale']:self.server.release.wait(10)
        if mode=='error':status=401;response={'error':{'message':SECRET}}
        else:
            status=200
            text='Stale suggestion must not appear' if mode=='stale' else 'Provider advice: review the proof first.'
            if 'Suggest my next step for today' in json.dumps(body):
                text += '\nTip: focus'

            if provider=='openai':response={'output':[{'content':[{'type':'output_text','text':text}]}]}
            elif provider=='anthropic':response={'content':[{'type':'text','text':text}]}
            elif provider=='gemini':response={'candidates':[{'content':{'parts':[{'text':text}]}}]}
            elif provider=='ollama':response={'message':{'role':'assistant','content':text},'done':True}
            else:response={'choices':[{'message':{'role':'assistant','content':text}}]}
        encoded=json.dumps(response).encode()
        try:
            self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(encoded)));self.end_headers();self.wfile.write(encoded)
        except (BrokenPipeError,ConnectionResetError):pass

server=ThreadingHTTPServer(('127.0.0.1',0),Handler);server.daemon_threads=True
server.records=[];server.provider='openai';server.mode='normal';server.started=threading.Event();server.release=threading.Event()
threading.Thread(target=server.serve_forever,daemon=True).start()

def alive(pid):
    p=Path(f'/proc/{pid}/stat');return p.exists() and p.read_text().split()[2]!='Z'

with tempfile.TemporaryDirectory(prefix='lofiji-ai-test-') as tmp:
    root=Path(tmp)
    def run(provider,mode='normal',auto=False):
        server.provider=provider;server.mode=mode;server.started.clear();server.release.clear()
        config=root/f'{provider}-{mode}.toml';data=config.with_suffix('.planner.toml')
        key='' if provider=='ollama' else 'LOFIJI_TEST_AI_KEY'
        config.write_text(f'animations = false\n[ai]\nprovider = "{provider}"\nmodel = "mock-model"\nbase_url = "http://127.0.0.1:{server.server_port}"\nkey_env = "{key}"\ntimeout_seconds = 5\nauto_brief = {str(auto).lower()}\n')
        data.write_text('version = 1\n[[entries]]\nid = 1\nkind = "task"\ntitle = "Review the proof"\n')
        env=os.environ.copy();env['LOFIJI_TEST_AI_KEY']=SECRET
        master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,120,0,0));baseline=termios.tcgetattr(slave)
        before=len(server.records)
        proc=subprocess.Popen([str(BIN),'--config',str(config)],stdin=slave,stdout=slave,stderr=slave,env=env)
        output=bytearray()
        def drain(seconds=.15):
            end=time.monotonic()+seconds
            while time.monotonic()<end:
                if select.select([master],[],[],max(0,end-time.monotonic()))[0]:output.extend(os.read(master,65536))
            return ANSI.sub(b'',bytes(output))
        def send(keys):os.write(master,keys);return drain()
        def wait_for(marker,seconds=4):
            end=time.monotonic()+seconds;last_resize=0;width=120
            while time.monotonic()<end:
                if marker in drain(.05):return
                # A VT stream contains differential cell updates. Resizing asks
                # Ratatui for a complete frame before checking visible text.
                if time.monotonic()-last_resize>.3:
                    width=119 if width==120 else 120
                    fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,width,0,0))
                    os.kill(proc.pid,signal.SIGWINCH);last_resize=time.monotonic()
            raise AssertionError(f'{provider}/{mode}: missing expected UI response '+repr(marker))
        try:
            drain(.25)
            if not auto:
                assert len(server.records)==before,'AI request occurred without a user action'
                send(b'c')
                if provider=='openai':
                    send(b'\x05');wait_for(b'Effort Low')
                send(b'What should I do?\r')
            assert server.started.wait(2),'AI HTTP request never reached the local server'
            path,headers,body=server.records[-1]
            expected={'openai':'/responses','anthropic':'/messages','gemini':'/models/mock-model:generateContent','ollama':'/api/chat','openrouter':'/chat/completions','compatible':'/chat/completions'}[provider]
            assert path==expected,(provider,path)
            if provider=='gemini':assert headers.get('x-goog-api-key')==SECRET and SECRET not in path
            elif provider!='ollama':assert headers.get('Authorization')=='Bearer '+SECRET
            else:assert 'Authorization' not in headers
            assert 'Review the proof' in json.dumps(body),'saved task context was not sent'
            if provider=='openai':
                assert body.get('reasoning')=={'effort':'low'},'Talk effort did not reach Responses API'
            if mode=='slow':
                wait_for(b'Waiting for AI')
                wait_until=time.monotonic()+1
                spinner=[frame.encode() for frame in ['◐','◓','◑','◒']]
                while sum(frame in output for frame in spinner)<2 and time.monotonic()<wait_until:
                    drain(.05)
                assert sum(frame in output for frame in spinner)>=2,'waiting indicator stopped animating with scene motion off'
                children=set()
                for file in Path(f'/proc/{proc.pid}/task').glob('*/children'):
                    try:children.update(file.read_text().split())
                    except FileNotFoundError:pass
                assert children,'no worker-owned curl process found'
                for pid in children:
                    argv=Path(f'/proc/{pid}/cmdline').read_bytes();assert SECRET.encode() not in argv and b'Review the proof' not in argv
                start=time.monotonic();send(b'\x0b');wait_for(b'Reply cancelled.');send(b'\x1b');send(b'q');assert proc.wait(timeout=2)==0
                assert time.monotonic()-start<1.5,'cancelling AI blocked the terminal'
                assert all(not alive(pid) for pid in children),'curl survived application exit'
            elif mode=='timeout':
                wait_for(b'AI request timed out',7);send(b'\x1b');send(b'q');assert proc.wait(timeout=2)==0
            elif mode=='error':
                wait_for(b'AI authentication refused');assert SECRET.encode() not in drain();send(b'\x1b');send(b'q');assert proc.wait(timeout=2)==0
            elif mode=='stale':
                send(b'+New task\r');server.release.set();wait_for(b'Goals changed')
                assert b'Stale suggestion must not appear' not in drain()
                send(b'q');assert proc.wait(timeout=2)==0
            elif auto:
                wait_for(b'AI ');wait_for(b'Provider advice: review');wait_for(b'Open Pomodoro with p.')
                send(b'q');assert proc.wait(timeout=2)==0
            else:
                wait_for(b'Provider advice: review')
                assert len(tomllib.loads(data.read_text())['entries'])==1,'model reply changed task records'
                wait_for_count=len(server.records);send(b'And then?\r')
                end=time.monotonic()+2
                while len(server.records)==wait_for_count and time.monotonic()<end:drain(.05)
                assert 'Provider advice: review' in json.dumps(server.records[-1][2]),'assistant conversation history was not sent'
                drain(.3);count=len(server.records);send(b'task: Saved locally\r')
                assert len(server.records)==count,'explicit task command was sent to AI'
                assert len(tomllib.loads(data.read_text())['entries'])==2
                send(b'\x1b');send(b'q');assert proc.wait(timeout=2)==0
            assert SECRET.encode() not in output,'API credential leaked into terminal output'
            assert SECRET not in config.read_text() and SECRET not in data.read_text(),'API credential persisted'
            assert termios.tcgetattr(slave)==baseline,'terminal not restored after AI'
        finally:
            server.release.set()
            if proc.poll() is None:proc.terminate();proc.wait(timeout=3)
            os.close(master);os.close(slave)
    try:
        for provider in ['openai','anthropic','gemini','ollama','openrouter','compatible']:
            run(provider);print(f'PASS: {provider} native protocol, context/history, local commands and credential handling',flush=True)
        for mode,auto in [('slow',False),('error',False),('timeout',False),('normal',True),('stale',True)]:
            run('ollama',mode,auto);print(f'PASS: AI {mode}, auto={auto}',flush=True)
    finally:server.shutdown();server.server_close()
