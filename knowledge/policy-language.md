---
description: Separate structural parsing, semantic validation, and later normalization and authorization contracts.
tags:
- architecture
- policy-language
- validation
title: Policy language parsing and validation boundaries
type: concept
---
# Policy language parsing and validation boundaries

Scryer change `chg-3y8xma` (02) owns structural parsing; `chg-hjb0s4` (03) owns semantic validation. The Policy language component in Scryer is authoritative architecture. This page records the approved public contract.

## Structural parsing

`parse_policy(source, input)` returns an owned, read-only `ParsedPolicy` or a source-located `ParseError`. The source path is a diagnostic label. Parsing performs no loading, execution, interpolation, normalization, matching, DNS, or authorization.

The YAML shape carries a required unsigned decimal version, ordered rules, optional per-family defaults, rule IDs/descriptions, distinct allow/deny/ask effects, canonical actions, and File/Command/Git/Network selectors. Missing defaults and optional fields remain absent. The detailed syntax and executable examples live in `crates/policy-language/README.md`.

Structural parsing rejects malformed YAML, unknown fields/names, duplicate decoded mapping keys, tags, anchors, aliases, merge keys, and multiple documents. Input is bounded to 1 MiB and 64 nested collections. It deliberately preserves unsupported versions, duplicate/blank IDs, empty actions, incompatible action/resource pairs, zero ports, and uninterpreted selector strings for the next stage. `policy-core` remains independent of YAML; the parser uses marked events rather than a mapping loader that discards duplicate keys.

## Semantic validation

`validate_policy(ParsedPolicy)` consumes a parsed document and returns `ValidatedPolicy` or one `ValidationError`. Validated aggregates have private fields and read-only accessors. Only version 1 is supported. IDs must be nonblank and control-free, with exact, case-sensitive document-local uniqueness; no trimming occurs. Rules need at least one action, each compatible with the selector's family. Duplicate actions and source order are preserved, as are descriptions, effects, absent defaults, and field locations.

Validation checks version first, then each rule's ID, actions, and selector fields. Executable precedes arguments; host precedes port. Errors carry typed categories, source, one-based character locations, and field paths. Duplicate IDs include the first location. Display omits scalar values and snippets. Validation performs no I/O or permission decision.

## Selector contract for downstream consumers

- File and Git share typed repository-relative path patterns. `/` separates segments; `.` alone denotes root. `*` stays within a segment, `?` consumes one Unicode scalar, and whole-segment `**` consumes zero or more complete segments. Patterns cover the entire repository-relative identity; hidden names have no special exclusion. Absolute/drive-prefixed paths, backslashes, parent traversal, embedded dot/empty segments, trailing separators, embedded globstars, character classes, braces, leading negation, and controls are rejected.
- Path spelling is preserved. Change 04 and later matching must apply actual filesystem case semantics consistently to resources and selectors. Validation does not probe or guess host behavior. Outside-root identities cannot match these patterns; their identity and default outcomes remain separate concerns.
- Commands use literal executables and argv. Omitted arguments leave argv unconstrained; supplied lists mean exact length/order/values, including empty strings. Executables must be nonblank, and neither executables nor arguments may contain NUL. No expansion, shell parsing, or PATH lookup occurs.
- Network hosts are exact ASCII DNS names or typed IPv4/unbracketed IPv6 literals. DNS identity is lowercase without one optional final dot; DNS labels are 1–63 bytes and names at most 253 bytes excluding that dot. Numeric/dot-only input must parse as an IP. Wildcards, URLs, CIDR, zones, embedded ports, and malformed names are rejected. Host identity equality is exposed separately from the original spelling. Explicit ports are nonzero u16 values; absence means any port. No DNS resolution occurs.

## Boundary and evidence

Validation is language validity, never permission. Normalization, symlink handling, matching, hierarchy/defaults (change 07), Cedar compilation, and authorization remain downstream responsibilities. No resource families or dependencies were added for semantic validation.

Evidence: `crates/policy-language/tests/parsing.rs` preserves the structural boundary; `crates/policy-language/tests/validation.rs` exercises all action/resource pairs, selector boundaries, effects/defaults, identity equivalence, error ordering, Unicode and CRLF. The crate guide includes executable and compile-fail API examples.
