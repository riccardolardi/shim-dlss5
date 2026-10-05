//! Read-only registry access through `winreg`.
//!
//! "Not found" is the normal case (a launcher is not installed) and returns
//! `None`/empty quietly. Any other failure, such as access denied or a value
//! of the wrong type, is logged so it is never mistaken for "not installed".
//!
//! Callers name the 32-bit view explicitly (`SOFTWARE\WOW6432Node\...`) where
//! a launcher writes there; no WOW64 redirection flag is applied.

use shim_core::platform::{Hive, Registry};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

pub struct WinRegistry;

const ERROR_FILE_NOT_FOUND: i32 = 2;

fn root(hive: Hive) -> RegKey {
    match hive {
        Hive::CurrentUser => RegKey::predef(HKEY_CURRENT_USER),
        Hive::LocalMachine => RegKey::predef(HKEY_LOCAL_MACHINE),
    }
}

fn open(hive: Hive, key: &str) -> Option<RegKey> {
    match root(hive).open_subkey_with_flags(key, KEY_READ) {
        Ok(k) => Some(k),
        Err(e) if e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND) => None,
        Err(e) => {
            tracing::warn!(%key, error = %e, "registry key could not be opened");
            None
        }
    }
}

impl Registry for WinRegistry {
    fn read_u64(&self, hive: Hive, key: &str, value: &str) -> Option<u64> {
        let k = open(hive, key)?;
        k.get_value::<u64, _>(value).ok()
    }

    fn read_string(&self, hive: Hive, key: &str, value: &str) -> Option<String> {
        let k = open(hive, key)?;
        match k.get_value::<String, _>(value) {
            Ok(v) => Some(v),
            Err(e) if e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND) => None,
            Err(e) => {
                tracing::warn!(%key, %value, error = %e, "registry value could not be read");
                None
            }
        }
    }

    fn subkeys(&self, hive: Hive, key: &str) -> Vec<String> {
        let Some(k) = open(hive, key) else {
            return Vec::new();
        };
        k.enum_keys()
            .filter_map(|r| match r {
                Ok(name) => Some(name),
                Err(e) => {
                    tracing::warn!(%key, error = %e, "registry subkey could not be listed");
                    None
                }
            })
            .collect()
    }
}
