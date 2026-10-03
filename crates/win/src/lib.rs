//! Windows implementations of the platform traits.
//!
//! On any other OS this crate exposes only [`platform`], which returns the
//! core's `Unavailable` implementation, so the app still builds and runs for
//! UI work.

use shim_core::platform::Platform;

#[cfg(windows)]
mod authenticode;
#[cfg(windows)]
mod registry;

/// The best platform this build can offer.
pub fn platform() -> Platform {
    #[cfg(windows)]
    {
        Platform {
            registry: Box::new(registry::WinRegistry),
            signatures: Box::new(authenticode::WinAuthenticode),
        }
    }
    #[cfg(not(windows))]
    {
        Platform::unavailable()
    }
}

pub fn is_windows() -> bool {
    cfg!(windows)
}
