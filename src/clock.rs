use crate::config::Config;
use chrono::{DateTime, Local, Timelike};

pub fn time(now: DateTime<Local>, c: &Config) -> String {
    now.format(match (c.format_24h, c.show_seconds) {
        (true, true) => "%H:%M:%S",
        (true, false) => "%H:%M",
        (false, true) => "%I:%M:%S",
        (false, false) => "%I:%M",
    })
    .to_string()
}
pub fn date(now: DateTime<Local>, short: bool) -> String {
    now.format(if short {
        "%a · %b %-d"
    } else {
        "%A · %B %-d"
    })
    .to_string()
    .to_uppercase()
}
pub fn meridiem(now: DateTime<Local>) -> &'static str {
    if now.hour() < 12 {
        "AM"
    } else {
        "PM"
    }
}

// Five-by-seven pixel numerals, with dedicated compact faces for short windows.
pub const DIGITS: [[&str; 7]; 10] = [
    [
        "01110", "11011", "11011", "11011", "11011", "11011", "01110",
    ],
    [
        "00110", "01110", "00110", "00110", "00110", "00110", "01111",
    ],
    [
        "01110", "11011", "00011", "00110", "01100", "11000", "11111",
    ],
    [
        "11110", "00011", "00011", "01110", "00011", "00011", "11110",
    ],
    [
        "00011", "00111", "01111", "11011", "11111", "00011", "00011",
    ],
    [
        "11111", "11000", "11000", "11110", "00011", "00011", "11110",
    ],
    [
        "01110", "11000", "11000", "11110", "11011", "11011", "01110",
    ],
    [
        "11111", "00011", "00011", "00110", "01100", "01100", "01100",
    ],
    [
        "01110", "11011", "11011", "01110", "11011", "11011", "01110",
    ],
    [
        "01110", "11011", "11011", "01111", "00011", "00011", "01110",
    ],
];
// Compact five-row numerals preserve the clock's identity in shorter windows.
pub const COMPACT_DIGITS: [[&str; 5]; 10] = [
    ["111", "101", "101", "101", "111"],
    ["010", "110", "010", "010", "111"],
    ["110", "001", "010", "100", "111"],
    ["110", "001", "010", "001", "110"],
    ["101", "101", "111", "001", "001"],
    ["111", "100", "110", "001", "110"],
    ["011", "100", "111", "101", "111"],
    ["111", "001", "010", "010", "010"],
    ["111", "101", "111", "101", "111"],
    ["111", "101", "111", "001", "110"],
];
pub fn compact_width(time: &str, scale: u16) -> u16 {
    time.chars()
        .map(|c| (if c == ':' { scale } else { 3 * scale }) + 1)
        .sum::<u16>()
        .saturating_sub(1)
}
pub fn pixel_width(time: &str, scale: u16) -> u16 {
    time.chars()
        .map(|c| if c == ':' { scale + 2 } else { 5 * scale + 2 })
        .sum::<u16>()
        .saturating_sub(2)
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn seven_segment_uprights_and_crossbars_stay_connected() {
        for compact in [false, true] {
            let (w, h) = if compact { (3, 5) } else { (5, 7) };
            for y in 0..h {
                for digit in [0, 1, 4, 7] {
                    assert!(
                        digital_pixel(digit, w - 1, y, compact),
                        "right upright: {digit} row {y}"
                    );
                }
                assert!(digital_pixel(0, 0, y, compact));
            }
            for x in 0..w {
                assert!(digital_pixel(4, x, h / 2, compact), "4 crossbar");
            }
            assert!(!digital_pixel(4, w / 2, 0, compact), "4 has no top bar");
            assert!(
                !digital_pixel(0, w / 2, h / 2, compact),
                "0 has no middle bar"
            );
            for digit in 0..10 {
                let lit = (0..h)
                    .flat_map(|y| (0..w).map(move |x| (x, y)))
                    .filter(|(x, y)| digital_pixel(digit, *x, *y, compact))
                    .collect::<Vec<_>>();
                let mut pending = vec![lit[0]];
                let mut visited = Vec::new();
                while let Some((x, y)) = pending.pop() {
                    if visited.contains(&(x, y)) {
                        continue;
                    }
                    visited.push((x, y));
                    for &(nx, ny) in &lit {
                        if x.abs_diff(nx) + y.abs_diff(ny) == 1 && !visited.contains(&(nx, ny)) {
                            pending.push((nx, ny));
                        }
                    }
                }
                assert_eq!(
                    visited.len(),
                    lit.len(),
                    "disconnected digit {digit} compact={compact}"
                );
            }
        }
    }
    #[test]
    fn midnight_and_noon_have_correct_meridiem() {
        let c = Config {
            format_24h: false,
            ..Config::default()
        };
        for (hour, expected, suffix) in [
            (0, "12:05:09", "AM"),
            (12, "12:05:09", "PM"),
            (23, "11:05:09", "PM"),
        ] {
            let now = Local.with_ymd_and_hms(2026, 10, 24, hour, 5, 9).unwrap();
            assert_eq!(time(now, &c), expected);
            assert_eq!(meridiem(now), suffix);
        }
    }
}

