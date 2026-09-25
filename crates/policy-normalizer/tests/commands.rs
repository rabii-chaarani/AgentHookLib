//! Contract tests for pure command parsing.
use policy_core::{Action, CommandAction, Context, GitAction};
use policy_normalizer::commands::{CommandParseErrorKind as ErrorKind, parse_command_line};
use std::ffi::OsString;

fn context() -> Context {
    let root = std::env::temp_dir().join("agent-hook-lib-command-tests");
    let working_directory = root.join("repository").join("src");
    let repository_root = root.join("repository");
    Context::new(
        "repository",
        repository_root,
        working_directory,
        Some("feature/parser".to_owned()),
        Some("task-05".to_owned()),
    )
    .unwrap()
}

fn parse(source: &str) -> policy_normalizer::commands::ParsedCommand {
    parse_command_line(source, &context()).unwrap()
}

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn preserves_quoted_and_escaped_argument_boundaries_and_context() {
    let supplied_context = context();
    let parsed = parse_command_line(
        r#"tool "" "two words" 'literal $(touch nope)' escaped\ value "a\"b""#,
        &supplied_context,
    )
    .unwrap();

    assert_eq!(parsed.context(), &supplied_context);
    assert_eq!(parsed.operations().len(), 1);
    let operation = &parsed.operations()[0];
    assert_eq!(operation.action(), Action::Command(CommandAction::Execute));
    assert_eq!(operation.invocation().executable(), "tool");
    assert_eq!(
        operation.invocation().arguments(),
        args(&[
            "",
            "two words",
            "literal $(touch nope)",
            "escaped value",
            "a\"b"
        ])
    );
}

#[test]
fn parses_each_supported_wrapper_and_retains_its_invocation() {
    for wrapper in ["env", "command", "exec", "time"] {
        let parsed = parse(&format!("{wrapper} git commit -m message"));
        assert_eq!(parsed.operations().len(), 1);
        let operation = &parsed.operations()[0];
        assert_eq!(operation.action(), Action::Git(GitAction::Commit));
        assert_eq!(operation.invocation().executable(), wrapper);
        assert_eq!(
            operation.invocation().arguments(),
            args(&["git", "commit", "-m", "message"])
        );
    }
}

#[test]
fn compound_operators_preserve_every_operation_in_source_order() {
    let parsed = parse(
        "git status && echo ok | git commit -m done || git push origin main; command git push branch",
    );

    assert_eq!(parsed.operations().len(), 5);
    assert_eq!(
        parsed
            .operations()
            .iter()
            .map(|operation| operation.action())
            .collect::<Vec<_>>(),
        vec![
            Action::Command(CommandAction::Execute),
            Action::Command(CommandAction::Execute),
            Action::Git(GitAction::Commit),
            Action::Git(GitAction::Push),
            Action::Git(GitAction::Push),
        ]
    );
    assert_eq!(parsed.operations()[0].invocation().executable(), "git");
    assert_eq!(parsed.operations()[1].invocation().executable(), "echo");
    assert_eq!(parsed.operations()[2].invocation().executable(), "git");
    assert_eq!(parsed.operations()[3].invocation().executable(), "git");
    assert_eq!(parsed.operations()[4].invocation().executable(), "command");

    let multiline = parse("first &&\nsecond |\nthird ||\nfourth");
    assert_eq!(multiline.operations().len(), 4);
}

#[test]
fn recognizes_only_exact_supported_git_executables_and_subcommands() {
    let cases = [
        ("git commit -m done", Action::Git(GitAction::Commit)),
        (
            "/usr/bin/git checkout main",
            Action::Git(GitAction::Checkout),
        ),
        ("git reset --soft HEAD", Action::Git(GitAction::Reset)),
        ("git push origin main", Action::Git(GitAction::Push)),
        ("git status", Action::Command(CommandAction::Execute)),
        (
            "mygit push origin main",
            Action::Command(CommandAction::Execute),
        ),
        ("git-helper commit", Action::Command(CommandAction::Execute)),
    ];

    for (source, expected) in cases {
        assert_eq!(parse(source).operations()[0].action(), expected, "{source}");
    }

    let exe_action = if cfg!(windows) {
        Action::Git(GitAction::Push)
    } else {
        Action::Command(CommandAction::Execute)
    };
    assert_eq!(
        parse("GIT.EXE push origin main").operations()[0].action(),
        exe_action
    );
}

