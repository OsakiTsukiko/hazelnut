//! The list of recently opened and starred notebooks, persisted between runs.
//!
//! Stored as `<config dir>/hazelnut/library`, one notebook per line:
//! `<starred 0|1>\t<last opened, unix seconds>\t<path>`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RECENT: usize = 40;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub starred: bool,
    pub opened: u64,
    /// Notebook title read from the file (not persisted).
    pub title: Option<String>,
    pub exists: bool,
}

impl Entry {
    pub fn display_title(&self) -> String {
        match &self.title {
            Some(t) if !t.trim().is_empty() => t.trim().to_owned(),
            _ => self
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled notebook".into()),
        }
    }
}

#[derive(Default)]
pub struct Library {
    pub entries: Vec<Entry>,
    file: Option<PathBuf>,
}

impl Library {
    pub fn load() -> Self {
        let file = config_dir().map(|d| d.join("hazelnut").join("library"));
        let mut lib = Library { entries: Vec::new(), file };
        if let Some(src) = lib.file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()) {
            for line in src.lines() {
                let mut parts = line.splitn(3, '\t');
                let (Some(star), Some(time), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
                    continue;
                };
                let path = PathBuf::from(path);
                if path.as_os_str().is_empty() || lib.position(&path).is_some() {
                    continue;
                }
                lib.entries.push(Entry {
                    title: peek_title(&path),
                    exists: path.exists(),
                    path,
                    starred: star == "1",
                    opened: time.parse().unwrap_or(0),
                });
            }
        }
        lib
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        let mut out = String::new();
        for e in &self.entries {
            let Some(p) = e.path.to_str() else { continue };
            out.push_str(&format!("{}\t{}\t{}\n", e.starred as u8, e.opened, p));
        }
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, out);
    }

    fn position(&self, path: &Path) -> Option<usize> {
        self.entries.iter().position(|e| e.path == path)
    }

    /// Record that `path` was just opened or saved.
    pub fn touch(&mut self, path: &Path, title: &str) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        let starred = self.position(&path).map(|i| self.entries.remove(i).starred).unwrap_or(false);
        self.entries.insert(
            0,
            Entry {
                path,
                starred,
                opened: now(),
                title: Some(title.to_owned()),
                exists: true,
            },
        );
        // Drop the oldest unstarred entries beyond the limit.
        let mut unstarred = 0;
        self.entries.retain(|e| {
            if !e.starred {
                unstarred += 1;
            }
            e.starred || unstarred <= MAX_RECENT
        });
        self.save();
    }

    pub fn is_starred(&self, path: &Path) -> bool {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        self.position(&path).is_some_and(|i| self.entries[i].starred)
    }

    pub fn set_starred(&mut self, path: &Path, starred: bool) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        if let Some(i) = self.position(&path) {
            self.entries[i].starred = starred;
            self.save();
        }
    }

    pub fn remove(&mut self, path: &Path) {
        if let Some(i) = self.position(path) {
            self.entries.remove(i);
            self.save();
        }
    }

    /// Re-check which files still exist and re-read their titles.
    pub fn refresh(&mut self) {
        for e in &mut self.entries {
            e.exists = e.path.exists();
            e.title = peek_title(&e.path);
        }
    }

    pub fn starred(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.starred)
    }

    pub fn recent(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| !e.starred)
    }
}

/// Read just the `%title` line from a notebook file.
fn peek_title(path: &Path) -> Option<String> {
    use std::io::{BufRead, BufReader};
    let f = std::fs::File::open(path).ok()?;
    BufReader::new(f)
        .lines()
        .take(50)
        .map_while(Result::ok)
        .find_map(|l| l.strip_prefix("%title").map(|t| t.trim().to_owned()))
}

pub fn config_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "just now", "5 min ago", "yesterday", "3 weeks ago", …
pub fn relative_time(then: u64) -> String {
    let d = now().saturating_sub(then);
    let (m, h, day) = (60, 3600, 86400);
    match d {
        _ if then == 0 => String::new(),
        0..60 => "just now".into(),
        _ if d < h => format!("{} min ago", d / m),
        _ if d < day => format!("{} h ago", d / h),
        _ if d < 2 * day => "yesterday".into(),
        _ if d < 14 * day => format!("{} days ago", d / day),
        _ if d < 60 * day => format!("{} weeks ago", d / (7 * day)),
        _ if d < 365 * day => format!("{} months ago", d / (30 * day)),
        _ => format!("{} years ago", d / (365 * day)),
    }
}

/// Shorten a path for display, using `~` for the home directory.
pub fn pretty_path(p: &Path) -> String {
    let dir = p.parent().unwrap_or(p);
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && let Ok(rest) = dir.strip_prefix(&home)
    {
        return if rest.as_os_str().is_empty() { "~".into() } else { format!("~/{}", rest.display()) };
    }
    dir.display().to_string()
}
