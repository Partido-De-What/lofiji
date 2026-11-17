//! Cached raster scenes. GIF compositing and timing are independent of render frequency.
use crate::{
    config::{Background, Config},
    scene_motion::{self, Motion},
    theme::*,
};
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
#[derive(Clone)]
pub struct Image {
    frames: Arc<Vec<(Raster, u64)>>,
    duration: u64,
    motion: Option<Motion>,
}
impl Image {
    pub fn scene(scene: Background, value: &str, config: &Path) -> Result<Self, String> {
        static WINDOW: OnceLock<Result<Image, String>> = OnceLock::new();
        static CITY: OnceLock<Result<Image, String>> = OnceLock::new();
        static FOREST: OnceLock<Result<Image, String>> = OnceLock::new();
        static SNOW: OnceLock<Result<Image, String>> = OnceLock::new();
        static BONFIRE: OnceLock<Result<Image, String>> = OnceLock::new();
        let (cache, bytes): (&OnceLock<Result<Image, String>>, &[u8]) = match scene {
            Background::Window => (
                &WINDOW,
                include_bytes!("../assets/wallpapers/rainy-window.gif"),
            ),
            Background::City => (&CITY, include_bytes!("../assets/wallpapers/city.png")),
            Background::Forest => (&FOREST, include_bytes!("../assets/wallpapers/forest.png")),
            Background::Snow => (&SNOW, include_bytes!("../assets/wallpapers/snow.png")),
            Background::Bonfire => (&BONFIRE, include_bytes!("../assets/wallpapers/bonfire.png")),
            Background::Image => return Self::load(value, config),
        };
        cache
            .get_or_init(|| {
                Self::decode(bytes).map(|mut image| {
                    image.motion = Motion::for_scene(scene);
                    image
                })
            })
            .clone()
    }
    pub fn load(value: &str, config_path: &Path) -> Result<Self, String> {
        if value.is_empty() {
            return Self::scene(Background::Window, "", config_path);
        }
        let path = if let Some(rest) = value.strip_prefix("~/") {
            PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?).join(rest)
        } else {
            let p = PathBuf::from(value);
            if p.is_absolute() {
                p
            } else {
                config_path.parent().unwrap_or(Path::new(".")).join(p)
            }
        };
        let file = std::fs::File::open(path).map_err(|e| format!("Cannot open image: {e}"))?;
        let mut bytes = Vec::new();
        file.take((MEMORY_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > MEMORY_LIMIT {
            return Err("Image file exceeds 64 MiB".into());
        }
        Self::decode(&bytes)
    }
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.starts_with(b"GIF8") {
            return Self::gif(bytes);
        }
        let image = Raster::decode(Cursor::new(bytes))?;
        Ok(Self {
            frames: Arc::new(vec![(image, 1000)]),
            duration: 1000,
            motion: None,
        })
    }
    fn gif(bytes: &[u8]) -> Result<Self, String> {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        options.set_memory_limit(gif::MemoryLimit(MEMORY_LIMIT as u32));
        options.check_frame_consistency(true);
        let mut reader = options
            .read_info(Cursor::new(bytes))
            .map_err(|e| format!("Invalid GIF: {e}"))?;
        let (width, height) = (u32::from(reader.width()), u32::from(reader.height()));
        let size = width as usize * height as usize * 4;
        if size == 0 || size > MEMORY_LIMIT / 3 {
            return Err("GIF canvas exceeds memory limit".into());
        }
        let background = reader
            .bg_color()
            .and_then(|index| {
                reader
                    .global_palette()
                    .and_then(|p| p.get(index * 3..index * 3 + 3))
            })
            .map(|p| [p[0], p[1], p[2], 255])
            .unwrap_or([0; 4]);
        let mut canvas = vec![0; size];
        let mut frames = Vec::new();
        let mut duration = 0;
        while let Some(frame) = reader
            .read_next_frame()
            .map_err(|e| format!("Cannot decode GIF: {e}"))?
        {
            if (frames.len() + 3) * size > MEMORY_LIMIT || frames.len() >= 512 {
                return Err(
                    "Decoded GIF exceeds 64 MiB or 512 frames; use a smaller animation".into(),
                );
            }
            if frames.is_empty() && frame.transparent.is_none() {
                for pixel in canvas.chunks_exact_mut(4) {
                    pixel.copy_from_slice(&background);
                }
            }
            let saved = (frame.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
            for y in 0..usize::from(frame.height) {
                for x in 0..usize::from(frame.width) {
                    let src = (y * usize::from(frame.width) + x) * 4;
                    let dst = ((y + usize::from(frame.top)) * width as usize
                        + x
                        + usize::from(frame.left))
                        * 4;
                    if frame.buffer[src + 3] != 0 {
                        canvas[dst..dst + 4].copy_from_slice(&frame.buffer[src..src + 4]);
                    }
                }
            }
            let delay = if frame.delay < 2 {
                100
            } else {
                u64::from(frame.delay) * 10
            };
            duration += delay;
            frames.push((
                Raster {
                    width,
                    height,
                    rgba: canvas.clone(),
                },
                delay,
            ));
            match frame.dispose {
                gif::DisposalMethod::Previous => canvas = saved.unwrap(),
                gif::DisposalMethod::Background => {
                    for y in usize::from(frame.top)..usize::from(frame.top + frame.height) {
                        let start = (y * width as usize + usize::from(frame.left)) * 4;
                        let clear = if frame.transparent.is_some() {
                            [0; 4]
                        } else {
                            background
                        };
                        for pixel in
                            canvas[start..start + usize::from(frame.width) * 4].chunks_exact_mut(4)
                        {
                            pixel.copy_from_slice(&clear);
                        }
                    }
                }
                _ => {}
            }
        }
        if frames.is_empty() {
            return Err("GIF contains no frames".into());
        }
        Ok(Self {
            frames: Arc::new(frames),
            duration,
            motion: None,
        })
    }
    fn frame_index(&self, elapsed_ms: u64) -> usize {
        let mut position = elapsed_ms % self.duration;
        for (index, (_, delay)) in self.frames.iter().enumerate() {
            if position < *delay {
                return index;
            }
            position -= delay;
        }
        0
    }
    pub fn animated(&self) -> bool {
        self.frames.len() > 1 || self.motion.is_some()
    }
    pub fn draw_at(
        &self,
        buffer: &mut Buffer,
        brightness: u8,
        protected: &[Rect],
        elapsed_ms: u64,
    ) {
        let raster = &self.frames[self.frame_index(elapsed_ms)].0;
        let motion = self
            .motion
            .map(|kind| scene_motion::Frame::new(kind, raster.width, raster.height, elapsed_ms));
        raster.draw_motion(buffer, brightness, protected, motion.as_ref());
    }
}
pub fn weather(buffer: &mut Buffer, config: &Config, frame: u64, protected: &[Rect]) {
    crate::weather::particles(buffer, config.weather, frame, protected, None);
}
struct Raster {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}
impl Raster {
    fn decode(reader: impl Read) -> Result<Self, String> {
        let mut decoder = png::Decoder::new(reader);
        decoder.set_limits(png::Limits {
            bytes: 64 * 1024 * 1024,
        });
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("Invalid PNG: {e}"))?;
        let info = reader.info();
        if info.width == 0
            || info.height == 0
            || u64::from(info.width) * u64::from(info.height) > 16_000_000
        {
            return Err("PNG must contain at most 16 megapixels".into());
        }
        let mut bytes = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut bytes)
            .map_err(|e| format!("Cannot decode PNG: {e}"))?;
        let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
        for p in bytes[..info.buffer_size()].chunks(info.color_type.samples()) {
            match info.color_type {
                png::ColorType::Rgb => rgba.extend_from_slice(&[p[0], p[1], p[2], 255]),
                png::ColorType::Rgba => rgba.extend_from_slice(p),
                png::ColorType::Grayscale => rgba.extend_from_slice(&[p[0], p[0], p[0], 255]),
                png::ColorType::GrayscaleAlpha => rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]),
                _ => return Err("Unsupported PNG color format".into()),
            }
        }
        Ok(Self {
            width: info.width,
            height: info.height,
            rgba,
        })
    }
    #[cfg(test)]
    pub fn draw(&self, buffer: &mut Buffer, brightness: u8, protected: &[Rect]) {
        self.draw_motion(buffer, brightness, protected, None);
    }
    fn draw_motion(
        &self,
        buffer: &mut Buffer,
        brightness: u8,
        protected: &[Rect],
        motion: Option<&scene_motion::Frame>,
    ) {
        let area = buffer.area;
        if area.width == 0 || area.height == 0 {
            return;
        }
        let target_w = u32::from(area.width);
        let target_h = u32::from(area.height) * 2;
        // Cover crop, preserving square half-block pixels and source aspect ratio.
        let scale = (f64::from(self.width) / f64::from(target_w))
            .min(f64::from(self.height) / f64::from(target_h));
        let crop_x = (f64::from(self.width) - f64::from(target_w) * scale) / 2.0;
        let crop_y = (f64::from(self.height) - f64::from(target_h) * scale) / 2.0;
        let sample = |x: u32, y: u32| {
            let sx = ((f64::from(x) + 0.5) * scale + crop_x) as u32;
            let sy = ((f64::from(y) + 0.5) * scale + crop_y) as u32;
            let at = ((sy.min(self.height - 1) * self.width + sx.min(self.width - 1)) * 4) as usize;
            let p = if let Some(motion) = motion {
                motion.sample(
                    &self.rgba,
                    self.width,
                    self.height,
                    sx.min(self.width - 1),
                    sy.min(self.height - 1),
                )
            } else {
                [
                    self.rgba[at],
                    self.rgba[at + 1],
                    self.rgba[at + 2],
                    self.rgba[at + 3],
                ]
            };
            let amount = u32::from(brightness) * u32::from(p[3]);
            let mix = |source: u8, base: u8| {
                ((u32::from(source) * amount + u32::from(base) * (25500 - amount)) / 25500) as u8
            };
            Color::Rgb(mix(p[0], 0x1a), mix(p[1], 0x1b), mix(p[2], 0x26))
        };
        for y in 0..area.height {
            for x in 0..area.width {
                let top = sample(u32::from(x), u32::from(y) * 2);
                let bottom = sample(u32::from(x), u32::from(y) * 2 + 1);
                let cell = buffer.get_mut(x, y);
                if protected.iter().any(|r| r.contains((x, y).into())) {
                    cell.set_symbol(" ").set_bg(average(top, bottom));
                } else {
                    cell.set_symbol("▀").set_fg(top).set_bg(bottom);
                }
            }
        }
    }
}
fn average(a: Color, b: Color) -> Color {
    match (a, b) {
        (Color::Rgb(r, g, b), Color::Rgb(nr, ng, nb)) => Color::Rgb(
            ((u16::from(r) + u16::from(nr)) / 2) as u8,
            ((u16::from(g) + u16::from(ng)) / 2) as u8,
            ((u16::from(b) + u16::from(nb)) / 2) as u8,
        ),
        _ => BG,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_png_scene_moves_gently_and_closes_its_loop() {
        for scene in [
            Background::City,
            Background::Forest,
            Background::Snow,
            Background::Bonfire,
        ] {
            let image = Image::scene(scene, "", Path::new("")).unwrap();
            assert!(image.animated());
            let render = |ms| {
                let mut buffer = Buffer::empty(Rect::new(0, 0, 120, 36));
                image.draw_at(&mut buffer, 40, &[], ms);
                buffer
            };
            let first = render(0);
            assert_eq!(first, render(scene_motion::LOOP_MS), "{scene:?} loop seam");
            let changed = [200, 800, 2000, 6000, 12000]
                .into_iter()
                .any(|ms| render(ms) != first);
            assert!(changed, "{scene:?} has no visible movement");
            let changed = first
                .content
                .iter()
                .zip(render(2000).content)
                .filter(|(a, b)| **a != *b)
                .count();
            assert!(
                changed < first.content.len() / 3,
                "{scene:?} motion dominates the scene"
            );
        }
    }
    #[test]
    fn gif_offsets_disposal_delays_and_long_suspensions_are_correct() {
        use std::borrow::Cow;
        let mut bytes = Vec::new();
        {
            let mut encoder = gif::Encoder::new(
                &mut bytes,
                2,
                1,
                &[0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255],
            )
            .unwrap();
            for (left, width, pixels, dispose, delay) in [
                (0, 2, vec![1, 1], gif::DisposalMethod::Keep, 10),
                (0, 1, vec![2], gif::DisposalMethod::Previous, 20),
                (1, 1, vec![3], gif::DisposalMethod::Keep, 30),
                (0, 1, vec![2], gif::DisposalMethod::Background, 10),
                (1, 1, vec![2], gif::DisposalMethod::Keep, 10),
            ] {
                encoder
                    .write_frame(&gif::Frame {
                        left,
                        width,
                        height: 1,
                        buffer: Cow::Owned(pixels),
                        transparent: Some(0),
                        dispose,
                        delay,
                        ..gif::Frame::default()
                    })
                    .unwrap();
            }
        }
        let image = Image::decode(&bytes).unwrap();
        assert_eq!(image.frames[0].0.rgba, [255, 0, 0, 255, 255, 0, 0, 255]);
        assert_eq!(image.frames[1].0.rgba, [0, 255, 0, 255, 255, 0, 0, 255]);
        assert_eq!(image.frames[2].0.rgba, [255, 0, 0, 255, 0, 0, 255, 255]);
        assert_eq!(image.frames[4].0.rgba, [0, 0, 0, 0, 0, 255, 0, 255]);
        for (ms, index) in [
            (0, 0),
            (99, 0),
            (100, 1),
            (299, 1),
            (300, 2),
            (799, 4),
            (800, 0),
            (80_000_000_100, 1),
        ] {
            assert_eq!(image.frame_index(ms), index);
        }
        assert!(Image::decode(b"GIF89a broken").is_err());
    }
    #[test]
    fn bundled_wallpaper_loads_without_a_local_path() {
        let image = Image::load("", Path::new("/missing/config.toml")).unwrap();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
        image.frames[0].0.draw(&mut buffer, 30, &[]);
        assert!(buffer.content.iter().any(|cell| cell.bg != BG));
    }
    #[test]
    fn png_alpha_scaling_and_protected_clock_cells_are_correct() {
        let path = std::env::temp_dir().join(format!("lofiji-image-{}.png", std::process::id()));
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&[255, 0, 0, 255, 255, 0, 0, 0, 0, 0, 255, 255, 0, 0, 255, 0])
                .unwrap();
        }
        std::fs::write(&path, bytes).unwrap();
        let image = Image::load(path.to_str().unwrap(), Path::new("")).unwrap();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
        image.frames[0].0.draw(&mut buffer, 100, &[]);
        assert_eq!(buffer.get(0, 0).fg, Color::Rgb(255, 0, 0));
        assert_eq!(buffer.get(0, 0).bg, Color::Rgb(0, 0, 255));
        assert_eq!(buffer.get(1, 0).fg, BG);
        image.frames[0]
            .0
            .draw(&mut buffer, 100, &[Rect::new(0, 0, 1, 1)]);
        assert_eq!(buffer.get(0, 0).symbol(), " ");
        assert_eq!(buffer.get(0, 0).bg, Color::Rgb(127, 0, 127));
        image.frames[0].0.draw(&mut buffer, 0, &[]);
        assert!(buffer.content.iter().all(|c| c.fg == BG && c.bg == BG));
        std::fs::write(&path, b"invalid").unwrap();
        assert!(Image::load(path.to_str().unwrap(), Path::new("")).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
