//! Read-only, agent-neutral resource normalization.
//!
//! Identities describe a filesystem snapshot, not permission or execution-time
//! isolation. Final symbolic links identify their referents, including for
//! delete and rename descriptions. See [`paths`] and [`commands`] for the
//! public normalization contracts.
#![doc = include_str!("../README.md")]

pub mod commands;
pub mod paths;
