---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: active
  owner: path-normalizer
  created_at: 2026-09-24T05:31:29Z
  last_verified_at: 2026-09-24T05:31:29Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: scryer
    reference: chg-bb1frj; node-kff7hb directives and approved implementation plan
    content_hash: null
  - kind: wiki
    reference: path-normalization.md
    content_hash: e80f0ede7326e07353c73e2035f45c0c7db27aa8800fd2e495de77d8e876633b
  - kind: test
    reference: crates/policy-normalizer/tests/paths.rs::missing_leaf_requires_an_existing_parent; final_links_identify_referents_and_dangling_links_never_become_new_files; root_and_component_containment_do_not_rebase_outside_paths
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-24T05:31:29Z
    reason: Reviewed against the approved change 04 contract, binding Scryer directives, immutable API, and passing local integration tests for missing parents, link referents, and outside-root identities. This records the architectural boundary, not an assertion of completed native cross-platform verification.
description: Preserve change 04's boundary between canonical resource identity, permission, and action-time entry semantics.
tags:
- normalization
- paths
- public-contract
timestamp: 2026-09-24T05:31:29Z
title: Normalized paths are referent snapshots, not execution capabilities
type: agent-memory
---
The approved change 04 contract treats a normalized path as a read-only snapshot of canonical path identity, never authorization or an execution capability. Final links identify existing referents for every file action; this does not fully describe the directory entry changed by unlink/rename, and adapters must not replace the original operation with the normalized path to execute it. AllowMissingLeaf permits only a final filename under an existing parent; missing parent chains and dangling links fail rather than being lexically rebased. Outside-root results remain explicit and have no repository-relative identity. Keep core FileResource construction structural-only; converting a normalized result back to it deliberately discards normalization metadata. Runtime isolation and execution-time race protection remain with the coding-agent runtime.