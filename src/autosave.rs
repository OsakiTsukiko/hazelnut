//! Crash-recovery copies of notebooks with unsaved changes.
//!
//! For `/dir/notes.hzn` the autosave is the hidden file
//! `/dir/.notes.hzn.autosave`, in the normal `.hzn` format. It's removed when
//! the notebook is saved or its changes are discarded, so one only survives
//! if Hazelnut didn't exit cleanly.

use crate::format;
use crate::model::Notebook;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub fn path_for(notebook: &Path) -> PathBuf {
    let name = notebook.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    notebook.with_file_name(format!(".{name}.autosave"))
}

/// Write the autosave atomically (temp file + rename).
pub fn write(notebook: &Path, data: &str) -> std::io::Result<()> {
    let target = path_for(notebook);
    let tmp = target.with_extension("autosave.tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, &target).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub fn remove(notebook: &Path) {
    let _ = std::fs::remove_file(path_for(notebook));
}

/// If an autosave exists for `notebook`, when it was written (unix seconds).
pub fn find(notebook: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path_for(notebook)).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(modified)
}

pub fn read(notebook: &Path) -> Result<Notebook, String> {
    let src = std::fs::read_to_string(path_for(notebook)).map_err(|e| e.to_string())?;
    format::parse(&src).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_find_read_remove() {
        let dir = std::env::temp_dir().join(format!("hazelnut-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let nb_path = dir.join("notes.hzn");
        assert!(find(&nb_path).is_none());

        let nb = Notebook::new("Autosaved");
        write(&nb_path, &format::serialize(&nb)).unwrap();
        assert!(find(&nb_path).is_some());
        assert!(dir.join(".notes.hzn.autosave").exists());
        assert_eq!(read(&nb_path).unwrap().title, "Autosaved");

        remove(&nb_path);
        assert!(find(&nb_path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hidden_sibling_path() {
        assert_eq!(path_for(Path::new("/a/b/notes.hzn")), PathBuf::from("/a/b/.notes.hzn.autosave"));
    }
}
