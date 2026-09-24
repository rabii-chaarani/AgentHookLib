---
agent_memory:
  version: 1
  kind: episodic
  scope: repository
  status: active
  owner: path-normalizer
  created_at: 2026-09-24T06:05:52Z
  last_verified_at: 2026-09-24T06:05:52Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: pull-request
    reference: https://github.com/rabii-chaarani/AgentHookLib/pull/3#discussion_r4090378649
    content_hash: null
  - kind: test
    reference: crates/policy-normalizer/tests/xfs.rs::native_xfs_modes_preserve_equivalence_and_stored_spelling
    content_hash: null
  - kind: ci
    reference: https://github.com/rabii-chaarani/AgentHookLib/actions/runs/35962729192
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-24T06:05:52Z
    reason: Verified against the upstream XFS UAPI and geometry implementation, reviewed code at 62c3a8a, passing Linux regression tests, and the native CI job on both exact and ASCII-CI XFS mounts.
description: The XFS review regression showed why naming modes require metadata and real filesystem fixtures.
tags:
- case-semantics
- paths
- regression
- xfs
timestamp: 2026-09-24T06:05:52Z
title: XFS filesystem type alone does not establish case sensitivity
type: agent-memory
---
PR #3 exposed an incorrect unconditional Exact classification for XFS. XFS can use ASCII case-insensitive naming; selecting Exact also bypassed stored-name enumeration, splitting case aliases into different resource identities. The repair queries XFS_IOC_FSGEOMETRY, requires the supported geometry version, and selects ASCII-insensitive semantics from DIRV2CI. Query failure or unknown versions return an error rather than an exact-mode fallback. When adding filesystem backends, test naming capabilities rather than assuming a filesystem name implies one case mode. The dedicated XFS CI job exercises real ordinary and ASCII-CI mounts, including existing aliases, stored spelling, and nonexistent leaf identities.