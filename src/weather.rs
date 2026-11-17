//! Sparse foreground weather; functional clock cells are protected.
use crate::{config::Weather, theme::*};
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
pub fn particles(
    buffer: &mut Buffer,
    mode: Weather,
    frame: u64,
    protected: &[Rect],
    clip: Option<Rect>,
) {
    let area = buffer.area;
    if mode == Weather::Clear || area.width < 12 || area.height < 7 {
        return;
    }
    let field = clip.unwrap_or(Rect::new(0, 0, area.width, area.height - 3));
    if field.width == 0 || field.height == 0 {
        return;
    }
    let count = (u64::from(field.width) * u64::from(field.height) / 85).clamp(3, 48);
    for i in 0..count {
        let seed = scatter(i + 401);
        let near = i % 3 == 0;
        let period = u64::from(field.height) + 8;
        let speed = if mode == Weather::Snow {
            frame / 3
        } else {
            frame * (1 + u64::from(near))
        };
        let phase = (speed + seed % period) % period;
        let wind = if mode == Weather::Snow {
            frame / 16
        } else {
            phase / 6
        };
        let x = field.x + ((seed / 17 + wind) % u64::from(field.width)) as u16;
        if phase < u64::from(field.height) {
            let y = field.y + phase as u16;
            let symbol = if mode == Weather::Snow {
                "·"
            } else if near {
                "╲"
            } else {
                "╎"
            };
            let color = if mode == Weather::Snow {
                MUTED
            } else if near {
                Color::Rgb(0x69, 0x79, 0x9c)
            } else {
                Color::Rgb(0x40, 0x4c, 0x68)
            };
            if !protected.iter().any(|r| r.contains((x, y).into())) {
                buffer.get_mut(x, y).set_symbol(symbol).set_fg(color);
            }
            if mode == Weather::Rain && near && phase > 0 && x > field.x {
                let (tx, ty) = (x - 1, y - 1);
                if !protected.iter().any(|r| r.contains((tx, ty).into())) {
                    buffer.get_mut(tx, ty).set_symbol("╲").set_fg(DIM);
                }
            }
        } else if mode == Weather::Rain
            && near
            && (phase == u64::from(field.height) + 1 || phase == u64::from(field.height) + 2)
        {
            let y = field.bottom() - 1;
            if !protected.iter().any(|r| r.contains((x, y).into())) {
                buffer.get_mut(x, y).set_symbol("·").set_fg(MUTED);
            }
        }
    }
}
fn scatter(mut seed: u64) -> u64 {
    seed ^= seed >> 30;
    seed = seed.wrapping_mul(0xbf58476d1ce4e5b9);
    seed ^= seed >> 27;
    seed = seed.wrapping_mul(0x94d049bb133111eb);
    seed ^ (seed >> 31)
}
