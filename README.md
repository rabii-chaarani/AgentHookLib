# AgentHookLib

Portable policy enforcement contracts for coding agents. The current workspace
contains only `policy-core`: a dependency-free Rust library for principals,
actions, resources, context, authorization requests, decisions, and the
`PolicyService` interface.

Request constructors validate structure, including action/resource compatibility
and rename destinations. They preserve supplied values and perform no I/O or
normalization. A valid request is not permission to execute. The future policy
service evaluates permission; the coding-agent runtime owns execution and its
sandbox. Parsing, evaluation, auditing, adapters, and the executable CLI remain
planned work in Scryer.

The public API is re-exported by `policy_core`. Aggregates have fallible
constructors and read-only accessors. `PolicyService::authorize` returns
`Result<PolicyDecision, AuthorizationError>`: approval required, policy denial,
and evaluation failure are distinct outcomes. An error or approval requirement
must never be interpreted as permission. Callers must sanitize diagnostics before
constructing authorization errors; these types do not perform log redaction.

Development uses the pinned Rust 1.98.1 toolchain and cargo-nextest (verified with
0.9.140). The crate has no runtime or test dependencies. From the repository root:

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
