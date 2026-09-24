//! Private read-only platform metadata operations. Unsafe is confined to native wrappers.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub(super) use linux::naming;
#[cfg(target_os = "macos")]
pub(super) use macos::naming;
#[cfg(windows)]
pub(super) use windows::{naming, parse, redirect, stored_name};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(super) use unix::{parse, redirect, stored_name};

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub(super) fn naming(
    _: &std::path::Path,
) -> Result<super::NamingSemantics, super::NormalizationError> {
    Err(super::NormalizationError::UnsupportedPlatform)
}
