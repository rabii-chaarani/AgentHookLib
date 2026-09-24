# AgentHookLib

Portable policy enforcement contracts for coding agents. The Rust workspace
contains `policy-core`, a dependency-free library for authorization contracts and
the `PolicyService` interface, and `policy-language`, a strict YAML parser with a
typed, source-located policy AST.

Request constructors validate structure, including action/resource compatibility
and rename destinations. They preserve supplied values and perform no I/O or
normalization. A valid request is not permission to execute. The future policy
service evaluates permission; the coding-agent runtime owns execution and its
sandbox. Semantic policy validation, evaluation, auditing, adapters, and the executable CLI remain
planned work in Scryer.

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
validity or permission. See the [policy syntax and API guide](crates/policy-language/README.md)
for selectors, diagnostics, limits, and the boundary with change 03.

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
