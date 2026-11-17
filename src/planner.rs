//! Local goals, tasks and deterministic daily suggestions. No AI/network calls.
use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{fs::OpenOptionsExt, io::AsRawFd},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Task,
    Short,
    Long,
    Character,
}
impl Kind {
    pub const OBJECTIVES: [Self; 3] = [Self::Task, Self::Short, Self::Long];
    pub fn label(self) -> &'static str {
        match self {
            Self::Task => "Tasks",
            Self::Short => "Short-term goals",
            Self::Long => "Long-term goals",
            Self::Character => "Character reminders",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Task => Self::Short,
            Self::Short => Self::Long,
            Self::Long => Self::Task,
            Self::Character => Self::Character,
        }
    }
    pub fn previous(self) -> Self {
        self.next().next()
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    High,
    #[default]
    Normal,
    Low,
}
impl Priority {
    pub fn label(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Normal => "Normal",
            Self::Low => "Low",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::High => Self::Normal,
            Self::Normal => Self::Low,
            Self::Low => Self::High,
        }
    }
    pub fn previous(self) -> Self {
        self.next().next()
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: u64,
    pub kind: Kind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub archived: bool,
}
impl Entry {
    pub fn blank(kind: Kind) -> Self {
        Self {
            id: 0,
            kind,
            title: String::new(),
            parent: None,
            due: None,
            priority: Priority::Normal,
            done: false,
            archived: false,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Data {
    pub version: u8,
    pub entries: Vec<Entry>,
}
impl Default for Data {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}
impl Data {
    pub fn entry(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
    pub fn visible(&self, kind: Kind) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| e.kind == kind && !e.archived)
            .collect()
    }
    pub fn valid_parents(&self, kind: Kind) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| {
                !e.archived
                    && !e.done
                    && match kind {
                        Kind::Task => matches!(e.kind, Kind::Short | Kind::Long),
                        Kind::Short => e.kind == Kind::Long,
                        Kind::Long | Kind::Character => false,
                    }
            })
            .collect()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported planner file version".into());
        }
        if self.entries.len() > 2000 {
            return Err("Planner holds at most 2,000 entries".into());
        }
        let mut ids = std::collections::HashSet::new();
        for e in &self.entries {
            if e.id == 0 || !ids.insert(e.id) {
                return Err("Planner entry IDs must be unique and nonzero".into());
            }
            if e.title.trim().is_empty()
                || e.title.len() > 512
                || e.title.chars().any(char::is_control)
            {
                return Err("Use a title of 1–512 bytes without control characters".into());
            }
            if let Some(due) = &e.due {
                NaiveDate::parse_from_str(due, "%Y-%m-%d")
                    .map_err(|_| "Use a due date in YYYY-MM-DD format")?;
            }
            if let Some(parent) = e.parent {
                let p = self.entry(parent).ok_or("Linked goal no longer exists")?;
                if !matches!(
                    (e.kind, p.kind),
                    (Kind::Task, Kind::Short | Kind::Long) | (Kind::Short, Kind::Long)
                ) {
                    return Err(
                        "Link tasks to a goal, and short-term goals to a long-term goal".into(),
                    );
                }
            }
        }
        Ok(())
    }
    pub fn pending(&self, date: NaiveDate) -> Vec<&Entry> {
        let mut tasks = self
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Task && !e.done && !e.archived)
            .collect::<Vec<_>>();
        tasks.sort_by_key(|e| {
            let due = e
                .due
                .as_ref()
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
            let group = match due {
                Some(d) if d < date => 0,
                Some(d) if d == date => 1,
                None => 2,
                _ => 3,
            };
            (group, e.priority, due, e.id)
        });
        tasks
    }
    pub fn goals_for(&self, task: Option<&Entry>) -> (Option<&Entry>, Option<&Entry>) {
        let linked = task.and_then(|e| e.parent).and_then(|id| self.entry(id));
        let short = linked.filter(|e| e.kind == Kind::Short).or_else(|| {
            self.entries
                .iter()
                .find(|e| e.kind == Kind::Short && !e.done && !e.archived)
        });
        let long = linked
            .filter(|e| e.kind == Kind::Long)
            .or_else(|| {
                short
                    .and_then(|e| e.parent)
                    .and_then(|id| self.entry(id))
                    .filter(|e| !e.done && !e.archived)
            })
            .or_else(|| {
                self.entries
                    .iter()
                    .find(|e| e.kind == Kind::Long && !e.done && !e.archived)
            });
        (short, long)
    }
    pub fn daily_lines(&self, date: NaiveDate, rows: usize) -> Vec<String> {
        if rows == 0 {
            return vec![];
        }
        let tasks = self.pending(date);
        let (short, long) = self.goals_for(tasks.first().copied());
        let mut lines = vec![if let Some(task) = tasks.first() {
            let urgency = task
                .due
                .as_deref()
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                .map(|d| match d.cmp(&date) {
                    std::cmp::Ordering::Less => " · overdue",
                    std::cmp::Ordering::Equal => " · due today",
                    std::cmp::Ordering::Greater => "",
                })
                .unwrap_or("");
            format!("Start with: {}{}", task.title, urgency)
        } else if self
            .entries
            .iter()
            .any(|e| e.kind == Kind::Task && e.done && !e.archived)
        {
            "Your tasks are clear. Take a breath, or choose a new next step.".into()
        } else {
            "A small step is enough. Press + to add today's first task.".into()
        }];
        if rows >= 4 && tasks.len() > 1 {
            lines.push(format!(
                "Then: {}",
                tasks
                    .iter()
                    .skip(1)
                    .take(2)
                    .map(|e| e.title.as_str())
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        if rows >= 3 {
            if let Some(e) = short {
                lines.push(format!("Short term: {}", e.title));
            }
            if let Some(e) = long {
                lines.push(format!("Long term: {}", e.title));
            }
        } else if rows >= 2 {
            let goals = [short, long]
                .into_iter()
                .flatten()
                .map(|e| e.title.as_str())
                .collect::<Vec<_>>()
                .join(" → ");
            if !goals.is_empty() {
                lines.push(format!("Working toward: {goals}"));
            }
        }
        if lines.len() < rows && self.entries.is_empty() {
            lines.push("g goals & tasks · c plan with me".into());
        }
        lines.truncate(rows);
        lines
    }
    pub fn brief(&self, date: NaiveDate) -> String {
        self.daily_lines(date, 4).join("\n")
    }
}
fn read(path: &Path) -> Result<Data, String> {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Data::default()),
        Err(e) => return Err(format!("Cannot read goals: {e}")),
    };
    let mut text = String::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("Cannot read goals: {e}"))?;
    if text.len() > 4 * 1024 * 1024 {
        return Err("Planner file exceeds 4 MiB".into());
    }
    let data: Data = toml::from_str(&text)
        .map_err(|_| "Invalid planner file; repair it before saving goals".to_string())?;
    data.validate()?;
    Ok(data)
}
pub struct Store {
    pub data: Data,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}
