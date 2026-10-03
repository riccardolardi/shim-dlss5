//! Unpack a verified archive into a store folder, applying the manifest's
//! strip/only rules. Every entry path is checked so an archive can never
//! write outside its destination.

use std::io::Read;
use std::path::{Component as PathComponent, Path, PathBuf};

use crate::{
    components::manifest::{ArchiveKind, ExtractRules},
    Error, Result,
};

/// Extract `archive` into `dest`. Returns the relative paths written, sorted.
pub fn extract(
    archive: &Path,
    kind: ArchiveKind,
    rules: &ExtractRules,
    dest: &Path,
) -> Result<Vec<String>> {
    std::fs::create_dir_all(dest).map_err(|e| Error::io(dest, e))?;
    let mut written = match kind {
        ArchiveKind::Zip => extract_zip(archive, rules, dest)?,
        ArchiveKind::SevenZ => extract_7z(archive, rules, dest)?,
    };
    written.sort();
    Ok(written)
}

/// List entry paths without extracting (for diagnostics and tests).
pub fn list(archive: &Path, kind: ArchiveKind) -> Result<Vec<String>> {
    let mut out = Vec::new();
    match kind {
        ArchiveKind::Zip => {
            let file = std::fs::File::open(archive).map_err(|e| Error::io(archive, e))?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| archive_err(archive, e))?;
            for i in 0..zip.len() {
                let entry = zip.by_index(i).map_err(|e| archive_err(archive, e))?;
                if !entry.is_dir() {
                    out.push(normalise(entry.name()));
                }
            }
        }
        ArchiveKind::SevenZ => {
            let mut reader =
                sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty())
                    .map_err(|e| archive_err(archive, e))?;
            reader
                .for_each_entries(|entry, _| {
                    if !entry.is_directory() {
                        out.push(normalise(entry.name()));
                    }
                    Ok(true)
                })
                .map_err(|e| archive_err(archive, e))?;
        }
    }
    out.sort();
    Ok(out)
}

fn extract_zip(archive: &Path, rules: &ExtractRules, dest: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(archive).map_err(|e| Error::io(archive, e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| archive_err(archive, e))?;
    let mut written = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| archive_err(archive, e))?;
        if entry.is_dir() {
            continue;
        }
        let name = normalise(entry.name());
        let Some(target) = target_for(&name, rules, dest)? else {
            continue;
        };
        let hint = entry_size_hint(entry.size());
        let bytes = read_capped(&mut entry, hint, &name).map_err(|e| archive_err(archive, e))?;
        write_file(&target, &bytes)?;
        written.push(name);
    }
    Ok(written)
}

/// Largest single file we will unpack. The biggest real entry (a DLSS DLL
/// inside OptiScaler) is ~50 MB; this guards against decompression bombs and
/// forged size headers.
pub const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;

/// Pre-allocate from the header, but never trust it beyond the cap.
fn entry_size_hint(declared: u64) -> usize {
    declared.min(64 * 1024 * 1024) as usize
}

/// Read a whole entry, failing instead of growing past `MAX_ENTRY_BYTES`.
fn read_capped(reader: &mut dyn Read, hint: usize, name: &str) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(hint);
    reader.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(std::io::Error::other(format!(
            "entry {name} is larger than {MAX_ENTRY_BYTES} bytes"
        )));
    }
    Ok(bytes)
}

fn extract_7z(archive: &Path, rules: &ExtractRules, dest: &Path) -> Result<Vec<String>> {
    let mut reader = sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty())
        .map_err(|e| archive_err(archive, e))?;
    let mut written = Vec::new();
    let mut failure: Option<Error> = None;
    reader
        .for_each_entries(|entry, data| {
            if entry.is_directory() {
                return Ok(true);
            }
            let name = normalise(entry.name());
            let target = match target_for(&name, rules, dest) {
                Ok(Some(t)) => t,
                Ok(None) => {
                    // Solid 7z streams must be read through even when an entry
                    // is dropped, or the archive's checksum check fails.
                    std::io::copy(data, &mut std::io::sink())?;
                    return Ok(true);
                }
                Err(e) => {
                    failure = Some(e);
                    return Ok(false);
                }
            };
            let bytes = read_capped(data, entry_size_hint(entry.size()), &name)?;
            if let Err(e) = write_file(&target, &bytes) {
                failure = Some(e);
                return Ok(false);
            }
            written.push(name);
            Ok(true)
        })
        .map_err(|e| archive_err(archive, e))?;
    match failure {
        Some(e) => Err(e),
        None => Ok(written),
    }
}