// Alternative faces share the same metrics; compact faces retain their own proportions.
pub const ROUND_DIGITS: [[&str; 7]; 10] = [
    [
        "01110", "10001", "10001", "10001", "10001", "10001", "01110",
    ],
    [
        "00100", "01100", "00100", "00100", "00100", "00100", "01110",
    ],
    [
        "01110", "10001", "00001", "00010", "00100", "01000", "11111",
    ],
    [
        "11110", "00001", "00001", "01110", "00001", "00001", "11110",
    ],
    [
        "10001", "10001", "10001", "11111", "00001", "00001", "00001",
    ],
    [
        "11111", "10000", "10000", "11110", "00001", "00001", "11110",
    ],
    [
        "01110", "10000", "10000", "11110", "10001", "10001", "01110",
    ],
    [
        "11111", "00001", "00010", "00100", "01000", "01000", "01000",
    ],
    [
        "01110", "10001", "10001", "01110", "10001", "10001", "01110",
    ],
    [
        "01110", "10001", "10001", "01111", "00001", "00001", "01110",
    ],
];
pub const COMPACT_ROUND: [[&str; 5]; 10] = [
    ["010", "101", "101", "101", "010"],
    ["010", "110", "010", "010", "010"],
    ["110", "001", "010", "100", "011"],
    ["110", "001", "010", "001", "110"],
    ["101", "101", "111", "001", "001"],
    ["111", "100", "110", "001", "110"],
    ["010", "100", "110", "101", "010"],
    ["111", "001", "010", "010", "010"],
    ["010", "101", "010", "101", "010"],
    ["010", "101", "011", "001", "010"],
];
// Bits A–G describe actual segments. Vertical strokes include their junction
// rows, so 0/1/4/7 cannot acquire gaps when the middle or end bars are absent.
const SEGMENTS: [u8; 10] = [0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x07, 0x7f, 0x6f];
pub fn digital_pixel(digit: usize, x: usize, y: usize, compact: bool) -> bool {
    let mask = SEGMENTS[digit];
    let (right, middle, bottom) = if compact { (2, 2, 4) } else { (4, 3, 6) };
    if x > right || y > bottom {
        return false;
    }
    (y == 0 && x > 0 && x < right && mask & 0x01 != 0)
        || (x == right && y <= middle && mask & 0x02 != 0)
        || (x == right && y >= middle && mask & 0x04 != 0)
        || (y == bottom && x > 0 && x < right && mask & 0x08 != 0)
        || (x == 0 && y >= middle && mask & 0x10 != 0)
        || (x == 0 && y <= middle && mask & 0x20 != 0)
        || (y == middle && x > 0 && x < right && mask & 0x40 != 0)
}
pub const BLOCK_DIGITS: [[&str; 7]; 10] = [
    [
        "11111", "11011", "11011", "11011", "11011", "11011", "11111",
    ],
    [
        "00110", "01110", "00110", "00110", "00110", "00110", "11111",
    ],
    [
        "11111", "00011", "00011", "11111", "11000", "11000", "11111",
    ],
    [
        "11111", "00011", "00011", "11111", "00011", "00011", "11111",
    ],
    [
        "11011", "11011", "11011", "11111", "00011", "00011", "00011",
    ],
    [
        "11111", "11000", "11000", "11111", "00011", "00011", "11111",
    ],
    [
        "11111", "11000", "11000", "11111", "11011", "11011", "11111",
    ],
    [
        "11111", "00011", "00011", "00011", "00011", "00011", "00011",
    ],
    [
        "11111", "11011", "11011", "11111", "11011", "11011", "11111",
    ],
    [
        "11111", "11011", "11011", "11111", "00011", "00011", "11111",
    ],
];
pub const COMPACT_BLOCK: [[&str; 5]; 10] = [
    ["111", "101", "101", "101", "111"],
    ["110", "010", "010", "010", "111"],
    ["111", "001", "111", "100", "111"],
    ["111", "001", "111", "001", "111"],
    ["101", "101", "111", "001", "001"],
    ["111", "100", "111", "001", "111"],
    ["111", "100", "111", "101", "111"],
    ["111", "001", "001", "001", "001"],
    ["111", "101", "111", "101", "111"],
    ["111", "101", "111", "001", "111"],
];
const SLANT_BASE: [[&str; 7]; 10] = [
    ["111", "101", "101", "101", "101", "101", "111"],
    ["010", "110", "010", "010", "010", "010", "111"],
    ["111", "001", "001", "111", "100", "100", "111"],
    ["111", "001", "001", "111", "001", "001", "111"],
    ["101", "101", "101", "111", "001", "001", "001"],
    ["111", "100", "100", "111", "001", "001", "111"],
    ["111", "100", "100", "111", "101", "101", "111"],
    ["111", "001", "001", "001", "001", "001", "001"],
    ["111", "101", "101", "111", "101", "101", "111"],
    ["111", "101", "101", "111", "001", "001", "111"],
];
pub const COMPACT_SLANT: [[&str; 5]; 10] = [
    ["011", "101", "101", "101", "110"],
    ["001", "011", "010", "010", "110"],
    ["011", "001", "010", "100", "110"],
    ["011", "001", "010", "001", "110"],
    ["101", "101", "111", "010", "010"],
    ["011", "100", "110", "001", "110"],
    ["011", "100", "110", "101", "110"],
    ["111", "001", "010", "010", "100"],
    ["011", "101", "010", "101", "110"],
    ["011", "101", "011", "001", "110"],
];
pub fn pixel(
    style: crate::config::ClockStyle,
    digit: usize,
    x: usize,
    y: usize,
    compact: bool,
) -> bool {
    use crate::config::ClockStyle;
    if style == ClockStyle::Digital {
        return digital_pixel(digit, x, y, compact);
    }
    if style == ClockStyle::Slant && !compact {
        let offset = if y < 2 {
            2
        } else if y < 5 {
            1
        } else {
            0
        };
        return x >= offset && SLANT_BASE[digit][y].as_bytes().get(x - offset) == Some(&b'1');
    }
    let rows: &[&str] = if compact {
        match style {
            ClockStyle::Round => &COMPACT_ROUND[digit],
            ClockStyle::Block => &COMPACT_BLOCK[digit],
            ClockStyle::Slant => &COMPACT_SLANT[digit],
            _ => &COMPACT_DIGITS[digit],
        }
    } else {
        match style {
            ClockStyle::Round => &ROUND_DIGITS[digit],
            ClockStyle::Block => &BLOCK_DIGITS[digit],
            _ => &DIGITS[digit],
        }
    };
    rows[y].as_bytes()[x] == b'1'
}
pub const COMPACT_OUTLINE: [[&str; 5]; 10] = [
    ["╭─╮", "│ │", "│ │", "│ │", "╰─╯"],
    [" ╷ ", "─┤ ", " │ ", " │ ", "─┴─"],
    ["──╮", "  │", "╭─╯", "│  ", "╰──"],
    ["──╮", "  │", " ─┤", "  │", "──╯"],
    ["╷ ╷", "│ │", "╰─┤", "  │", "  ╵"],
    ["╭──", "│  ", "╰─╮", "  │", "──╯"],
    ["╭──", "│  ", "├─╮", "│ │", "╰─╯"],
    ["──╮", "  │", "  │", "  │", "  ╵"],
    ["╭─╮", "│ │", "├─┤", "│ │", "╰─╯"],
    ["╭─╮", "│ │", "╰─┤", "  │", "──╯"],
];
pub const OUTLINE_DIGITS: [[&str; 7]; 10] = [
    [
        "╭───╮",
        "│   │",
        "│   │",
        "│   │",
        "│   │",
        "│   │",
        "╰───╯",
    ],
    [
        "  ╷  ",
        " ─┤  ",
        "  │  ",
        "  │  ",
        "  │  ",
        "  │  ",
        " ─┴─ ",
    ],
    [
        "╭───╮",
        "    │",
        "    │",
        "╭───╯",
        "│    ",
        "│    ",
        "╰────",
    ],
    [
        "────╮",
        "    │",
        "    │",
        " ───┤",
        "    │",
        "    │",
        "────╯",
    ],
    [
        "╷   ╷",
        "│   │",
        "│   │",
        "╰───┤",
        "    │",
        "    │",
        "    ╵",
    ],
    [
        "╭────",
        "│    ",
        "│    ",
        "╰───╮",
        "    │",
        "    │",
        "────╯",
    ],
    [
        "╭────",
        "│    ",
        "│    ",
        "├───╮",
        "│   │",
        "│   │",
        "╰───╯",
    ],
    [
        "────╮",
        "    │",
        "    │",
        "    │",
        "    │",
        "    │",
        "    ╵",
    ],
    [
        "╭───╮",
        "│   │",
        "│   │",
        "├───┤",
        "│   │",
        "│   │",
        "╰───╯",
    ],
    [
        "╭───╮",
        "│   │",
        "│   │",
        "╰───┤",
        "    │",
        "    │",
        "────╯",
    ],
];

