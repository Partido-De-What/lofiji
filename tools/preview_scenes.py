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
        def render(scene='window', mode='clock', weather='clear', size='120x36', elapsed=8,
                   style='pixel', palette=0, workspace=True, brightness=30,
                   opacity=75, volumes=None):
            path=Path(tmp)/'frame.svg'
            config=Path(tmp)/'config.toml'
            example = (ROOT/'config.example.toml').read_text()
            config_text = example.replace('clock_style = "pixel"', f'clock_style = "{style}"')
            config_text = config_text.replace('clock_palette = 0', f'clock_palette = {palette}')
            config_text = config_text.replace('show_plan = true', f'show_plan = {str(workspace).lower()}')
            config_text = config_text.replace('image_brightness = 30', f'image_brightness = {brightness}')
            config_text = config_text.replace('dashboard_opacity = 75', f'dashboard_opacity = {opacity}')
            if volumes is not None:
                start = config_text.index('sound_volumes = [')
                end = config_text.index(']', start)
                config_text = config_text[:start] + f'sound_volumes = {volumes}' + config_text[end + 1:]
            config.write_text(config_text)
            subprocess.run([str(args.binary),'--config',str(config),'--snapshot',size,'--at','2026-11-16T22:48:07','--mode',mode,'--background',scene,'--weather',weather,'--elapsed',str(elapsed),'--output',str(path)],check=True)
            return Image.open(io.BytesIO(subprocess.check_output(['rsvg-convert',str(path)]))).convert('RGB')
        names=[('window','Rainy window · GIF'),('city','Moonlit city'),('forest','Quiet forest'),('snow','Snow mountains'),('bonfire','Bonfire'),('rain-room','Rain Room')]
        gallery=Image.new('RGB',(1200,((len(names)+1)//2)*390),'#1a1b26')
        font=ImageFont.truetype(subprocess.check_output(['fc-match','-f','%{file}','DejaVu Sans Mono'],text=True),15)
        for i,(scene,label) in enumerate(names):
            image=render(scene, style='round' if scene == 'rain-room' else 'pixel',
                         workspace=False, brightness=60 if scene == 'rain-room' else 50)
            image.save(args.output/f'{scene}.png')
            x=(i%2)*600;y=(i//2)*390
            gallery.paste(image.resize((600,360),Image.Resampling.NEAREST),(x,y+25))
            ImageDraw.Draw(gallery).text((x+12,y+4),label,font=font,fill='#c0caf5')
        gallery.save(args.output/'scenes.png')
        render('rain-room', style='digital', palette=7, workspace=False,
               brightness=60).save(args.output/'rain-room-glass.png')
        render('image').save(args.output/'image.png')
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
        render('city', mode='focus', workspace=False, brightness=48,
               style='digital', elapsed=537).save(args.output/'pomodoro.png')
        render(size='60x17').save(args.output/'compact.png')
        render(mode='settings').save(args.output/'settings.png')
        render('rain-room', mode='sounds', brightness=55,
               volumes=[0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 28]).save(args.output/'sounds.png')
        render('rain-room', mode='plan', style='round', brightness=56,
               opacity=62).save(args.output/'plan.png')
        render('bonfire',mode='plan',size='80x24').save(args.output/'plan-medium.png')
        render('bonfire',mode='plan',size='60x17').save(args.output/'plan-compact.png')
        render('bonfire',mode='goals').save(args.output/'goals.png')
        render('bonfire',mode='character').save(args.output/'character.png')
        render('bonfire', mode='conversation', brightness=48).save(args.output/'conversation.png')
        render('bonfire',mode='ai_settings').save(args.output/'ai-settings.png')
        frames=[render(weather='rain',elapsed=t/4) for t in range(32)]
        frames[0].save(args.output/'rain.gif',save_all=True,append_images=frames[1:],duration=250,loop=0,disposal=2)
    print('Exported scenes, styles, planning, settings, Pomodoro and rain.')

if __name__=='__main__':main()