/// Where an entry lands, or `None` when the rules drop it. Rejects absolute
/// paths and `..` so a hostile archive cannot escape `dest`.
fn target_for(name: &str, rules: &ExtractRules, dest: &Path) -> Result<Option<PathBuf>> {
    if !rules.only.is_empty() && !rules.only.iter().any(|o| o == name) {
        return Ok(None);
    }
    if rules.strip.iter().any(|s| name.starts_with(s.as_str())) {
        return Ok(None);
    }
    let rel = Path::new(name);
    let safe = rel
        .components()
        .all(|c| matches!(c, PathComponent::Normal(_)));
    if !safe || name.is_empty() {
        return Err(Error::Archive {
            detail: format!("entry {name:?} would escape the destination"),
        });
    }
    Ok(Some(dest.join(rel)))
}

fn write_file(target: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::write(target, bytes).map_err(|e| Error::io(target, e))
}

fn normalise(name: &str) -> String {
    name.replace('\\', "/")
}

fn archive_err(archive: &Path, e: impl std::fmt::Display) -> Error {
    Error::Archive {
        detail: format!("{}: {e}", archive.display()),
    }
}

#[cfg(test)]
pub mod testing {
    use std::io::Write;
    use std::path::Path;

    /// A zip with the given `(path, bytes)` entries.
    pub fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::testing::write_zip;
    use super::*;

    #[test]
    fn zip_extracts_with_strip_and_only_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("a.zip");
        write_zip(
            &archive,
            &[
                ("OptiScaler.dll", b"dll"),
                ("docs/readme.md", b"doc"),
                ("D3D12_Optiscaler/d3d12.dll", b"redist"),
            ],
        );
        let dest = tmp.path().join("out");
        let rules = ExtractRules {
            strip: vec!["docs/".into()],
            only: vec![],
        };
        let written = extract(&archive, ArchiveKind::Zip, &rules, &dest).unwrap();
        assert_eq!(
            written,
            vec!["D3D12_Optiscaler/d3d12.dll", "OptiScaler.dll"]
        );
        assert_eq!(std::fs::read(dest.join("OptiScaler.dll")).unwrap(), b"dll");
        assert!(!dest.join("docs").exists());

        let dest2 = tmp.path().join("out2");
        let only = ExtractRules {
            strip: vec![],
            only: vec!["OptiScaler.dll".into()],
        };
        let written = extract(&archive, ArchiveKind::Zip, &only, &dest2).unwrap();
        assert_eq!(written, vec!["OptiScaler.dll"]);
        assert_eq!(
            list(&archive, ArchiveKind::Zip).unwrap(),
            vec![
                "D3D12_Optiscaler/d3d12.dll",
                "OptiScaler.dll",
                "docs/readme.md"
            ]
        );
    }

    #[test]
    fn zip_with_prefixed_bytes_still_opens() {
        // ReShade's setup exe is an executable with a zip appended.
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("setup.exe");
        write_zip(&archive, &[("ReShade64.dll", b"rs")]);
        let zipped = std::fs::read(&archive).unwrap();
        let mut prefixed = b"MZ fake exe header".to_vec();
        prefixed.extend(zipped);
        std::fs::write(&archive, prefixed).unwrap();
        let out = tmp.path().join("out");
        let written = extract(&archive, ArchiveKind::Zip, &ExtractRules::default(), &out).unwrap();
        assert_eq!(written, vec!["ReShade64.dll"]);
    }

    #[test]
    fn traversal_entries_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("evil.zip");
        write_zip(&archive, &[("../escape.txt", b"x")]);
        let out = tmp.path().join("out");
        let err = extract(&archive, ArchiveKind::Zip, &ExtractRules::default(), &out).unwrap_err();
        assert_eq!(err.code(), "archive");
        assert!(!tmp.path().join("escape.txt").exists());
    }

    #[test]
    fn garbage_is_an_archive_error() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("x.7z");
        std::fs::write(&archive, b"not an archive").unwrap();
        let out = tmp.path().join("out");
        assert_eq!(
            extract(
                &archive,
                ArchiveKind::SevenZ,
                &ExtractRules::default(),
                &out
            )
            .unwrap_err()
            .code(),
            "archive"
        );
    }
}
