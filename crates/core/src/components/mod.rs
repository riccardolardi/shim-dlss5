//! Third-party components: the pinned manifest, downloading with hash
//! verification, extraction, and the on-disk store. Nothing here is executed;
//! archives are only unpacked.

pub mod extract;
pub mod fetch;
pub mod manifest;
pub mod store;

pub use manifest::{Component, ComponentManifest};
pub use store::{ComponentStatus, ComponentStore};

use serde::Serialize;
use ts_rs::TS;

/// One row on the Components screen.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ComponentRow {
    pub component: Component,
    pub status: ComponentStatus,
}

pub fn rows(manifest: &ComponentManifest, store: &ComponentStore) -> Vec<ComponentRow> {
    manifest
        .components
        .iter()
        .map(|c| ComponentRow {
            component: c.clone(),
            status: store.status(c),
        })
        .collect()
}
