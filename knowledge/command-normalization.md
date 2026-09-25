---
description: Public classification contract for parsed command operations and change 06 Git variants.
tags:
- architecture
- command-normalizer
- git
- authorization
title: Command normalization and destructive Git classification
type: concept
---
# Command normalization and destructive Git classification

The Command normalizer component in Scryer (`node-kmct43`) owns pure parsing and semantic classification. Change `chg-as81s2` adds the force-push and hard-reset claims. `policy_normalizer::commands::parse_command_line` returns ordered operations with the supplied context and each original executable and argument vector. Classification is not permission and never invokes Git or a shell.

## Git action contract

Exact `git push` and `git reset` subcommands are classified from parsed argument tokens. Push options `--force`, `-f` (including short-option groups), and `--force-with-lease` classify as `GitAction::ForcePush`; a leading `+` on a refspec also does so. `git reset --hard` classifies as `GitAction::ResetHard`. Supported unambiguous abbreviations include `--force-w` for the lease option and `--har` for hard reset. `--force-if-includes` alone is not a force flag. Explicit `--no-force` and `--no-force-with-lease` cancel their respective earlier flags in argument order; a forced refspec remains forced.

Option values and tokens after `--` are not interpreted as option flags. A `+` refspec remains meaningful after `--`. Ambiguous or unsupported option syntax returns `UnsupportedOperation` with a byte offset and no partial operation list; command text is not echoed. The parser retains supported wrappers and all constituents of supported compound commands for separate policy evaluation.

The canonical action variants live in `policy-core`. Policy evaluation, adapter protocol mapping, execution, and sandbox ownership are downstream responsibilities. The source contract and examples are in `crates/policy-normalizer/README.md`; integration tests are in `crates/policy-normalizer/tests/commands.rs`.