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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
