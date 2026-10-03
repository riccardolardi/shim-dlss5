//! JSON files on disk: atomic writes, and loading with quarantine.
//!
//! A damaged file is never silently reset. It is renamed to `<name>.bad`
//! beside the original, defaults are used, and the error is handed back so
//! the UI can tell the user what happened.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{de::DeserializeOwned, Serialize};

use crate::{Error, Result};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Read a JSON file. Missing file yields `Ok(None)`; damaged file is an error.
pub fn read_json<T: DeserializeOwned>(file: &Path, what: &str) -> Result<Option<T>> {
    match std::fs::read(file) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| Error::parse(what, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(file, e)),
    }
}

/// Like [`read_json`], but a damaged file is moved aside and defaults are
/// returned together with the error that explains why.
pub fn read_json_or_quarantine<T: DeserializeOwned + Default>(
    file: &Path,
    what: &str,
) -> (T, Option<Error>) {
    match read_json::<T>(file, what) {
        Ok(Some(v)) => (v, None),
        Ok(None) => (T::default(), None),
        Err(e @ Error::Parse { .. }) => {
            let bad = quarantine_path(file);
            if let Err(rename_err) = std::fs::rename(file, &bad) {
                return (T::default(), Some(Error::io(bad, rename_err)));
            }
            (T::default(), Some(e))
        }
        Err(e) => (T::default(), Some(e)),
    }
}

/// Serialize to a temp file beside the target, fsync, then rename over it.
pub fn write_json<T: Serialize>(file: &Path, value: &T, what: &str) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| Error::parse(what, e))?;
    write_atomic(file, &bytes)
}

pub fn write_atomic(file: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let tmp = temp_path(file);
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        use std::io::Write;
        f.write_all(bytes).map_err(|e| Error::io(&tmp, e))?;
        f.sync_all().map_err(|e| Error::io(&tmp, e))?;
    }
    std::fs::rename(&tmp, file).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(file, e)
    })
}

fn temp_path(file: &Path) -> PathBuf {
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = file
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    file.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()))
}

fn quarantine_path(file: &Path) -> PathBuf {
    let name = file
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    file.with_file_name(format!("{name}.bad"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Thing {
        n: u32,
    }

    #[test]
    fn write_then_read_round_trips_without_leftovers() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a").join("thing.json");
        write_json(&file, &Thing { n: 7 }, "thing").unwrap();
        assert_eq!(
            read_json::<Thing>(&file, "thing").unwrap(),
            Some(Thing { n: 7 })
        );
        let leftovers: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("thing.json")]);
    }

    #[test]
    fn missing_file_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            read_json::<Thing>(&tmp.path().join("x.json"), "x").unwrap(),
            None
        );
    }

    #[test]
    fn damaged_file_is_quarantined_and_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("thing.json");
        std::fs::write(&file, b"{nope").unwrap();
        let (value, err) = read_json_or_quarantine::<Thing>(&file, "thing");
        assert_eq!(value, Thing::default());
        assert_eq!(err.unwrap().code(), "parse");
        assert!(!file.exists());
        assert!(tmp.path().join("thing.json.bad").exists());
    }

    #[test]
    fn two_temp_names_never_collide() {
        let a = temp_path(Path::new("/x/s.json"));
        let b = temp_path(Path::new("/x/s.json"));
        assert_ne!(a, b);
    }
}