#[test]
fn classifies_force_push_forms_in_any_option_position() {
    for source in [
        "git push --force origin main",
        "git push origin main -f",
        "git push -vf origin main",
        "git push origin -fv main",
        "git push -fo tracking origin main",
        "git push --force-with-lease origin main",
        "git push origin main --force-with-lease=refs/heads/main:abc123",
        "git push --force-w=refs/heads/main:abc123 origin main",
        "git push --force-with origin main",
        "git push origin +HEAD:main",
        "git push --repo=origin +HEAD:main",
        "git push --repo origin +HEAD:main",
        "git push origin -- +HEAD:main",
    ] {
        assert_eq!(
            parse(source).operations()[0].action(),
            Action::Git(GitAction::ForcePush),
            "{source}"
        );
    }
}

#[test]
fn classifies_hard_reset_and_preserves_operation_context() {
    let supplied_context = context();
    let parsed = parse_command_line(
        "echo before && command git reset --har HEAD | git push origin -f; echo after",
        &supplied_context,
    )
    .unwrap();
    assert_eq!(parsed.context(), &supplied_context);
    assert_eq!(parsed.operations().len(), 4);
    assert_eq!(
        parsed
            .operations()
            .iter()
            .map(|op| op.action())
            .collect::<Vec<_>>(),
        vec![
            Action::Command(CommandAction::Execute),
            Action::Git(GitAction::ResetHard),
            Action::Git(GitAction::ForcePush),
            Action::Command(CommandAction::Execute),
        ]
    );
    assert_eq!(parsed.operations()[1].invocation().executable(), "command");
    assert_eq!(
        parsed.operations()[1].invocation().arguments(),
        args(&["git", "reset", "--har", "HEAD"])
    );
    assert_eq!(
        parsed.operations()[2].invocation().arguments(),
        args(&["push", "origin", "-f"])
    );
    for source in [
        "git reset --hard HEAD",
        "git reset HEAD --hard",
        "git reset --har HEAD",
    ] {
        assert_eq!(
            parse(source).operations()[0].action(),
            Action::Git(GitAction::ResetHard),
            "{source}"
        );
    }
}

#[test]
fn negations_and_option_boundaries_do_not_invent_force() {
    let plain_push = [
        "git push --force --no-force origin main",
        "git push --force-with-lease --no-force-with-lease origin main",
        "git push --force-if-includes origin main",
        "git push --force-if origin main",
        "git push --no-force-if-includes origin main",
        "git push --repo=--force origin main",
        "git push --repo --force main",
        "git push -o --force origin main",
        "git push -o--force origin main",
        "git push -of origin main",
        "git push --push-option=--force origin main",
        "git push origin -- --force",
        "git push +origin main",
    ];
    for source in plain_push {
        assert_eq!(
            parse(source).operations()[0].action(),
            Action::Git(GitAction::Push),
            "{source}"
        );
    }
    for source in [
        "git push --no-force --force origin main",
        "git push --no-force-with-lease --force-with-lease origin main",
        "git push --force --no-force origin +main",
    ] {
        assert_eq!(
            parse(source).operations()[0].action(),
            Action::Git(GitAction::ForcePush),
            "{source}"
        );
    }
    assert_eq!(
        parse("git reset HEAD -- --hard").operations()[0].action(),
        Action::Git(GitAction::Reset)
    );
}

