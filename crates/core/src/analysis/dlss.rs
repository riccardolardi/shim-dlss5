//! DLSS-related files in a game folder: whether the game ships DLSS (and
//! which version), whether a DLSS 5 model is already present, and whether a
//! ReShade or OptiScaler that is not ours sits in a proxy slot.

use std::path::Path;

use crate::analysis::{apis::contains_ci, pe, walk::Tree};

/// Files that mean "this game integrates DLSS or Streamline".
const DLSS_DLLS: &[&str] = &["nvngx_dlss.dll", "nvngx_dlssg.dll", "nvngx_dlssd.dll"];
pub const MODEL_DLL: &str = "nvngx_dlssnr.dll";

/// Names a proxy DLL can take beside the executable.
pub const PROXY_NAMES: &[&str] = &[
    "dxgi.dll",
    "d3d11.dll",
    "d3d12.dll",
    "d3d9.dll",
    "winmm.dll",
    "version.dll",
    "dbghelp.dll",
    "wininet.dll",
    "winhttp.dll",
    "opengl32.dll",
    "reshade64.dll",
    "reshade32.dll",
    "optiscaler.dll",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DlssFacts {
    pub ships_dlss: bool,
    pub dlss_version: Option<String>,
    pub has_model: bool,
    pub foreign_reshade: bool,
    pub foreign_optiscaler: bool,
}

pub fn inspect(tree: &Tree, exe_dir: &Path) -> DlssFacts {
    let dlss = tree
        .files
        .iter()
        .find(|f| DLSS_DLLS.contains(&f.name.as_str()));
    let streamline = tree
        .files
        .iter()
        .any(|f| f.name.starts_with("sl.") && f.name.ends_with(".dll"));
    let (foreign_reshade, foreign_optiscaler) = foreign_proxies(tree, exe_dir);
    DlssFacts {
        ships_dlss: dlss.is_some() || streamline,
        dlss_version: tree
            .find_file("nvngx_dlss.dll")
            .and_then(|f| file_version(&f.path)),
        has_model: tree.has_file(MODEL_DLL),
        foreign_reshade,
        foreign_optiscaler,
    }
}

/// Look at every proxy-named DLL beside the exe and at the config files
/// those tools leave behind.
fn foreign_proxies(tree: &Tree, exe_dir: &Path) -> (bool, bool) {
    let mut reshade = tree.has_file("reshade.ini") || tree.has_dir("reshade-shaders");
    let mut optiscaler = tree.has_file("optiscaler.ini") || tree.has_file("optiscaler.log");
    for name in PROXY_NAMES {
        let path = exe_dir.join(name);
        if !path.is_file() {
            continue;
        }
        let Ok(info) = pe::read_header(&path) else {
            continue;
        };
        let bytes = pe::read_sections_or_file(&path, &info, &[".rdata", ".rsrc", ".data"]);
        reshade |= contains_ci(&bytes, "reshade");
        optiscaler |= contains_ci(&bytes, "optiscaler");
    }
    (reshade, optiscaler)
}

/// `FileVersion` from the resource section, as `a.b.c.d`. Finds the
/// `VS_VERSION_INFO` string and the `VS_FIXEDFILEINFO` signature behind it.
pub fn file_version(path: &Path) -> Option<String> {
    let info = pe::read_header(path).ok()?;
    let rsrc = info.section(".rsrc")?;
    let bytes = pe::read_section(path, rsrc).ok()?;
    version_from_resource(&bytes)
}

pub fn version_from_resource(bytes: &[u8]) -> Option<String> {
    const KEY: &[u8] = b"V\0S\0_\0V\0E\0R\0S\0I\0O\0N\0_\0I\0N\0F\0O\0";
    const SIGNATURE: [u8; 4] = [0xBD, 0x04, 0xEF, 0xFE];
    let at = bytes.windows(KEY.len()).position(|w| w == KEY)?;
    let search = &bytes[at + KEY.len()..(at + KEY.len() + 64).min(bytes.len())];
    let sig = search.windows(4).position(|w| w == SIGNATURE)?;
    let fixed = &search[sig..];
    if fixed.len() < 16 {
        return None;
    }
    let ms = u32::from_le_bytes([fixed[8], fixed[9], fixed[10], fixed[11]]);
    let ls = u32::from_le_bytes([fixed[12], fixed[13], fixed[14], fixed[15]]);
    Some(format!(
        "{}.{}.{}.{}",
        ms >> 16,
        ms & 0xFFFF,
        ls >> 16,
        ls & 0xFFFF
    ))
}

#[cfg(test)]
pub mod testing {
    /// A fake `.rsrc` body carrying a `VS_FIXEDFILEINFO` with this version.
    pub fn version_resource(major: u16, minor: u16, build: u16, rev: u16) -> Vec<u8> {
        let mut out = vec![0u8; 6]; // wLength, wValueLength, wType
        out.extend_from_slice(b"V\0S\0_\0V\0E\0R\0S\0I\0O\0N\0_\0I\0N\0F\0O\0\0\0");
        out.extend_from_slice(&[0, 0]); // padding to 32-bit
        out.extend_from_slice(&[0xBD, 0x04, 0xEF, 0xFE]); // dwSignature
        out.extend_from_slice(&0x0001_0000u32.to_le_bytes()); // dwStrucVersion
        out.extend_from_slice(&(((major as u32) << 16) | minor as u32).to_le_bytes());
        out.extend_from_slice(&(((build as u32) << 16) | rev as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; 32]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::pe::testing::{build_pe, write_exe, X64};
    use crate::analysis::walk::testing::make_tree;

    #[test]
    fn version_is_read_from_fixed_file_info() {
        let rsrc = testing::version_resource(3, 7, 10, 0);
        assert_eq!(version_from_resource(&rsrc), Some("3.7.10.0".into()));
        assert_eq!(version_from_resource(b"nothing"), None);
    }

    #[test]
    fn dlss_dlls_streamline_and_model_are_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let dll = build_pe(X64, &[(".rsrc", &testing::version_resource(310, 2, 1, 0))]);
        make_tree(
            tmp.path(),
            &[
                ("Game.exe", b""),
                (
                    "Engine/Plugins/DLSS/Binaries/ThirdParty/Win64/nvngx_dlss.dll",
                    &dll,
                ),
                ("sl.interposer.dll", b""),
            ],
        );
        let tree = Tree::collect(tmp.path(), 6);
        let facts = inspect(&tree, tmp.path());
        assert!(facts.ships_dlss);
        assert_eq!(facts.dlss_version.as_deref(), Some("310.2.1.0"));
        assert!(!facts.has_model);
        assert!(!facts.foreign_reshade && !facts.foreign_optiscaler);

        make_tree(tmp.path(), &[(MODEL_DLL, b"")]);
        assert!(inspect(&Tree::collect(tmp.path(), 6), tmp.path()).has_model);
    }

    #[test]
    fn foreign_proxies_are_recognised_by_strings_and_config_files() {
        let tmp = tempfile::tempdir().unwrap();
        write_exe(
            &tmp.path().join("dxgi.dll"),
            X64,
            &["ReShade 6.3 by crosire"],
        );
        make_tree(tmp.path(), &[("Game.exe", b"")]);
        let facts = inspect(&Tree::collect(tmp.path(), 2), tmp.path());
        assert!(facts.foreign_reshade);
        assert!(!facts.foreign_optiscaler);

        let tmp = tempfile::tempdir().unwrap();
        make_tree(tmp.path(), &[("Game.exe", b""), ("OptiScaler.ini", b"[x]")]);
        write_exe(&tmp.path().join("winmm.dll"), X64, &["OptiScaler v0.7"]);
        let facts = inspect(&Tree::collect(tmp.path(), 2), tmp.path());
        assert!(facts.foreign_optiscaler);
        assert!(!facts.foreign_reshade);
    }

    #[test]
    fn plain_system_dll_copies_are_not_foreign() {
        let tmp = tempfile::tempdir().unwrap();
        write_exe(
            &tmp.path().join("version.dll"),
            X64,
            &["GetFileVersionInfoW"],
        );
        make_tree(tmp.path(), &[("Game.exe", b"")]);
        let facts = inspect(&Tree::collect(tmp.path(), 2), tmp.path());
        assert!(!facts.foreign_reshade && !facts.foreign_optiscaler);
    }
}