// Junctions share the same horizontal anchor as vertical strokes, including the
// centered stem of 1. Right-edge corners extend to the end of a scaled cell.
pub fn wire_symbol(ch: char, col: usize, width: usize, dx: u16, scale: u16) -> &'static str {
    let anchor = if col == width - 1 { scale - 1 } else { 0 };
    match ch {
        '╭' => {
            if dx == 0 {
                "╭"
            } else {
                "─"
            }
        }
        '╰' => {
            if dx == 0 {
                "╰"
            } else {
                "─"
            }
        }
        '╮' => {
            if dx == scale - 1 {
                "╮"
            } else {
                "─"
            }
        }
        '╯' => {
            if dx == scale - 1 {
                "╯"
            } else {
                "─"
            }
        }
        '├' => match dx.cmp(&anchor) {
            std::cmp::Ordering::Equal => "├",
            std::cmp::Ordering::Greater => "─",
            std::cmp::Ordering::Less => " ",
        },
        '┤' => match dx.cmp(&anchor) {
            std::cmp::Ordering::Equal => "┤",
            std::cmp::Ordering::Less => "─",
            std::cmp::Ordering::Greater => " ",
        },
        '┴' => {
            if dx == anchor {
                "┴"
            } else {
                "─"
            }
        }
        '│' => {
            if dx == anchor {
                "│"
            } else {
                " "
            }
        }
        '╷' => {
            if dx == anchor {
                "╷"
            } else {
                " "
            }
        }
        '╵' => {
            if dx == anchor {
                "╵"
            } else {
                " "
            }
        }
        '─' => "─",
        _ => " ",
    }
}
