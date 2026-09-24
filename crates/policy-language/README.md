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
selector strings. Semantic validation is Scryer change 03; selector matching,
case sensitivity, normalization, policy layering, and authorization belong to
later stages. **A parsed policy is never permission to execute.**

There is no glob implementation in this crate. Examples containing `src/**` or
other patterns demonstrate preserved selector text, not implemented matching.

Aggregates expose read-only accessors:

```compile_fail
# use std::path::Path;
# use policy_language::parse_policy;
let mut policy = parse_policy(Path::new("policy.yaml"), "version: 1\nrules: []").unwrap();
policy.version = 2;
```

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
