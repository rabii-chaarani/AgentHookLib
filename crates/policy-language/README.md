# Policy language

`policy-language` parses one UTF-8 YAML policy into an owned, read-only
`ParsedPolicy`. It performs no file access, execution, interpolation, path
normalization, DNS resolution, or output. The source path is a diagnostic label.

```rust
use std::path::Path;
use policy_language::{parse_policy, Effect};

let policy = parse_policy(Path::new("agent-policy.yaml"), r#"
version: 1
defaults:
  file: deny
  command: ask
rules:
  - id: allow-source-edits
    description: Allow source editing
    effect: allow
    actions: [file.write]
    resource:
      kind: file
      pattern: "src/**"
"#)?;
assert_eq!(policy.version(), 1);
assert_eq!(policy.rules()[0].effect(), Effect::Allow);
assert!(policy.location("rules[0].resource.pattern").is_some());
# Ok::<(), policy_language::ParseError>(())
```

## Structure

`version` and `rules` are required. `version` is a decimal `u64`; quoted decimal
digits are also accepted. `rules` is an ordered sequence, which may be empty.
`defaults` is optional; each of its `file`, `command`, `git`, and `network` entries
is independently optional and accepts `allow`, `deny`, or `ask`. No fallback is
inserted. An absent defaults mapping remains distinct from an empty mapping.

Each rule requires `id`, `effect`, `actions`, and `resource`. `description` is an
optional string. Effects accept exactly `allow`, `deny`, and `ask`. Actions are
an ordered sequence of the following case-sensitive names:

| Family | Action names |
| --- | --- |
| File | `file.read`, `file.write`, `file.delete`, `file.rename` |
| Command | `command.execute` |
| Git | `git.commit`, `git.checkout`, `git.reset`, `git.push`, `git.reset-hard`, `git.force-push` |
| Network | `network.connect` |

Each resource mapping requires `kind` and the corresponding fields below:

| Kind | Required fields | Optional fields |
| --- | --- | --- |
| `file` | `pattern`: string | none |
| `command` | `executable`: string | `arguments`: ordered string sequence |
| `git` | `repository`: string | none |
| `network` | `host`: string | `port`: decimal `u16` |

All textual scalars preserve decoded YAML text, including numeric-looking and
boolean-looking text. Quoted escapes and block-scalar folding follow YAML
decoding; original quote style and comments are not retained. Explicit nulls,
including implicit empty values, are rejected wherever a value is expected.
Quote literal `null` or `~` strings. Optional fields must be omitted rather than
set to null. Unknown fields and duplicate decoded mapping keys are errors.

## Parsing is not semantic validation

The parser recognizes the structure and known names only. It deliberately
preserves unsupported version numbers, duplicate or blank rule IDs, empty action
lists, incompatible action/resource combinations, zero ports, and uninterpreted
selector strings. `validate_policy` is a separate, consuming semantic check.
Selector matching, filesystem normalization, policy layering, and authorization
belong to later stages. **Neither a parsed nor a validated policy is permission
to execute.**

Aggregates expose read-only accessors:

```compile_fail
# use std::path::Path;
# use policy_language::parse_policy;
let mut policy = parse_policy(Path::new("policy.yaml"), "version: 1\nrules: []").unwrap();
policy.version = 2;
```

## Semantic validation

`validate_policy(ParsedPolicy) -> Result<ValidatedPolicy, ValidationError>` accepts
version `1` only. Rules require a nonblank, control-free ID, unique within the
document, at least one action, and a selector compatible with every action.
IDs compare exactly, without trimming or case folding. Repeated actions retain
their order. Unknown effects are structural errors; valid `allow`, `deny`, and
`ask` effects remain distinct. Absent defaults remain absent, including the
distinction between an absent mapping and an empty one. Policy hierarchy owns
fallback behavior.

