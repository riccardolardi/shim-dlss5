//! Unpack already-downloaded component assets into a temp store and print what
//! came out. For checking the pinned manifest's extract rules against the real
//! archives without a network call:
//!
//! ```powershell
//! cargo run -p shim-win --example components -- <dir with the downloaded assets>
//! ```
//!
//! The directory must hold the assets under their manifest file names
//! (`optiscaler.7z`, `reshade_setup.exe`, `feeder.zip` are also accepted).

use std::path::PathBuf;

use shim_core::components::{ComponentManifest, ComponentStore};

fn main() {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("pass the directory holding the downloaded assets");
    let store_dir = std::env::temp_dir().join("shim-components-example");
    let _ = std::fs::remove_dir_all(&store_dir);
    let store = ComponentStore::at(&store_dir);
    let manifest = ComponentManifest::embedded();

    for c in &manifest.components {
        let by_url = dir.join(c.asset.rsplit('/').next().unwrap_or(""));
        let alias = match c.id.as_str() {
            "optiscaler" => "optiscaler.7z",
            "reshade" => "reshade_setup.exe",
            "dlss5-feeder" => "feeder.zip",
            _ => "",
        };
        let asset = if by_url.is_file() {
            by_url
        } else {
            dir.join(alias)
        };
        println!("== {} {} from {}", c.id, c.version, asset.display());
        match store.install_from(c, &asset) {
            Ok(()) => {
                let files = store.files(c).unwrap_or_default();
                println!("   {:?}", store.status(c));
                for f in files {
                    println!("   {f}");
                }
            }
            Err(e) => println!("   FAILED: {}", e.detail()),
        }
    }
    println!("\nstore: {}", store_dir.display());
}
