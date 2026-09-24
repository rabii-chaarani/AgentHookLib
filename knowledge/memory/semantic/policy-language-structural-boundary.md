---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: active
  owner: policy-language
  created_at: 2026-09-24T03:20:40Z
  last_verified_at: 2026-09-24T03:22:00Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: scryer
    reference: chg-3y8xma and chg-hjb0s4; node-qtv7gb
    content_hash: null
  - kind: wiki
    reference: policy-language.md
    content_hash: e5c060e42b3b3ebe7e2fc343a949323e448f4055a14546a395ded8925da3009d
  - kind: test
    reference: crates/policy-language/tests/parsing.rs::leaves_semantic_validation_to_change_03
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-24T03:22:00Z
    reason: Reviewed against the approved change 02 plan, change 03's remaining Scryer responsibilities, the curated policy-language page, and passing semantic-boundary/default-preservation tests.
description: Change 02 preserves semantically invalid values intentionally; parsing success is never permission.
tags:
- architecture
- policy-language
- validation
timestamp: 2026-09-24T03:20:40Z
title: Parsed policies require a separate semantic validation stage
type: agent-memory
---
The agreed architecture separates structural policy parsing (change 02) from semantic validation (change 03). `ParsedPolicy` preserves unsupported version numbers, duplicate/blank rule IDs, empty action lists, incompatible action/resource pairs, zero ports, and uninterpreted selector strings. Unknown field/action/effect/kind names are rejected structurally. Missing defaults remain absent; hierarchy (change 07) owns fallback semantics. Do not add authorization or fallback behavior to the parser or treat parsing success as semantic validity. The separation is intentional and verified by the semantic-boundary and missing-default regression tests.