use crate::config::Config;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}
impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Focus => "FOCUS",
            Self::ShortBreak => "SHORT BREAK",
            Self::LongBreak => "LONG BREAK",
        }
    }
    pub fn duration(self, c: &Config) -> Duration {
        Duration::from_secs(
            60 * match self {
                Self::Focus => c.focus_minutes,
                Self::ShortBreak => c.short_break_minutes,
                Self::LongBreak => c.long_break_minutes,
            },
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    Running,
    Paused,
}

pub struct Pomodoro {
    pub phase: Phase,
    pub status: Status,
    pub completed_focus: u64,
    total: Duration,
    remaining: Duration,
    deadline: Option<Instant>,
}
impl Pomodoro {
    pub fn new(c: &Config) -> Self {
        let total = Phase::Focus.duration(c);
        Self {
            phase: Phase::Focus,
            status: Status::Ready,
            completed_focus: 0,
            total,
            remaining: total,
            deadline: None,
        }
    }
    pub fn remaining(&self, now: Instant) -> Duration {
        self.deadline
            .map(|end| end.saturating_duration_since(now))
            .unwrap_or(self.remaining)
    }
    pub fn seconds(&self, now: Instant) -> u64 {
        self.remaining(now).as_millis().div_ceil(1000) as u64
    }
    pub fn progress(&self, now: Instant) -> f64 {
        (1.0 - self.remaining(now).as_secs_f64() / self.total.as_secs_f64()).clamp(0.0, 1.0)
    }
    pub fn toggle(&mut self, now: Instant) {
        if self.status == Status::Running {
            self.remaining = self.remaining(now);
            self.deadline = None;
            self.status = Status::Paused;
        } else {
            self.deadline = Some(now + self.remaining);
            self.status = Status::Running;
        }
    }
    pub fn reset(&mut self, c: &Config) {
        self.total = self.phase.duration(c);
        self.remaining = self.total;
        self.deadline = None;
        self.status = Status::Ready;
    }
    fn advance(&mut self, c: &Config, completed: bool) {
        if self.phase == Phase::Focus && completed {
            self.completed_focus += 1;
        }
        self.phase = match self.phase {
            Phase::Focus if completed && self.completed_focus % 4 == 0 => Phase::LongBreak,
            Phase::Focus => Phase::ShortBreak,
            _ => Phase::Focus,
        };
        self.reset(c);
    }
    pub fn skip(&mut self, c: &Config, now: Instant) {
        let running = self.status == Status::Running;
        self.advance(c, false);
        if running {
            self.toggle(now);
        }
    }
    /// Advance from the previous deadline, preserving elapsed time across delayed ticks.
    pub fn tick(&mut self, now: Instant, c: &Config) -> u64 {
        let mut completions = 0;
        while let Some(deadline) = self.deadline {
            if now < deadline {
                break;
            }
            self.advance(c, true);
            completions += 1;
            if !c.auto_advance {
                break;
            }
            self.status = Status::Running;
            self.deadline = Some(deadline + self.total);
        }
        completions
    }
    pub fn apply_config(&mut self, c: &Config) {
        if self.status == Status::Ready {
            self.reset(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pauses_resume_without_drift_and_rounds_up() {
        let c = Config::default();
        let t = Instant::now();
        let mut p = Pomodoro::new(&c);
        p.toggle(t);
        let later = t + Duration::from_millis(1234);
        assert_eq!(p.seconds(later), 1499);
        p.toggle(later);
        assert_eq!(
            p.remaining(t + Duration::from_secs(8000)),
            Duration::from_millis(1498766)
        );
        p.toggle(t + Duration::from_secs(8000));
        assert_eq!(p.tick(t + Duration::from_secs(9498), &c), 0);
        assert_eq!(p.tick(t + Duration::from_secs(9499), &c), 1);
        assert_eq!(p.phase, Phase::ShortBreak);
        assert_eq!(p.status, Status::Ready);
    }
    #[test]
    fn fourth_completed_focus_gets_long_break() {
        let c = Config::default();
        let mut t = Instant::now();
        let mut p = Pomodoro::new(&c);
        for i in 1..=4 {
            p.toggle(t);
            t += Duration::from_secs(1500);
            p.tick(t, &c);
            assert_eq!(p.completed_focus, i);
            assert_eq!(
                p.phase,
                if i == 4 {
                    Phase::LongBreak
                } else {
                    Phase::ShortBreak
                }
            );
            p.skip(&c, t);
        }
    }
    #[test]
    fn delayed_ticks_catch_up_using_deadlines() {
        let c = Config {
            auto_advance: true,
            ..Config::default()
        };
        let t = Instant::now();
        let mut p = Pomodoro::new(&c);
        p.toggle(t);
        assert_eq!(p.tick(t + Duration::from_secs(3600), &c), 4);
        assert_eq!(p.phase, Phase::Focus);
        assert_eq!(p.seconds(t + Duration::from_secs(3601)), 1499);
        assert_eq!(p.completed_focus, 2);
        assert_eq!(p.status, Status::Running);
    }
    #[test]
    fn skipped_focus_is_not_completed_and_reset_stops() {
        let c = Config::default();
        let t = Instant::now();
        let mut p = Pomodoro::new(&c);
        p.toggle(t);
        p.skip(&c, t);
        assert_eq!(p.completed_focus, 0);
        assert_eq!(p.phase, Phase::ShortBreak);
        assert_eq!(p.status, Status::Running);
        p.reset(&c);
        assert_eq!(p.status, Status::Ready);
        assert_eq!(p.seconds(t), 300);
    }
    #[test]
    fn edits_do_not_change_running_or_paused_sessions() {
        let mut c = Config::default();
        let t = Instant::now();
        let mut p = Pomodoro::new(&c);
        p.toggle(t);
        c.focus_minutes = 42;
        p.apply_config(&c);
        assert_eq!(p.seconds(t), 1500);
        p.toggle(t);
        p.apply_config(&c);
        assert_eq!(p.seconds(t), 1500);
        p.reset(&c);
        assert_eq!(p.seconds(t), 2520);
    }
    #[test]
    fn full_automatic_cycles_include_long_breaks() {
        let c = Config {
            auto_advance: true,
            ..Config::default()
        };
        let start = Instant::now();
        let mut p = Pomodoro::new(&c);
        p.toggle(start);
        // 4 * 25 focus + 3 * 5 short break + 15 long break = 130 minutes.
        let end = start + Duration::from_secs(130 * 60 * 3 + 7);
        assert_eq!(p.tick(end, &c), 24);
        assert_eq!(p.completed_focus, 12);
        assert_eq!(p.phase, Phase::Focus);
        assert_eq!(p.seconds(end), 1493);
        assert_eq!(p.status, Status::Running);
    }
}
