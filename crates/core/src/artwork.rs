//! Cover art, fetched after a scan, never during one. Steam games get the
//! library capsule: first from the Steam client's own cache on disk (no
//! network, and the only place newer app ids have one), then from Steam's
//! CDN. Everything else keeps the title card. Covers live in
//! `cache\covers\<game_id>.jpg` and are fetched once.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{
    model::{Game, Launcher},
    Error, Result,
};

const STEAM_CDN: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps";
/// Portrait capsule names, by Steam client generation.
const CACHE_NAMES: &[&str] = &["library_600x900.jpg", "library_capsule.jpg"];
/// Capsules are ~100 KB; anything much larger is not a cover.
const MAX_COVER_BYTES: u64 = 4 * 1024 * 1024;

/// Where this game's cover would be cached.
pub fn cover_path(covers_dir: &Path, game: &Game) -> PathBuf {
    covers_dir.join(format!("{}.jpg", game.id))
}

fn steam_appid(game: &Game) -> Option<&str> {
    match (game.launcher, game.launcher_id.as_deref()) {
        (Launcher::Steam, Some(appid)) if appid.chars().all(|c| c.is_ascii_digit()) => Some(appid),
        _ => None,
    }
}

/// The publisher URL for this game's cover, if we know one.
pub fn cover_url(game: &Game) -> Option<String> {
    steam_appid(game).map(|appid| format!("{STEAM_CDN}/{appid}/library_600x900.jpg"))
}

/// The portrait capsule in the Steam client's cache
/// (`appcache\librarycache\<appid>\<hash>\library_*.jpg`), if present.
pub fn cached_in_steam(steam_root: &Path, game: &Game) -> Option<PathBuf> {
    let dir = steam_root
        .join("appcache")
        .join("librarycache")
        .join(steam_appid(game)?);
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dir).ok()?.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            candidates.extend(CACHE_NAMES.iter().map(|n| path.join(n)));
        } else {
            candidates.push(path);
        }
    }
    candidates.sort();
    candidates.into_iter().find(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| CACHE_NAMES.contains(&n))
            && p.is_file()
    })
}

/// Fetch the cover unless it is already cached. `Ok(None)` when the game has
/// no known cover source or none of the sources has one.
pub fn fetch_cover(
    covers_dir: &Path,
    game: &Game,
    steam_root: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let path = cover_path(covers_dir, game);
    if path.is_file() {
        return Ok(Some(path));
    }
    if let Some(local) = steam_root.and_then(|root| cached_in_steam(root, game)) {
        let bytes = std::fs::read(&local).map_err(|e| Error::io(&local, e))?;
        if looks_like_jpeg(&bytes) {
            crate::persist::write_atomic(&path, &bytes)?;
            return Ok(Some(path));
        }
    }
    let Some(url) = cover_url(game) else {
        return Ok(None);
    };
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("shim/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| download_err(&url, e))?;
    let response = client.get(&url).send().map_err(|e| download_err(&url, e))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response
        .error_for_status()
        .map_err(|e| download_err(&url, e))?;
    if response
        .content_length()
        .is_some_and(|n| n > MAX_COVER_BYTES)
    {
        return Err(download_err(&url, "cover is unreasonably large"));
    }
    let bytes = response.bytes().map_err(|e| download_err(&url, e))?;
    if bytes.len() as u64 > MAX_COVER_BYTES || !looks_like_jpeg(&bytes) {
        return Err(download_err(&url, "response is not a JPEG"));
    }
    crate::persist::write_atomic(&path, &bytes)?;
    Ok(Some(path))
}

fn looks_like_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF])
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
    use crate::model::GameStatus;

    fn game(launcher: Launcher, id: Option<&str>) -> Game {
        Game {
            id: "abc".into(),
            launcher,
            title: "G".into(),
            install_dir: PathBuf::from("C:/g"),
            launcher_id: id.map(str::to_string),
            analysis: None,
            status: GameStatus::Pending,
            cover: None,
            hidden: false,
            mode: None,
            neural: None,
        }
    }

    #[test]
    fn only_steam_games_with_numeric_appids_have_a_cover_url() {
        assert_eq!(
            cover_url(&game(Launcher::Steam, Some("1091500"))),
            Some(format!("{STEAM_CDN}/1091500/library_600x900.jpg"))
        );
        assert_eq!(cover_url(&game(Launcher::Steam, Some("abc"))), None);
        assert_eq!(cover_url(&game(Launcher::Epic, Some("Calluna"))), None);
        assert_eq!(cover_url(&game(Launcher::Steam, None)), None);
    }

    #[test]
    fn cached_cover_is_returned_without_network_and_unknown_source_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let g = game(Launcher::Steam, Some("1"));
        let path = cover_path(tmp.path(), &g);
        std::fs::write(&path, b"\xFF\xD8\xFF").unwrap();
        assert_eq!(fetch_cover(tmp.path(), &g, None).unwrap(), Some(path));
        let gog = Game {
            id: "other".into(),
            ..game(Launcher::Gog, Some("1"))
        };
        assert_eq!(fetch_cover(tmp.path(), &gog, None).unwrap(), None);
    }

    #[test]
    fn steam_client_cache_is_used_before_the_network_either_file_name() {
        let tmp = tempfile::tempdir().unwrap();
        let steam = tmp.path().join("Steam");
        let covers = tmp.path().join("covers");
        for (appid, name) in [
            ("3917090", "library_capsule.jpg"),
            ("2537590", "library_600x900.jpg"),
        ] {
            let dir = steam
                .join("appcache")
                .join("librarycache")
                .join(appid)
                .join("deadbeef");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("library_hero.jpg"), b"\xFF\xD8\xFFhero").unwrap();
            std::fs::write(dir.join(name), b"\xFF\xD8\xFFcapsule").unwrap();
            let g = Game {
                id: appid.into(),
                ..game(Launcher::Steam, Some(appid))
            };
            let got = fetch_cover(&covers, &g, Some(&steam)).unwrap().unwrap();
            assert_eq!(
                std::fs::read(&got).unwrap(),
                b"\xFF\xD8\xFFcapsule",
                "{appid}"
            );
        }
        // Not in the cache and no network in tests: a bogus root falls through to the URL path.
        let g = Game {
            id: "nocache".into(),
            ..game(Launcher::Epic, None)
        };
        assert_eq!(fetch_cover(&covers, &g, Some(&steam)).unwrap(), None);
    }
}
