//! Download one pinned asset: HTTPS only, size-capped, SHA-256 checked while
//! streaming. The file is written to a temp path and renamed into place only
//! when the hash matches, so a bad download never looks like a good one.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Hard cap on any single download. The largest pinned asset is ~55 MB.
pub const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub received: u64,
    /// From the manifest, not from the server.
    pub expected: u64,
}

/// Download `url` to `dest`, verifying `expected_sha256` (lowercase hex).
pub fn download(
    url: &str,
    expected_sha256: &str,
    expected_size: u64,
    dest: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<()> {
    if !url.starts_with("https://") {
        return Err(Error::Download {
            url: url.to_string(),
            detail: "only https URLs are allowed".into(),
        });
    }
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("shim/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| download_err(url, e))?;
    let response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| download_err(url, e))?;

    let tmp = temp_path(dest);
    let result = stream_to_file(response, &tmp, url, expected_size, progress)
        .and_then(|actual| check_hash(url, expected_sha256, &actual))
        .and_then(|()| std::fs::rename(&tmp, dest).map_err(|e| Error::io(dest, e)));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// SHA-256 of a file on disk, lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf).map_err(|e| Error::io(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn stream_to_file(
    mut response: impl Read,
    tmp: &Path,
    url: &str,
    expected_size: u64,
    progress: &mut dyn FnMut(Progress),
) -> Result<String> {
    if let Some(parent) = tmp.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut file = std::fs::File::create(tmp).map_err(|e| Error::io(tmp, e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut received = 0u64;
    let cap = expected_size.clamp(1, MAX_DOWNLOAD_BYTES);
    loop {
        let n = response.read(&mut buf).map_err(|e| download_err(url, e))?;
        if n == 0 {
            break;
        }
        received += n as u64;
        if received > cap {
            return Err(Error::Download {
                url: url.to_string(),
                detail: format!("response exceeded the pinned size of {expected_size} bytes"),
            });
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| Error::io(tmp, e))?;
        progress(Progress {
            received,
            expected: expected_size,
        });
    }
    file.sync_all().map_err(|e| Error::io(tmp, e))?;
    if received != expected_size {
        return Err(Error::Download {
            url: url.to_string(),
            detail: format!("got {received} bytes, the manifest pins {expected_size}"),
        });
    }
    Ok(hex(&hasher.finalize()))
}

fn check_hash(url: &str, expected: &str, actual: &str) -> Result<()> {
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(Error::HashMismatch {
            what: url.to_string(),
            expected: expected.to_lowercase(),
            actual: actual.to_string(),
        })
    }
}

fn temp_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dest.with_file_name(format!(".{name}.{}.part", std::process::id()))
}

fn download_err(url: &str, e: impl std::fmt::Display) -> Error {
    Error::Download {
        url: url.to_string(),
        detail: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_checks_size_and_hashes() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("x.bin");
        let data = b"hello components";
        let mut seen = 0;
        let hash = stream_to_file(
            std::io::Cursor::new(data),
            &out,
            "https://x",
            data.len() as u64,
            &mut |_| seen += 1,
        )
        .unwrap();
        assert_eq!(hash, sha256_file(&out).unwrap());
        assert_eq!(hash, hex(&Sha256::digest(data)));
        assert!(seen >= 1);

        let short = stream_to_file(
            std::io::Cursor::new(data),
            &out,
            "https://x",
            data.len() as u64 + 1,
            &mut |_| {},
        );
        assert_eq!(short.unwrap_err().code(), "download");

        let long = stream_to_file(
            std::io::Cursor::new(data),
            &out,
            "https://x",
            3,
            &mut |_| {},
        );
        assert!(long.unwrap_err().detail().contains("exceeded"));
    }

    #[test]
    fn hash_mismatch_is_typed() {
        let err = check_hash("https://x", "aa", "bb").unwrap_err();
        assert_eq!(err.code(), "hash_mismatch");
        check_hash("https://x", "AB", "ab").unwrap();
    }

    #[test]
    fn plain_http_is_refused_before_any_network() {
        let tmp = tempfile::tempdir().unwrap();
        let err = download(
            "http://example.com/x",
            "00",
            1,
            &tmp.path().join("x"),
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(err.code(), "download");
    }
}
