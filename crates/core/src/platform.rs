//! Traits for everything that only exists on Windows.
//!
//! The core never touches the registry or Authenticode directly. It asks for
//! these capabilities through traits so it compiles and tests everywhere; the
//! `shim-win` crate provides the real implementations.

use std::path::Path;

/// Read-only access to the Windows registry.
pub trait Registry: Send + Sync {
    /// Read a string value. `None` when the key or value does not exist.
    fn read_string(&self, hive: Hive, key: &str, value: &str) -> Option<String>;
    /// Names of the direct subkeys of `key`, empty when it does not exist.
    fn subkeys(&self, hive: Hive, key: &str) -> Vec<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

/// Result of checking a file's Authenticode signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signature {
    Valid { signer: String },
    HashMismatch { signer: String },
    Unsigned,
    Unknown(String),
}

pub trait SignatureChecker: Send + Sync {
    fn check(&self, file: &Path) -> Signature;
}

/// Everything platform-specific the core may need, bundled.
pub struct Platform {
    pub registry: Box<dyn Registry>,
    pub signatures: Box<dyn SignatureChecker>,
}

/// A platform that has nothing: used on non-Windows hosts and in tests.
pub struct Unavailable;

impl Registry for Unavailable {
    fn read_string(&self, _: Hive, _: &str, _: &str) -> Option<String> {
        None
    }
    fn subkeys(&self, _: Hive, _: &str) -> Vec<String> {
        Vec::new()
    }
}

impl SignatureChecker for Unavailable {
    fn check(&self, _: &Path) -> Signature {
        Signature::Unknown("signature checks need Windows".to_string())
    }
}

impl Platform {
    pub fn unavailable() -> Self {
        Self {
            registry: Box::new(Unavailable),
            signatures: Box::new(Unavailable),
        }
    }
}

/// Fakes shared by the adapter tests.
#[cfg(test)]
pub mod testing {
    use super::*;
    use std::collections::BTreeMap;

    /// An in-memory registry. Keys compare case-insensitively like the real one.
    #[derive(Default)]
    pub struct FakeRegistry {
        values: BTreeMap<(Hive, String), BTreeMap<String, String>>,
    }

    impl FakeRegistry {
        pub fn add(&mut self, hive: Hive, key: &str, values: &[(&str, &str)]) {
            let entry = self.values.entry((hive, key.to_lowercase())).or_default();
            for (name, value) in values {
                entry.insert(name.to_lowercase(), value.to_string());
            }
        }

        pub fn with_platform(self) -> Platform {
            Platform {
                registry: Box::new(self),
                signatures: Box::new(Unavailable),
            }
        }
    }

    impl Registry for FakeRegistry {
        fn read_string(&self, hive: Hive, key: &str, value: &str) -> Option<String> {
            self.values
                .get(&(hive, key.to_lowercase()))?
                .get(&value.to_lowercase())
                .cloned()
        }

        fn subkeys(&self, hive: Hive, key: &str) -> Vec<String> {
            let prefix = format!("{}\\", key.to_lowercase());
            let mut out: Vec<String> = self
                .values
                .keys()
                .filter(|(h, k)| *h == hive && k.starts_with(&prefix))
                .filter_map(|(_, k)| k[prefix.len()..].split('\\').next())
                .map(str::to_string)
                .collect();
            out.sort();
            out.dedup();
            out
        }
    }
}
