---
description: Canonical file identities, naming semantics, and normalization boundaries for change 04.
tags:
- architecture
- normalization
- paths
- public-contract
title: File identity normalization
type: concept
---
# File identity normalization

The Path normalizer component in Scryer (`node-kff7hb`, change `chg-bb1frj`) owns read-only file identity normalization. Its public API lives in `policy_normalizer::paths`.

## Contract

`normalize_path(path, context, requirement)` establishes an immutable canonical native path, comparison identity, existence status, per-component naming semantics, and an optional repository-relative path. Repository root is `.`; outside-root identities have no relative path. Both the explicit repository root and operation cwd must resolve to existing directories. No environment or repository discovery occurs.

`Existing` requires the target to exist. `AllowMissingLeaf` permits only one absent final filename beneath an existing parent. Missing parents are rejected even if followed by `..`; a regular file cannot be traversed as a directory. Symlinks resolve before subsequent parent segments. Final links identify existing referents, and dangling links are errors. Hard-link names remain distinct because policies describe paths rather than inodes.

`normalize_file_resource` requires existing read/delete targets and rename sources; writes and rename destinations may have a missing leaf. Rename endpoints are returned together or not at all. An explicit conversion back to `FileResource` discards normalization metadata; core constructors continue to validate structure only.

## Naming and selector boundary

Canonical spelling and equality are separate. Comparison keys use verified per-parent filesystem naming rules, never string-prefix containment, lossy decoding, operating-system defaults, or generic Unicode lowercasing. The same fallible `NamingSemantics::name_key` operation is available to downstream selector-literal comparison. Matching and glob expansion remain later responsibilities.

Platform backends query local APFS/HFS+ metadata on macOS, filesystem type, ext directory flags, and XFS filesystem geometry on Linux, and NTFS volume/directory metadata on Windows. Exact modes preserve native code units. Modes whose Unicode equivalence is not established accept ASCII only and otherwise fail closed. XFS requires a successful version-5 `XFS_IOC_FSGEOMETRY` response: `DIRV2CI` selects ASCII-insensitive naming, and its absence selects exact naming. Unsupported queries or unknown geometry versions fail closed; filesystem type alone never establishes XFS case sensitivity. Unknown or remote naming rules are not guessed. Unsafe native calls are isolated in audited platform wrappers; shared logic remains safe Rust.

## Operational boundary

Normalization reads metadata and never creates, modifies, or executes a file. Errors contain no input paths or raw OS text; the library emits no stdout/stderr output. A normalized value is not permission. It is also not execution-time isolation: later filesystem changes may invalidate it. Final-link referent identity is not a complete model of the directory entry affected by unlink/rename, and callers must not substitute it for the original operation to execute.

The approved implementation choices require macOS, Linux, and Windows support, errors for unverified naming semantics, existing parents for new targets, and narrowly audited FFI. Native CI on all three platforms is required before declaring cross-platform completion.

Evidence: `crates/policy-normalizer/README.md`, public API doctests, shared integration tests in `crates/policy-normalizer/tests/paths.rs`, and platform backend tests. [Policy language](policy-language.md) preserves selector spelling and delegates filesystem semantics to this boundary.
