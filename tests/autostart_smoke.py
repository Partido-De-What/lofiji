#!/usr/bin/env python3
"""Test installer/idempotence/removal in a temporary profile, never the real session."""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='lofiji-autostart-test-') as tmp:
    home=Path(tmp);config=home/'.config/i3/config';config.parent.mkdir(parents=True)
    config.write_text('set $mod Mod4\n')
    env=os.environ.copy();env['XDG_CURRENT_DESKTOP']='i3'
    cmd=['python3',str(ROOT/'tools/install_autostart.py'),'--binary',str(ROOT/'target/debug/lofiji'),'--home',str(home),'--config-root',str(home/'.config')]
    for _ in range(2):subprocess.run(cmd,env=env,check=True,stdout=subprocess.DEVNULL)
    entry=home/'.config/autostart/lofiji.desktop';fragment=home/'.config/i3/lofiji-autostart.conf'
    assert (home/'.local/bin/lofiji').is_file() and os.access(home/'.local/bin/lofiji',os.X_OK)
    assert not entry.exists(),'native i3 startup must not create a duplicate desktop autostart entry'
    assert config.read_text().count('# Lofiji login clock')==1
    assert '--title Lofiji' in fragment.read_text() and 'exec --no-startup-id' in fragment.read_text()
    subprocess.run(['i3','-C','-c',str(config)],check=True)
    subprocess.run(cmd+['--disable'],env=env,check=True,stdout=subprocess.DEVNULL)
    assert not entry.exists() and not fragment.exists()
    assert 'Lofiji login clock' not in config.read_text() and 'set $mod Mod4' in config.read_text()
    print('PASS: isolated login installer, duplicate prevention, i3 parse validation, disable cleanup')
