//! `MacAppearance`: the system accent colour via the shim
//! (03-macos-shim.md §Appearance).

use lalela_core::ports::{Appearance, AppearanceError};

/// The macOS accent colour.
#[derive(Debug, Default)]
pub struct MacAppearance;

impl MacAppearance {
    /// Build the adapter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Appearance for MacAppearance {
    fn accent_rgba(&self) -> Result<[u8; 4], AppearanceError> {
        let mut rgba = [0u8; 4];
        // SAFETY: `rgba` is a caller-owned 4-byte buffer.
        let status = unsafe { crate::ffi::lalela_appearance_accent_rgba(rgba.as_mut_ptr()) };
        let status = crate::ffi::ShimStatus::from_raw(status);
        if status.is_ok() {
            Ok(rgba)
        } else {
            Err(AppearanceError::Platform {
                status: match status {
                    crate::ffi::ShimStatus::Internal => 99,
                    crate::ffi::ShimStatus::Unknown(status) => status,
                    _ => 99,
                },
            })
        }
    }
}
