---
agent_memory:
  version: 1
  kind: semantic
  scope: repository
  status: active
  owner: policy-language
  created_at: 2026-09-24T04:42:13Z
  last_verified_at: 2026-09-24T04:42:13Z
  verified_by: codex
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: scryer
    reference: chg-hjb0s4; node-qtv7gb and node-kff7hb directives
    content_hash: null
  - kind: wiki
    reference: policy-language.md
    content_hash: 708747d3b1394e408df1e57cdf4984f59cf72ffe7ded4ae509be3a70b558287c
  - kind: test
    reference: crates/policy-language/tests/validation.rs::accepts_shared_path_grammar_and_preserves_tokens; command_values_remain_literal_and_absence_is_distinct; accepts_canonical_dns_and_ip_identities_and_port_boundaries
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: codex
    at: 2026-09-24T04:42:13Z
    reason: Reviewed against approved change 03, governing path-normalizer directives, implemented read-only selector types, and passing semantic integration tests. The claim records downstream boundaries rather than transient implementation status.
description: Keep change 03 validation separate from filesystem normalization, matching, and policy composition.
tags:
- normalization
- policy-language
- selectors
- validation
timestamp: 2026-09-24T04:42:13Z
title: Validated selectors establish grammar, not runtime identity or permission
type: agent-memory
---
The approved change 03 contract makes `ValidatedPolicy` a language-valid, immutable document, never a permission or a filesystem-normalized identity. File and Git share repository-relative small globs; their original case is preserved, and downstream normalization/matching must use the actual filesystem's case semantics consistently for both selectors and resources. Do not use the build operating system as a case-sensitivity shortcut. Command values are literal; absent argv means unconstrained while an explicit empty list means no arguments. Network equality belongs to `NetworkHost` (canonical DNS or typed IP), not the enclosing selector, which also retains original spelling. Missing defaults remain absent for policy hierarchy to interpret. Downstream consumers must not add wildcard, shell-expansion, or fallback semantics to these validated values.