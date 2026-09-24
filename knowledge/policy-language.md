---
description: The structural parser contract and its separation from policy semantics and authorization.
tags:
- policy-language
- architecture
- validation
title: Policy language parsing boundary
type: concept
---
# Policy language parsing boundary

Scryer change `chg-3y8xma` (02) owns structural parsing in `policy-language`. Change `chg-hjb0s4` (03) owns semantic validation. The Scryer Policy language component is the authoritative architecture; this page records the agreed public contract.

`parse_policy(source, input)` returns an owned, read-only `ParsedPolicy` or a source-located `ParseError`. The source path is a diagnostic label: parsing performs no file loading, execution, interpolation, normalization, matching, DNS, or authorization.

The YAML shape has a required unsigned decimal version and ordered rules, with optional per-family defaults. Rules contain identifiers, optional descriptions, distinct allow/deny/ask effects, canonical `policy_core::Action` values, and explicitly typed File/Command/Git/Network selectors. Missing defaults and optional selectors remain absent; parsing must never manufacture fallback permissions. The detailed syntax lives in `crates/policy-language/README.md` and its executable Rust examples.

Structural parsing rejects malformed YAML, unknown fields/names, duplicate decoded mapping keys, explicit tags, anchors, aliases, merge keys, and multiple documents. Input is bounded to 1 MiB and 64 nested collections. Diagnostics retain one-based character locations and field paths, with both locations for duplicate keys.

A parsed policy is not semantically validated. Unsupported version numbers, duplicate or blank rule IDs, empty action lists, incompatible action/resource combinations, zero ports, and uninterpreted selector patterns deliberately survive this stage. Change 03 must validate before a compiler or authorization service consumes these values. Global fallback semantics belong to policy hierarchy (change 07), not the parser.

`policy-core` remains independent of YAML. The parser uses yaml-rust2 marked events rather than a mapping loader that could discard duplicate keys. Regression tests cover source-location normalization, including Unicode and CRLF.

Evidence: `crates/policy-language/tests/parsing.rs`, especially `leaves_semantic_validation_to_change_03`, `preserves_absence_without_inserting_defaults`, and `diagnoses_exact_fields_and_duplicate_origins`.