impl Store {
    pub fn open(config: &Path) -> Self {
        let path = (!config.as_os_str().is_empty()).then(|| config.with_extension("planner.toml"));
        let mut store = Self {
            data: Data::default(),
            path,
            error: None,
        };
        store.refresh();
        store
    }
    pub fn refresh(&mut self) {
        if let Some(path) = &self.path {
            match read(path) {
                Ok(data) => {
                    self.data = data;
                    self.error = None;
                }
                Err(e) => self.error = Some(e),
            }
        }
    }
    fn commit(&mut self, draft: Data) -> Result<(), String> {
        draft.validate()?;
        if self.error.is_some() {
            return Err("Goals could not be loaded; reopen Goals after repairing the file".into());
        }
        if let Some(path) = &self.path {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create goals directory: {e}"))?;
            let lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .mode(0o600)
                .open(path.with_extension("lock"))
                .map_err(|e| e.to_string())?;
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("Goals are being saved in another window; try again".into());
            }
            if read(path)? != self.data {
                return Err("Goals changed in another window. Reopen Goals, then retry.".into());
            }
            let temp = path.with_extension(format!(
                "{}.{}.tmp",
                std::process::id(),
                Local::now().timestamp_nanos_opt().unwrap_or(0)
            ));
            let save = || -> Result<(), String> {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&temp)
                    .map_err(|e| e.to_string())?;
                file.write_all(
                    toml::to_string_pretty(&draft)
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                )
                .map_err(|e| e.to_string())?;
                file.sync_all().map_err(|e| e.to_string())?;
                fs::rename(&temp, path).map_err(|e| e.to_string())?;
                Ok(())
            };
            if let Err(e) = save() {
                let _ = fs::remove_file(temp);
                return Err(format!("Cannot save goals: {e}"));
            }
        }
        self.data = draft;
        Ok(())
    }
    pub fn put(&mut self, mut entry: Entry) -> Result<u64, String> {
        let mut draft = self.data.clone();
        if entry.id == 0 {
            entry.id = draft
                .entries
                .iter()
                .map(|e| e.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Entry IDs exhausted")?;
            draft.entries.push(entry.clone());
        } else {
            let existing = draft
                .entries
                .iter_mut()
                .find(|e| e.id == entry.id)
                .ok_or("Entry no longer exists")?;
            *existing = entry.clone();
        }
        self.commit(draft)?;
        Ok(entry.id)
    }
    pub fn modify(&mut self, id: u64, f: impl FnOnce(&mut Entry)) -> Result<(), String> {
        let mut e = self
            .data
            .entry(id)
            .cloned()
            .ok_or("Entry no longer exists")?;
        f(&mut e);
        self.put(e)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn memory() -> Store {
        Store::open(Path::new(""))
    }
    fn add(
        store: &mut Store,
        title: &str,
        kind: Kind,
        parent: Option<u64>,
        due: Option<&str>,
        priority: Priority,
    ) -> u64 {
        let mut e = Entry::blank(kind);
        e.title = title.into();
        e.parent = parent;
        e.due = due.map(str::to_owned);
        e.priority = priority;
        store.put(e).unwrap()
    }
    #[test]
    fn plan_orders_deadlines_then_priority_and_uses_linked_goals() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        let mut store = memory();
        let long = add(
            &mut store,
            "Publish Lofiji",
            Kind::Long,
            None,
            None,
            Priority::Normal,
        );
        let short = add(
            &mut store,
            "Finish the editor",
            Kind::Short,
            Some(long),
            None,
            Priority::Normal,
        );
        add(
            &mut store,
            "Later",
            Kind::Task,
            None,
            Some("2026-11-20"),
            Priority::High,
        );
        add(
            &mut store,
            "Unscheduled",
            Kind::Task,
            None,
            None,
            Priority::High,
        );
        add(
            &mut store,
            "Today",
            Kind::Task,
            None,
            Some("2026-11-17"),
            Priority::Normal,
        );
        let overdue = add(
            &mut store,
            "Review Unicode",
            Kind::Task,
            Some(short),
            Some("2026-11-16"),
            Priority::Low,
        );
        assert_eq!(
            store
                .data
                .pending(date)
                .iter()
                .map(|e| e.title.as_str())
                .collect::<Vec<_>>(),
            ["Review Unicode", "Today", "Unscheduled", "Later"]
        );
        let brief = store.data.brief(date);
        assert!(
            brief.contains("overdue")
                && brief.contains("Finish the editor")
                && brief.contains("Publish Lofiji")
        );
        store.modify(overdue, |e| e.done = true).unwrap();
        assert_eq!(store.data.pending(date)[0].title, "Today");
        let today = store.data.pending(date)[0].id;
        store.modify(today, |e| e.archived = true).unwrap();
        assert_eq!(store.data.pending(date)[0].title, "Unscheduled");
    }
    #[test]
    fn validation_prevents_invalid_links_dates_and_empty_titles() {
        let mut store = memory();
        let long = add(
            &mut store,
            "Long goal",
            Kind::Long,
            None,
            None,
            Priority::Normal,
        );
        let task = add(
            &mut store,
            "Task",
            Kind::Task,
            Some(long),
            None,
            Priority::Normal,
        );
        let mut e = Entry::blank(Kind::Short);
        e.title = "Bad parent".into();
        e.parent = Some(task);
        assert!(store.put(e).is_err());
        let mut e = Entry::blank(Kind::Task);
        e.title = "Bad date".into();
        e.due = Some("2026-02-30".into());
        assert!(store.put(e).is_err());
        assert!(store.put(Entry::blank(Kind::Task)).is_err());
        assert_eq!(store.data.entries.len(), 2);
    }
    #[test]
    fn goals_persist_privately_and_stale_or_corrupt_files_are_not_overwritten() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("lofiji-planner-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let config = root.join("config.toml");
        let mut first = Store::open(&config);
        let mut stale = Store::open(&config);
        let id = add(
            &mut first,
            "Write safely",
            Kind::Task,
            None,
            None,
            Priority::Normal,
        );
        let path = first.path.clone().unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let before = fs::read_to_string(&path).unwrap();
        let mut e = Entry::blank(Kind::Task);
        e.title = "Stale edit".into();
        assert!(stale.put(e).is_err());
        assert_eq!(before, fs::read_to_string(&path).unwrap());
        stale.refresh();
        stale.modify(id, |e| e.done = true).unwrap();
        assert!(Store::open(&config).data.entry(id).unwrap().done);
        fs::write(&path, "broken = [").unwrap();
        let mut broken = Store::open(&config);
        assert!(broken.error.is_some());
        let mut e = Entry::blank(Kind::Task);
        e.title = "Unsafe overwrite".into();
        assert!(broken.put(e).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken = [");
        fs::remove_dir_all(root).unwrap();
    }
}
