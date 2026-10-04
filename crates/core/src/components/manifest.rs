//! The pinned component manifest (`components.json`).
//!
//! The copy in our repo is compiled into the binary. A newer copy may later be
//! fetched from our GitHub release, but only if its minisign signature checks
//! out against [`PUBLIC_KEY`]; until a key exists the embedded copy is the only
//! one used.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, Result};

pub const MANIFEST_SCHEMA: u32 = 1;

/// Embedded, pinned copy of `components.json` from the repo root.
pub const EMBEDDED_JSON: &str = include_str!("../../../../components.json");

/// minisign public key that must have signed any remotely fetched manifest.
/// `None` until a release key is generated; then the remote path turns on.
pub const PUBLIC_KEY: Option<&str> = None;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ArchiveKind {
    Zip,
    SevenZ,
}

/// Which files leave the archive and where they land in the store.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ExtractRules {
    /// Archive paths (prefix match, `/`-separated) to drop entirely.
    pub strip: Vec<String>,
    /// If non-empty, only these exact archive paths are kept.
    pub only: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Component {
    pub id: String,
    pub name: String,
    pub license: String,
    /// The publisher's project page, shown to the user.
    pub publisher: String,
    pub version: String,
    /// Pinned HTTPS download URL on the publisher's own release page.
    pub asset: String,
    pub sha256: String,
    #[ts(type = "number")]
    pub size: u64,
    pub archive: ArchiveKind,
    pub extract: ExtractRules,
    /// One line on what this component does for the user.
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ComponentManifest {
    pub schema: u32,
    /// ISO date the pins were last reviewed.
    pub updated: String,
    pub components: Vec<Component>,
}

impl ComponentManifest {
    pub fn embedded() -> Self {
        // The embedded copy is validated by a unit test, so a parse failure
        // here is a build bug, not a runtime condition.
        Self::parse(EMBEDDED_JSON).expect("embedded components.json is valid")
    }

    pub fn parse(json: &str) -> Result<Self> {
        let m: Self = serde_json::from_str(json).map_err(|e| Error::parse("components", e))?;
        m.validate()?;
        Ok(m)
    }

    pub fn get(&self, id: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }

    fn validate(&self) -> Result<()> {
        let bad = |detail: String| Error::ManifestInvalid { detail };
        if self.schema != MANIFEST_SCHEMA {
            return Err(bad(format!(
                "schema {} is not {MANIFEST_SCHEMA}",
                self.schema
            )));
        }
        let mut ids = std::collections::HashSet::new();
        for c in &self.components {
            if !ids.insert(c.id.as_str()) {
                return Err(bad(format!("duplicate component id {}", c.id)));
            }
            if !c.asset.starts_with("https://") {
                return Err(bad(format!("{}: asset URL is not https", c.id)));
            }
            if c.sha256.len() != 64 || !c.sha256.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Err(bad(format!("{}: sha256 is not 64 hex digits", c.id)));
            }
            if c.size == 0 {
                return Err(bad(format!("{}: size is 0", c.id)));
            }
        }
        Ok(())
    }
}

/// Check a detached minisign signature over `bytes`.
pub fn verify_signature(bytes: &[u8], signature: &str, public_key: &str) -> Result<()> {
    let key = minisign_verify::PublicKey::from_base64(public_key).map_err(|e| {
        Error::ManifestSignature {
            detail: e.to_string(),
        }
    })?;
    let sig =
        minisign_verify::Signature::decode(signature).map_err(|e| Error::ManifestSignature {
            detail: e.to_string(),
        })?;
    key.verify(bytes, &sig, false)
        .map_err(|e| Error::ManifestSignature {
            detail: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_is_valid_and_has_the_v1_components() {
        let m = ComponentManifest::embedded();
        let ids: Vec<&str> = m.components.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "reshade",
                "dlss5-feeder",
                "optiscaler-nr",
                "optiscaler-dlssnr",
                "optiscaler"
            ]
        );
        for c in &m.components {
            assert!(c.asset.starts_with("https://"), "{}", c.id);
            assert!(!c.summary.is_empty(), "{}", c.id);
        }
    }

    #[test]
    fn validation_rejects_bad_manifests() {
        let base = ComponentManifest::embedded();
        let mut dup = base.clone();
        dup.components.push(base.components[0].clone());
        assert!(dup.validate().is_err());

        let mut http = base.clone();
        http.components[0].asset = "http://example.com/x.zip".into();
        assert_eq!(http.validate().unwrap_err().code(), "manifest_invalid");

        let mut hash = base.clone();
        hash.components[0].sha256 = "abc".into();
        assert!(hash.validate().is_err());

        assert_eq!(
            ComponentManifest::parse("{not json").unwrap_err().code(),
            "parse"
        );
    }

    #[test]
    fn signature_round_trip_with_a_generated_key() {
        let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let data = EMBEDDED_JSON.as_bytes();
        let sig = minisign::sign(
            None,
            &kp.sk,
            std::io::Cursor::new(data),
            Some("components.json"),
            None,
        )
        .unwrap();
        let pk = kp.pk.to_base64();
        verify_signature(data, &sig.to_string(), &pk).unwrap();

        let err = verify_signature(b"tampered", &sig.to_string(), &pk).unwrap_err();
        assert_eq!(err.code(), "manifest_signature");
        assert!(verify_signature(data, "garbage", &pk).is_err());
    }
}
