//! Experimental extensions to `std` for the Switch.

#![stable(feature = "rust1", since = "1.0.0")]
#![doc(cfg(target_os = "switch"))]

pub mod ffi;
pub mod fs;

/// A prelude for conveniently writing platform-specific code.
///
/// Includes all extension traits, and some important type definitions.
#[stable(feature = "rust1", since = "1.0.0")]
pub mod prelude {
    #[doc(no_inline)] #[stable(feature = "rust1", since = "1.0.0")]
    pub use super::ffi::{OsStrExt, OsStringExt};
    #[doc(no_inline)] #[stable(feature = "rust1", since = "1.0.0")]
    pub use super::fs::{FileExt};
}
