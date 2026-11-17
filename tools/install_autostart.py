#!/usr/bin/env python3
"""Install the built Lofiji binary and a desktop-login autostart entry."""
import argparse
import os
import shlex
import json
import subprocess
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]


def write_atomic(path, text, check_i3=False):
    temporary=path.with_name(f'.{path.name}.lofiji-{os.getpid()}.tmp')
    try:
        temporary.write_text(text)
        temporary.chmod(path.stat().st_mode & 0o777 if path.exists() else 0o644)
        if check_i3 and shutil.which('i3'):
            result=subprocess.run(['i3','-C','-c',str(temporary)],capture_output=True,text=True)
            if result.returncode:
                raise SystemExit('i3 configuration validation failed: '+result.stderr[-1500:])
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/lofiji')
    parser.add_argument('--disable', action='store_true', help='Remove the autostart entry; retain the installed app')
    parser.add_argument('--home',type=Path,default=Path.home(),help=argparse.SUPPRESS)
    parser.add_argument('--config-root',type=Path,help=argparse.SUPPRESS)
    args = parser.parse_args()
    home=args.home.resolve()
    root = Path(args.config_root or os.environ.get('XDG_CONFIG_HOME') or home/'.config')
    if not root.is_absolute():
        raise SystemExit('XDG_CONFIG_HOME must be an absolute path')
    entry = root/'autostart/lofiji.desktop'
    fragment=home/'.config/i3/lofiji-autostart.conf'
    i3_candidates=[home/'.config/i3/config',home/'.i3/config']
    marker='# Lofiji login clock (managed by tools/install_autostart.py)'
    include='include '+json.dumps(str(fragment))
    if args.disable:
        for config in i3_candidates:
            if config.exists():
                text=config.read_text()
                updated=text.replace(marker+'\n'+include+'\n','')
                if updated!=text:write_atomic(config,updated,check_i3=True)
        if fragment.exists() and marker in fragment.read_text():fragment.unlink()
        if entry.exists():
            if 'X-Lofiji-Autostart=true' not in entry.read_text():
                raise SystemExit(f'{entry} is not managed by this installer')
            entry.unlink()
        print(f'Autostart disabled: {entry}')
        return
    source = args.binary.resolve()
    if not source.is_file():
        raise SystemExit('Build first with cargo build --release --locked')
    terminal = shutil.which('kitty')
    if not terminal:
        raise SystemExit('Kitty is required by this launcher; install it before enabling autostart')
    destination = home/'.local/bin/lofiji'
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f'.lofiji-install-{os.getpid()}')
    try:
        shutil.copyfile(source, temporary)
        temporary.chmod(0o755)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)
    def quote(value):
        return '"'+str(value).replace('\\','\\\\').replace('"','\\"').replace('`','\\`').replace('$','\\$').replace('%','%%')+'"'
    desktop=os.environ.get('XDG_CURRENT_DESKTOP','').lower().split(':')
    direct_i3='i3' in desktop or (not any(desktop) and fragment.is_file() and marker in fragment.read_text())
    if direct_i3:
        config=next((path for path in i3_candidates if path.is_file()),None)
        if config is None:raise SystemExit('i3 config not found; cannot enable login startup')
        fragment.parent.mkdir(parents=True,exist_ok=True)
        command=shlex.join([terminal,'--class','lofiji','--title','Lofiji',str(destination)])
        write_atomic(fragment,marker+'\nexec --no-startup-id '+json.dumps(command)+'\n')
        text=config.read_text()
        if include not in text:
            backup=config.with_name(config.name+'.before-lofiji')
            if not backup.exists():shutil.copy2(config,backup)
            write_atomic(config,text.rstrip()+'\n\n'+marker+'\n'+include+'\n',check_i3=True)
    if direct_i3:
        if entry.exists() and 'X-Lofiji-Autostart=true' in entry.read_text():
            entry.unlink()
        print(f'Installed: {destination}\nStarts at i3 login: {fragment}')
        return
    contents = '\n'.join([
        '[Desktop Entry]', 'Type=Application', 'Name=Lofiji',
        'Comment=A quiet clock and a daily next step',
        'Exec='+quote(terminal)+' --class lofiji --title Lofiji '+quote(destination),
        'TryExec='+str(terminal), 'Terminal=false', 'StartupNotify=false',
        'StartupWMClass=lofiji', 'X-Lofiji-Autostart=true',
        '',
    ])
    entry.parent.mkdir(parents=True, exist_ok=True)
    if entry.exists() and 'X-Lofiji-Autostart=true' not in entry.read_text():
        backup = entry.with_name('lofiji.desktop.bak')
        if backup.exists():
            raise SystemExit(f'Review existing {entry} and {backup} before installing')
        shutil.copy2(entry, backup)
    write_atomic(entry,contents)
    entry.chmod(0o644)
    print(f'Installed: {destination}\nStarts at desktop login: {fragment if direct_i3 else entry}')


if __name__ == '__main__':
    main()