#[test]
fn ambiguous_or_malformed_git_options_fail_without_partial_operations() {
    for source in [
        "git push --forc origin main",
        "git push --force-with-lease= origin main",
        "git push --force-extra origin main",
        "git push --repo",
        "git push -o",
        "git push --unknown origin main",
        "git reset --ha HEAD",
        "git reset --harder HEAD",
        "git reset --hard=HEAD",
    ] {
        let error = parse_command_line(source, &context()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnsupportedOperation, "{source}");
        assert!(error.offset() < source.len(), "{source}");
    }
    assert!(parse_command_line("echo safe; git push --forc origin main", &context()).is_err());
}

#[test]
fn rejects_unsupported_syntax_and_returns_no_partial_operations() {
    let cases = [
        ("echo $(touch marker)", ErrorKind::UnsupportedSyntax),
        ("echo $HOME", ErrorKind::UnsupportedSyntax),
        ("echo `date`", ErrorKind::UnsupportedSyntax),
        ("echo %1", ErrorKind::UnsupportedSyntax),
        ("echo > output", ErrorKind::UnsupportedSyntax),
        ("echo *.rs", ErrorKind::UnsupportedSyntax),
        ("echo %PATH%", ErrorKind::UnsupportedSyntax),
        ("echo task &", ErrorKind::UnsupportedSyntax),
        ("if true; then echo yes; fi", ErrorKind::UnsupportedSyntax),
        ("bash -c 'echo hidden'", ErrorKind::UnsupportedSyntax),
        ("sh script.sh", ErrorKind::UnsupportedSyntax),
        (
            "powershell -Command Get-ChildItem",
            ErrorKind::UnsupportedSyntax,
        ),
        ("cmd.exe /c dir", ErrorKind::UnsupportedSyntax),
        ("sudo git status", ErrorKind::UnsupportedOperation),
        ("cd /tmp; git push", ErrorKind::UnsupportedOperation),
        (
            "export MODE=unsafe; git push",
            ErrorKind::UnsupportedOperation,
        ),
        (
            "env MODE=unsafe git status",
            ErrorKind::UnsupportedOperation,
        ),
        ("command -v git", ErrorKind::UnsupportedOperation),
        ("time -p git status", ErrorKind::UnsupportedOperation),
        ("tool &&", ErrorKind::MalformedSyntax),
        ("; tool", ErrorKind::MalformedSyntax),
        ("tool;;other", ErrorKind::MalformedSyntax),
        ("tool 'unterminated", ErrorKind::MalformedSyntax),
        ("tool\0arg", ErrorKind::MalformedSyntax),
    ];

    for (source, expected) in cases {
        let error = parse_command_line(source, &context()).unwrap_err();
        assert_eq!(error.kind(), expected, "{source}");
        assert!(error.offset() <= source.len(), "{source}");
    }

    assert!(parse_command_line("tool; git push --forc", &context()).is_err());
}

#[test]
fn reports_empty_commands_and_does_not_echo_input_in_errors() {
    assert_eq!(
        parse_command_line(" \n ", &context()).unwrap_err().kind(),
        ErrorKind::EmptyCommand
    );

    let source = "echo SECRET_VALUE $(touch marker)";
    let error = parse_command_line(source, &context()).unwrap_err();
    assert!(!error.to_string().contains("SECRET_VALUE"));
    assert!(error.offset() < source.len());
}

#[test]
fn parsing_substitution_text_never_creates_a_side_effect() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("executed");
    let source = format!("echo $(touch {})", marker.display());

    let error = parse_command_line(&source, &context()).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::UnsupportedSyntax);
    assert!(!marker.exists());
}

#[test]
fn accepts_expansion_characters_as_single_quoted_literal_text() {
    let parsed = parse("echo '$(touch marker) $HOME *.rs %PATH% !important! ^'");
    assert_eq!(parsed.operations().len(), 1);
    assert_eq!(
        parsed.operations()[0].invocation().arguments(),
        args(&["$(touch marker) $HOME *.rs %PATH% !important! ^"])
    );

    let double_quoted = parse("echo \"%PATH% !important! ^\"");
    assert_eq!(
        double_quoted.operations()[0].invocation().arguments(),
        args(&["%PATH% !important! ^"])
    );
}
