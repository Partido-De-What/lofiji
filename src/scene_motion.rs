//! Local motion sampled from the original PNGs; geometry and camera stay anchored.
use crate::config::Background;
use std::f64::consts::TAU;

pub const LOOP_MS: u64 = 24_000;
#[derive(Clone, Copy)]
pub enum Motion {
    City,
    Forest,
    Snow,
    Bonfire,
    RainRoom,
}
impl Motion {
    pub fn for_scene(scene: Background) -> Option<Self> {
        match scene {
            Background::City => Some(Self::City),
            Background::Forest => Some(Self::Forest),
            Background::Snow => Some(Self::Snow),
            Background::Bonfire => Some(Self::Bonfire),
            Background::RainRoom => Some(Self::RainRoom),
            _ => None,
        }
    }
}
struct Light {
    x: f64,
    y: f64,
    radius: f64,
    alpha: f64,
    color: [u8; 3],
}
pub struct Frame {
    motion: Motion,
    tick: u64,
    sway: i32,
    cloud_shift: i32,
    levels: [u16; 256],
    lights: Vec<Light>,
}
impl Frame {
    pub fn new(motion: Motion, width: u32, height: u32, elapsed_ms: u64) -> Self {
        let elapsed = elapsed_ms % LOOP_MS;
        let phase = elapsed as f64 / LOOP_MS as f64;
        let mut levels = [256; 256];
        for (i, level) in levels.iter_mut().enumerate() {
            let p = (elapsed / 250 + i as u64 * 13) % 96;
            *level = match p {
                0..=7 => 120 + p as u16 * 17,
                8..=15 => 256 - (p as u16 - 8) * 17,
                _ => 256,
            };
        }
        let count = match motion {
            Motion::Snow => 18,
            Motion::Forest => 5,
            Motion::Bonfire => 3,
            _ => 0,
        };
        let mut lights = Vec::with_capacity(count);
        for i in 0..count {
            let seed = scatter(i as u64 + 915);
            let offset = (seed % 1000) as f64 / 1000.0;
            let p = (phase * (1 + i % 3) as f64 + offset).fract();
            let x = ((seed / 997) % 1000) as f64 / 1000.0;
            let (x, y, alpha, color) = match motion {
                Motion::Snow => (
                    (x + (p * TAU).sin() * 0.025).rem_euclid(1.0),
                    p,
                    0.62,
                    [210, 220, 232],
                ),
                Motion::Forest => (
                    x + (p * TAU).sin() * 0.008,
                    0.65 + offset * 0.23,
                    (p * TAU).sin().max(0.0) * 0.7,
                    [211, 219, 130],
                ),
                Motion::Bonfire => (
                    0.738 + (p * TAU).sin() * 0.022,
                    0.78 - p * 0.20,
                    (1.0 - p) * 0.85,
                    [238, 162, 85],
                ),
                _ => unreachable!(),
            };
            lights.push(Light {
                x: x * width as f64,
                y: y * height as f64,
                radius: (width as f64 / 160.0).max(2.0),
                alpha,
                color,
            });
        }
        Self {
            motion,
            tick: elapsed / 200,
            sway: ((phase * TAU * 3.0).sin() * 1.5).round() as i32,
            cloud_shift: ((phase * TAU).sin() * 4.0).round() as i32,
            levels,
            lights,
        }
    }
    pub fn sample(&self, pixels: &[u8], width: u32, height: u32, x: u32, y: u32) -> [u8; 4] {
        let at = |sx: i32, sy: i32| {
            let sx = sx.clamp(0, width as i32 - 1) as u32;
            let sy = sy.clamp(0, height as i32 - 1) as u32;
            let p = ((sy * width + sx) * 4) as usize;
            [pixels[p], pixels[p + 1], pixels[p + 2], pixels[p + 3]]
        };
        let mut color = at(x as i32, y as i32);
        let fx = x as f64 / width as f64;
        let fy = y as f64 / height as f64;
        match self.motion {
            Motion::City => {
                // Individual light clusters change slowly; walls and moon never pulse.
                let window = fy > 0.35
                    && u16::from(color[0]) + u16::from(color[1]) + u16::from(color[2]) > 275;
                let star = fy < 0.31
                    && color[0] > 100
                    && color[1] > 100
                    && !(fx > 0.15 && fx < 0.30 && fy > 0.06);
                if window || star {
                    let seed = scatter(u64::from(x / 5) + u64::from(y / 7) * 173);
                    gain(&mut color, self.levels[seed as usize % 256]);
                }
            }
            Motion::Forest | Motion::Snow => {
                // Move only cloud-colored pixels; trunks, slopes and buildings stay fixed.
                let cloud_area = if matches!(self.motion, Motion::Snow) {
                    fy < 0.23 && fx > 0.26
                } else {
                    fy < 0.29
                };
                if cloud_area {
                    let shifted = at(x as i32 + self.cloud_shift, y as i32);
                    if cloud(color) || (cloud(shifted) && sky(color)) {
                        color = shifted;
                    }
                }
                if matches!(self.motion, Motion::Forest) && fy < 0.18 && foliage(color) {
                    let shifted = at(x as i32 + self.sway, y as i32);
                    if foliage(shifted) {
                        color = shifted;
                    }
                }
            }
            Motion::Bonfire => {
                if (0.69..0.79).contains(&fx) && (0.65..0.81).contains(&fy) {
                    let offsets = [0, 1, 2, 1, 0, -1, -2, -1];
                    let offset = offsets[((self.tick + u64::from(x / 5)) % 8) as usize];
                    let shifted = at(x as i32 + offset / 2, y as i32 + offset);
                    if warm(color) || warm(shifted) {
                        color = shifted;
                    }
                }
                if warm(color) && (0.64..0.86).contains(&fx) && fy > 0.65 {
                    let pulse = [256, 280, 268, 248, 260, 276, 244, 252][(self.tick % 8) as usize];
                    gain(&mut color, pulse);
                }
                // A narrow reflected-light field shimmers without moving the shoreline.
                if (0.46..0.66).contains(&fx) && (0.65..0.84).contains(&fy) {
                    let pulse = [236, 248, 266, 278, 266, 248]
                        [((self.tick + u64::from(y / 2)) % 6) as usize];
                    gain(&mut color, pulse);
                }
            }
            Motion::RainRoom => {
                // Flicker the flame and its spill on the hearth, away from the clock.
                if (0.048..0.205).contains(&fx) && (0.59..0.84).contains(&fy) && warm(color) {
                    let pulse = [256, 268, 261, 246, 255, 265, 250, 254][(self.tick % 8) as usize];
                    gain(&mut color, pulse);
                }
            }
        }
        for light in &self.lights {
            let distance = (x as f64 - light.x).abs() + (y as f64 - light.y).abs();
            if distance < light.radius {
                let alpha = light.alpha * (1.0 - distance / light.radius);
                for (channel, target) in color[..3].iter_mut().zip(light.color) {
                    *channel = (*channel as f64 * (1.0 - alpha) + target as f64 * alpha) as u8;
                }
            }
        }
        color
    }
}
fn gain(color: &mut [u8; 4], level: u16) {
    for channel in &mut color[..3] {
        *channel = (u32::from(*channel) * u32::from(level) / 256).min(255) as u8;
    }
}
fn cloud(c: [u8; 4]) -> bool {
    c[0] > 155 && c[1] > 165 && c[2] > 180 && c[0].abs_diff(c[1]) < 45
}
fn sky(c: [u8; 4]) -> bool {
    c[2] > c[0] && c[1] > c[0] && c[2] > 100
}
fn foliage(c: [u8; 4]) -> bool {
    u16::from(c[1]) > u16::from(c[0]) * 6 / 5 && c[1] > c[2]
}
fn warm(c: [u8; 4]) -> bool {
    c[0] > 90
        && u16::from(c[0]) > u16::from(c[1]) * 6 / 5
        && u16::from(c[1]) > u16::from(c[2]) * 6 / 5
}
fn scatter(mut n: u64) -> u64 {
    n ^= n >> 30;
    n = n.wrapping_mul(0xbf58476d1ce4e5b9);
    n ^= n >> 27;
    n = n.wrapping_mul(0x94d049bb133111eb);
    n ^ (n >> 31)
}
