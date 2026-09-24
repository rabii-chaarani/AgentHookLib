# AgentHookLib

Portable policy enforcement contracts for coding agents. The Rust workspace
contains `policy-core`, a dependency-free library for authorization contracts and
the `PolicyService` interface, and `policy-language`, a strict YAML parser and
semantic validator with typed, source-located policy representations. The
`policy-normalizer` library resolves file identities from explicit operation
context using read-only filesystem metadata on macOS, Linux, and Windows.

Request constructors validate structure, including action/resource compatibility
and rename destinations. They preserve supplied values and perform no I/O or
normalization. A valid request is not permission to execute. The future policy
service evaluates permission; the coding-agent runtime owns execution and its
sandbox. Evaluation, auditing, adapters, and the executable CLI remain planned
work in Scryer.

The public API is re-exported by `policy_core`. Aggregates have fallible
constructors and read-only accessors. `PolicyService::authorize` returns
`Result<PolicyDecision, AuthorizationError>`: approval required, policy denial,
and evaluation failure are distinct outcomes. An error or approval requirement
must never be interpreted as permission. Callers must sanitize diagnostics before
constructing authorization errors; these types do not perform log redaction.

`policy_language::parse_policy(source, input)` parses explicit rules and optional
per-resource defaults, preserving `allow`, `deny`, and `ask`. It rejects malformed
YAML, unknown fields/names, duplicate mapping keys, tags, anchors, aliases, and
merge keys. Parsing performs no I/O or execution and does not establish semantic
validity or permission. `validate_policy(parsed)` separately checks version 1,
rule IDs, action/resource compatibility, and selector grammar, returning an
immutable `ValidatedPolicy`. Its selectors support repository-relative path
globs, literal executable/argv values, and exact DNS/IP destinations with optional
nonzero ports. Validation does not normalize filesystem paths, match resources,
or authorize operations. See the [policy syntax and API guide](crates/policy-language/README.md)
for executable examples, selector boundaries, and diagnostics.

`policy_normalizer::paths::normalize_path` returns a canonical native path,
repository-relative identity when inside the root, and per-component naming
semantics. It resolves symlinks before parent traversal and permits a missing
leaf only beneath an existing parent. Unknown filesystem or Unicode naming
rules fail closed. `normalize_file_resource` normalizes both rename endpoints.
These are snapshots, not authorization or execution-time race protection; final
symlinks identify referents rather than unlink/rename entries. See the
[normalization contract](crates/policy-normalizer/README.md) for platform support,
errors, and safe downstream usage.

Development uses the pinned Rust 1.98.1 toolchain and cargo-nextest (verified with
0.9.140). `policy-core` remains dependency-free; `policy-language` uses
`yaml-rust2` 0.13.0 with default features disabled. Run `cargo fetch --locked`
once to populate the dependency cache, then from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --doc --locked --offline
cargo nextest run --workspace --profile ci --locked --offline
cargo metadata --format-version 1 --locked --offline
```

The nextest `ci` profile disables retries and writes
`target/nextest/ci/junit.xml` for Scryer test evidence. The workspace lockfile is
kept in version control. Add other workspace crates only when implementing their
Scryer changes.
