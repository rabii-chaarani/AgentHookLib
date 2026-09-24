# File identity normalization

`policy_normalizer::paths` resolves file resources using the caller's explicit
repository root and operation working directory. It reads filesystem metadata;
it does not create files, execute operations, evaluate policies, or grant access.

```no_run
use std::path::Path;
use policy_core::Context;
use policy_normalizer::paths::{normalize_path, PathRequirement};

fn inspect(context: &Context) -> Result<(), Box<dyn std::error::Error>> {
    let file = normalize_path(Path::new("src/new.rs"), context,
                              PathRequirement::AllowMissingLeaf)?;
    if let Some(relative) = file.repository_relative() {
        // Supply this identity and its naming semantics to policy evaluation.
        assert!(!relative.is_absolute());
    }
    // Outside-root identities require separate policy handling, never rebasing.
    Ok(())
}
```

`PathRequirement::Existing` requires an existing target.
`AllowMissingLeaf` permits one absent final filename beneath an existing
directory. Missing parents, including `missing/../file`, are errors. Repository
root and cwd must both resolve to existing directories. Files cannot be used as
ancestors, including through `file/..` or `file/.`.

`normalize_file_resource` requires existing read/delete targets and rename
sources. Write targets and rename destinations may have missing leaves. A
rename returns both normalized endpoints or an error; no partial result escapes.
`to_file_resource()` converts back to the unchanged structural `policy-core`
contract, explicitly discarding normalization metadata.

## Identity and naming

Each result carries the canonical absolute native path, optional repository-
relative path (`.` at root), existence status, and per-component naming rules.
Outside-root identities have no repository-relative path. Comparison uses
`PathIdentity`, not display text or string-prefix containment. Equal missing
names may retain different spellings on an insensitive filesystem.

Both root and cwd are resolved before relative resource paths. Symlinks are
resolved before subsequent parent segments, including links escaping the root.
Final symlinks identify their existing referents. Dangling links and unresolved
reparse points fail; they are never treated as missing writable files. Hard-link
names remain distinct because policies describe paths, not inodes.

Naming rules come from actual parent-directory metadata on macOS, Linux, and
Windows. Supported local modes are APFS/HFS+, ext2/3/4, XFS, Btrfs, tmpfs, and
NTFS. XFS uses the filesystem geometry's ASCII-CI flag; unavailable or
unknown-version geometry returns an error. Unknown or remote filesystem
semantics fail closed. Native exact modes
preserve code units, including non-UTF-8 Unix names. Modes whose Unicode
equivalence is not implemented accept ASCII names only; unsupported names return
`UnsupportedNamingSemantics`. No lossy decoding or general Unicode lowercasing
is used. APFS/HFS+ sensitive modes also restrict names to ASCII because their
Unicode normalization is not native byte equality.

`NamingSemantics::name_key` is the fallible, single-name operation shared by
resource identity and future selector-literal comparison. Apply the semantics
of each name's parent; do not assume one case flag for an entire repository.
Glob interpretation and policy matching remain downstream responsibilities.

## Command parsing

`policy_normalizer::commands::parse_command_line` parses a bounded POSIX-like
command form without starting a shell or executing a command. It accepts quoted
words and the `;`, `&&`, `||`, and `|` separators. Separators produce an ordered
list of operations; callers must evaluate every operation before allowing the
original command. The parser does not authorize commands.

The simple `env`, `command`, `exec`, and `time` wrappers are recognized and
retained in each operation's executable and argument vector. Wrapper options,
environment assignments, nested shells, and known wrappers such as `sudo`,
`nice`, and `nohup` are unsupported. Other unrecognized executables remain
generic command operations and are never unwrapped. State-changing shell
built-ins such as `cd`, `export`, and `source` are unsupported, since they can
change how later operations in a compound run.
Exact `git commit`, `git checkout`, `git reset`, and `git push` subcommands map
to their semantic Git actions. Other valid commands remain generic command
operations. Force-push and hard-reset forms return an unsupported-operation
error until their dedicated classifications are implemented.

Variable, command, arithmetic, and process substitutions; unquoted globbing;
redirection; background execution; shell control structures; and malformed
quoting are rejected with a typed error and byte offset. Errors never include
the supplied command text. A failed parse returns no partial operation list.
PowerShell and `cmd.exe` syntax is outside this grammar.

Windows accepts absolute drive, UNC, and supported extended-length forms.
Root-relative inputs use the explicit cwd's drive; drive-relative inputs,
device namespaces, alternate streams, and ambiguous legacy names are rejected.
UNC support does not imply remote filesystem semantics can be verified.

## Limits and failures

These are filesystem snapshots, not stable execution capabilities. Filesystem
changes after inspection can invalidate them. Runtime isolation and action-time
race protection belong to the coding-agent runtime. A final-link referent is
not a complete description of the entry changed by unlink or rename; adapters
must not substitute this path for the original operation to execute it.

Failures are typed and omit supplied paths and OS diagnostic strings. An error,
outside-root result, or structurally valid core request never grants permission.
The library produces no stdout/stderr output. Unsafe code is prohibited in the
shared logic and confined to documented platform metadata wrappers.

Normalized values cannot be constructed or mutated through public fields:

```compile_fail
use policy_normalizer::paths::NormalizedPath;
fn change(path: &mut NormalizedPath) {
    path.exists = true;
}
```

Run the workspace format, Clippy, doctest, and nextest checks documented in the
root README. CI runs native suites on macOS, Linux, and Windows and uploads the
JUnit evidence. Tests of unsupported filesystem modes do not claim support for
those modes.
