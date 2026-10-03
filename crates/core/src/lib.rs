//! shim core.
//!
//! Everything in this crate compiles and is tested on any OS. Windows-only
//! behaviour (registry, Authenticode, elevation) lives behind the traits in
//! [`platform`] and is implemented in the `shim-win` crate.

pub mod api;
pub mod discovery;
pub mod error;
pub mod library;
pub mod model;
pub mod paths;
pub mod persist;
pub mod platform;
pub mod settings;

pub use error::{Error, Result};