```rust
use std::path::Path;
use policy_language::{parse_policy, validate_policy, ValidatedResourceSelector, PathSegment};

let parsed = parse_policy(Path::new("agent-policy.yaml"), r#"
version: 1
rules:
  - id: source-edits
    effect: allow
    actions: [file.write]
    resource: {kind: file, pattern: "src/**"}
"#)?;
let policy = validate_policy(parsed)?;
assert!(policy.defaults().is_none());
let ValidatedResourceSelector::File(pattern) = policy.rules()[0].resource() else {
    panic!("expected a file selector");
};
assert_eq!(pattern.original(), "src/**");
assert_eq!(pattern.segments()[1], PathSegment::Recursive);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Validated policies, rules, and selector payloads have private fields and expose
read-only accessors. Construct them through validation, not a struct literal or
an unchecked constructor. Source labels and field locations survive validation.

```compile_fail
# use std::path::Path;
# use policy_language::{parse_policy, validate_policy};
let mut policy = validate_policy(parse_policy(Path::new("p"), "version: 1\nrules: []").unwrap()).unwrap();
policy.version = 2;
```

```compile_fail
# use policy_language::{ValidatedRule, ValidatedResourceSelector};
fn replace_selector(rule: &mut ValidatedRule, resource: ValidatedResourceSelector) {
    rule.resource = resource;
}
```

```compile_fail
use policy_language::PathPattern;
let unchecked = PathPattern { original: "../outside".into(), segments: vec![] };
```

### File and Git selectors

Both selectors use the same repository-relative path grammar and typed
`PathPattern` / `PathSegment` / `PathToken` representation. `/` separates
segments, and `.` alone denotes the repository root (an empty segment list).
Patterns are anchored to the repository root and cover the entire identity;
they are not substring searches. The defined matching semantics for consumers
are:

| Construct | Meaning |
| --- | --- |
| Literal | Exact text within a segment, subject to filesystem case semantics |
| `*` | Zero or more Unicode scalar values within one segment; never `/` |
| `?` | One Unicode scalar value within one segment; never `/` |
| `**` | A whole segment matching zero or more complete path segments |

For example, `src/**` includes `src` and its descendants; `a/**/b` includes
`a/b`. Hidden names receive no special exclusion. Spelling and Unicode are
preserved without case folding or Unicode normalization. Later normalization
and matching must apply the actual host filesystem's case semantics consistently
to selectors and resources; operating-system guesses must not replace that
contract. This crate does not probe the filesystem or implement matching.

Reject empty patterns, absolute/drive-prefixed paths, backslashes (including
escape syntax), `..`, embedded `.` segments, repeated/trailing separators,
embedded globstars (`a**`, `***`), character classes (`[]`), braces (`{}`),
leading `!`, and control characters. These constructs are errors rather than
alternate syntax or literals. Spaces and other literal punctuation remain
literal. Use `/` even on Windows. Outside-root identities cannot match these
patterns; their normalization belongs to change 04, and defaults to change 07.

### Command selectors

The executable is a nonblank literal string without NUL. No shell parsing,
interpolation, PATH lookup, case folding, trimming, or command execution occurs.
The later consumer must compare it to the supplied executable value literally.
Omitted `arguments` means unconstrained argv; an explicit list means exact
length, order, and values. `arguments: []` means no arguments. Empty argument
strings are valid; NUL is rejected. `*`, `${HOME}`, and `$(...)` in arguments
remain literal text, not globs or expressions.

### Network selectors

Hosts are exact ASCII DNS names, IPv4, or unbracketed IPv6 literals. DNS labels
contain ASCII letters, digits, and internal hyphens, with 1–63 bytes per label
and at most 253 bytes excluding an optional single trailing dot. Single-label
names and ASCII punycode labels are accepted. All-numeric/dot strings must be
valid IP literals; malformed numeric addresses are not reinterpreted as DNS.
Non-ASCII names, wildcards, URLs, CIDR, zone IDs, embedded ports, empty labels,
and leading/trailing label hyphens are rejected. An unbracketed string that is
a valid IPv6 address is treated entirely as an address; specify ports only in
the `port` field.

`NetworkSelector::original_host()` retains the input. `host()` returns a
`NetworkHost` whose equality uses lowercase DNS without the trailing dot, or
`std::net::IpAddr`. Compare these host identities, not the enclosing selector's
equality (which also retains original spelling). Different IP address families
remain distinct. Explicit ports are `NonZeroU16` values in `1..=65535`; omission
leaves the port unconstrained. No DNS lookup or network access occurs.

### Semantic diagnostics

Validation returns one deterministic `ValidationError`. It checks version first,
then rules in document order: ID validity and uniqueness, nonempty actions and
compatibility in action order, then selector fields. The command executable
precedes arguments in order; network host precedes port. Errors identify source,
field path, and the original one-based line/character column. Duplicate IDs also
identify the first occurrence. Error display includes the category and locations
without scalar values or source snippets. Escape source labels appropriately
when displaying them in a UI or log.

## YAML subset and diagnostics

Accept exactly one non-null YAML document, limited to 1 MiB of UTF-8 input and
64 nested mappings/sequences. Reject aliases, anchors, all explicit tags (even
standard tags), merge keys, and non-scalar mapping keys. No includes, custom
constructors, expressions, or environment expansion are supported.

Errors contain a typed category, the supplied source path, a one-based line and
character column, and a field path when available. Duplicate-key diagnostics
also identify the first key's location; missing-field diagnostics point to the
enclosing mapping. Paths use `$` for the document and otherwise forms such as
`rules[0].actions[1]`. `ParsedPolicy::location` locates values and collections;
absent optional fields have no location. Unknown/duplicate-field errors point
to keys. YAML scanner errors use the location reported by the scanner.

The parser returns one deterministic error. YAML syntax and subset checks run
before typed decoding. During typed decoding, unknown fields are checked in
source order and known fields in schema order. Error precedence is not a promise
to return the textually earliest issue across both phases. Scalar payloads and
source snippets are omitted from error formatting; source labels and field paths
are caller/input supplied and should be escaped appropriately by a UI or logger.
