#!/usr/bin/env python3
"""Audit every clock face with the live renderer. Needs Pillow and rsvg-convert."""
import argparse
import io
from pathlib import Path
import subprocess
import tempfile
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
STYLES = [('pixel', 'Pixel'), ('round', 'Rounded'), ('digital', 'Seven segment'),
          ('dots', 'Dot matrix'), ('outline', 'Outline'), ('block', 'Block'),
          ('slant', 'Slanted'), ('scanline', 'Scanlines')]


def export(binary, output):
    output.mkdir(parents=True, exist_ok=True)
    font = ImageFont.truetype(subprocess.check_output(
        ['fc-match', '-f', '%{file}', 'DejaVu Sans Mono'], text=True), 16)
    gallery = Image.new('RGB', (1200, ((len(STYLES)+1)//2)*390), '#1a1b26')
    sheets = {False: Image.new('RGB', (1200, len(STYLES)*150), '#1a1b26'),
              True: Image.new('RGB', (1200, len(STYLES)*150), '#1a1b26')}
    with tempfile.TemporaryDirectory(prefix='lofiji-type-') as tmp:
        tmp = Path(tmp)
        def render(style, stamp, compact=False):
            config = tmp/'config.toml'
            text = (ROOT/'config.example.toml').read_text()
            config.write_text(text.replace('clock_style = "pixel"', f'clock_style = "{style}"')
                              .replace('animations = true', 'animations = false'))
            path = tmp/'frame.svg'
            subprocess.run([str(binary), '--config', str(config), '--snapshot',
                            '80x18' if compact else '120x36', '--background', 'bonfire',
                            '--at', '2026-11-16T'+stamp, '--output', str(path)], check=True)
            return Image.open(io.BytesIO(subprocess.check_output(['rsvg-convert', str(path)]))).convert('RGB')
        for i, (style, label) in enumerate(STYLES):
            image = render(style, '22:48:07')
            x = (i % 2)*600; y = (i//2)*390
            gallery.paste(image.resize((600, 360), Image.Resampling.NEAREST), (x, y+25))
            ImageDraw.Draw(gallery).text((x+12, y+4), label, font=font, fill='#c0caf5')
            for compact, sheet in sheets.items():
                ImageDraw.Draw(sheet).text((12, i*150+4), label, font=font, fill='#c0caf5')
                for j, stamp in enumerate(['12:34:56', '07:48:09']):
                    frame = render(style, stamp, compact)
                    # Ratatui's native SVG uses 10x20 px cells. Both clock
                    # rectangles remain horizontally centered; crop the raised face.
                    crop = (160,60,630,160) if compact else (210,60,990,200)
                    height = 110 if compact else 97
                    face = frame.crop(crop).resize((540,height), Image.Resampling.NEAREST)
                    sheet.paste(face, (j*600+24, i*150+30))
    gallery.save(output/'styles.png')
    sheets[False].save(output/'numerals.png')
    sheets[True].save(output/'numerals-compact.png')
    print('Exported eight styles and full/compact numeral audits.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/lofiji')
    parser.add_argument('--output', type=Path, default=ROOT/'docs')
    args = parser.parse_args()
    export(args.binary.resolve(), args.output)
