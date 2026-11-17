//! User preferences, persisted as `key = value` lines in
//! `<config dir>/hazelnut/settings`.

use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    /// Id of the companion animal (see `pets::SPECIES`), if any.
    pub pet: Option<String>,
    /// Mark newly entered characters as literal in the normal editor.
    pub literal_text: bool,
}

fn file() -> Option<PathBuf> {
    crate::library::config_dir().map(|d| d.join("hazelnut").join("settings"))
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Settings::default();
        let Some(src) = file().and_then(|f| std::fs::read_to_string(f).ok()) else { return s };
        for line in src.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            // Unknown keys are ignored.
            if k.trim() == "literal_text" {
                s.literal_text = v.trim() == "true";
            }
            if k.trim() == "pet" {
                s.pet = Some(v.trim().to_owned()).filter(|v| !v.is_empty() && v != "none");
            }
        }
        s
    }

    pub fn save(&self) {
        let Some(file) = file() else { return };
        let out = format!("pet = {}\nliteral_text = {}\n", self.pet.as_deref().unwrap_or("none"), self.literal_text);
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, out);
    }
}
