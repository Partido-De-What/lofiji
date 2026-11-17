#!/usr/bin/env python3
"""Export Lofiji scene and clock previews. Dev-only dependencies: Pillow and rsvg-convert."""
import argparse
import io
from pathlib import Path
import subprocess
import tempfile
from PIL import Image, ImageDraw, ImageFont
from preview_clock_styles import export as export_styles

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/lofiji')
    parser.add_argument('--output', type=Path, default=ROOT/'docs')
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='lofiji-previews-') as tmp:
        def render(scene='window', mode='clock', weather='clear', size='120x36', elapsed=8, style='pixel'):
            path=Path(tmp)/'frame.svg'
            config=Path(tmp)/'config.toml'
            config.write_text((ROOT/'config.example.toml').read_text().replace('clock_style = \"pixel\"',f'clock_style = \"{style}\"'))
            subprocess.run([str(args.binary),'--config',str(config),'--snapshot',size,'--at','2026-11-16T22:48:07','--mode',mode,'--background',scene,'--weather',weather,'--elapsed',str(elapsed),'--output',str(path)],check=True)
            return Image.open(io.BytesIO(subprocess.check_output(['rsvg-convert',str(path)]))).convert('RGB')
        names=[('window','Rainy window · GIF'),('city','Moonlit city'),('forest','Quiet forest'),('snow','Snow mountains'),('bonfire','Bonfire'),('image','Custom PNG / GIF')]
        gallery=Image.new('RGB',(1200,1170),'#1a1b26')
        font=ImageFont.truetype(subprocess.check_output(['fc-match','-f','%{file}','DejaVu Sans Mono'],text=True),15)
        for i,(scene,label) in enumerate(names):
            image=render(scene,weather='clear')
            image.save(args.output/f'{scene}.png')
            x=(i%2)*600;y=(i//2)*390
            gallery.paste(image.resize((600,360),Image.Resampling.NEAREST),(x,y+25))
            ImageDraw.Draw(gallery).text((x+12,y+4),label,font=font,fill='#c0caf5')
        gallery.save(args.output/'scenes.png')
        motion=[]
        for t in range(48):
            grid=Image.new('RGB',(960,624),'#1a1b26')
            for i,(scene,label) in enumerate(names[1:5]):
                frame=render(scene,elapsed=t/2).resize((480,288),Image.Resampling.NEAREST)
                x=i%2*480;y=i//2*312
                grid.paste(frame,(x,y+24))
                ImageDraw.Draw(grid).text((x+10,y+3),label,font=font,fill='#c0caf5')
            motion.append(grid)
        motion[0].save(args.output/'scene-motion.gif',save_all=True,append_images=motion[1:],duration=500,loop=0,disposal=1)

        export_styles(args.binary.resolve(), args.output)
        render(mode='focus').save(args.output/'pomodoro.png')
        render(size='60x17').save(args.output/'compact.png')
        render(mode='settings').save(args.output/'settings.png')
        render(mode='sounds').save(args.output/'sounds.png')
        render('bonfire',mode='plan').save(args.output/'plan.png')
        render('bonfire',mode='plan',size='80x24').save(args.output/'plan-medium.png')
        render('bonfire',mode='plan',size='60x17').save(args.output/'plan-compact.png')
        render('bonfire',mode='goals').save(args.output/'goals.png')
        render('bonfire',mode='character').save(args.output/'character.png')
        render('bonfire',mode='conversation').save(args.output/'conversation.png')
        render('bonfire',mode='ai_settings').save(args.output/'ai-settings.png')
        frames=[render(weather='rain',elapsed=t/4) for t in range(32)]
        frames[0].save(args.output/'rain.gif',save_all=True,append_images=frames[1:],duration=250,loop=0,disposal=2)
    print('Exported scenes, styles, planning, settings, Pomodoro and rain.')

if __name__=='__main__':main()
