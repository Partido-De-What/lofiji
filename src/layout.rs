//! Keep the clock centered horizontally, reserving space for the text workspace.
use crate::clock;
use ratatui::layout::Rect;

pub struct Composition {
    pub header: Option<Rect>,
    pub clock: Rect,
    pub scale: Option<u16>,
    pub date: Option<Rect>,
    pub timer: Option<Rect>,
    pub shared_date_timer: bool,
    pub progress: Option<Rect>,
}
impl Composition {
    pub fn new(size: Rect, time: &str) -> Self {
        Self::with_workspace(size, time, true)
    }
    pub fn with_workspace(size: Rect, time: &str, workspace: bool) -> Self {
        let (w, h) = (size.width, size.height);
        let body_end = if h >= 6 { h - 3 } else { h.saturating_sub(2) };
        let header = (w >= 40 && h >= 12).then(|| {
            Rect::new(
                if w >= 70 { 3 } else { 1 },
                1,
                w - if w >= 70 { 6 } else { 2 },
                1,
            )
        });
        let full = h >= 24 && w >= clock::pixel_width(time, 1) + 4;
        let compact = !full && h >= 16 && w >= clock::compact_width(time, 1) + 4;
        let scale = if full {
            Some(if w >= clock::pixel_width(time, 2) + 4 {
                2
            } else {
                1
            })
        } else if compact {
            Some(if w >= 80 { 2 } else { 1 })
        } else {
            None
        };
        let clock_height = if full {
            7
        } else if compact {
            5
        } else {
            1
        };
        let top = if workspace && w >= 40 && h >= 17 {
            (h / 10).clamp(3, 6)
        } else {
            h.saturating_sub(clock_height) / 2
        };
        let width = if full {
            clock::pixel_width(time, scale.unwrap())
        } else if compact {
            clock::compact_width(time, scale.unwrap())
        } else {
            w
        };
        let clock = Rect::new((w - width) / 2, top, width, clock_height);
        let date_y = clock.bottom() + u16::from(full);
        let timer_y = date_y + if full && h >= 30 { 2 } else { 1 };
        let date = (date_y < body_end).then_some(Rect::new(0, date_y, w, 1));
        let timer_y = if timer_y < body_end && h >= 8 {
            timer_y
        } else {
            date_y
        };
        let timer = (timer_y < body_end).then_some(Rect::new(0, timer_y, w, 1));
        let progress = (full && h >= 30 && timer_y + 5 < body_end).then(|| {
            let width = w.min(28);
            Rect::new((w - width) / 2, timer_y + 2, width, 1)
        });
        Self {
            header,
            clock,
            scale,
            date,
            timer,
            shared_date_timer: timer_y == date_y,
            progress,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn centered_clock_content_never_overflows() {
        for time in ["08:48:08", "08:48"] {
            for w in 1..=180 {
                for h in 1..=50 {
                    let layout = Composition::new(Rect::new(0, 0, w, h), time);
                    assert!(
                        (i32::from(layout.clock.x * 2 + layout.clock.width) - i32::from(w)).abs()
                            <= 1
                    );
                    if w >= 40 && h >= 17 {
                        assert!(layout.clock.bottom() <= h / 2 + 3);
                    } else {
                        assert!(
                            (i32::from(layout.clock.y * 2 + layout.clock.height) - i32::from(h))
                                .abs()
                                <= 1
                        );
                    }
                    for item in [
                        Some(layout.clock),
                        layout.date,
                        layout.timer,
                        layout.progress,
                    ]
                    .into_iter()
                    .flatten()
                    {
                        assert!(item.right() <= w && item.bottom() <= h);
                    }
                }
            }
        }
    }
}
