//! Semantic contracts through the public API; structural parsing remains separate.

use policy_core::{Action, FileAction};
use policy_language::{
    Effect, NetworkSelector, ParseErrorKind, ParsedPolicy, PathPattern, PathSegment, PathToken,
    ValidatedPolicy, ValidatedResourceSelector as Selector, ValidationError,
    ValidationErrorKind as K, parse_policy, validate_policy,
};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
};

fn parsed(input: &str) -> ParsedPolicy {
    parse_policy(Path::new("fixtures/semantic.yaml"), input).unwrap()
}

fn validate(input: &str) -> Result<ValidatedPolicy, ValidationError> {
    validate_policy(parsed(input))
}

fn quoted(value: &str) -> String {
    // JSON-style escapes shared by YAML's double-quoted scalar syntax.
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn rule(action: &str, selector: &str) -> String {
    format!(
        "version: 1\nrules:\n  - id: test\n    effect: allow\n    actions: [{action}]\n    resource: {selector}\n"
    )
}

fn path_rule(family: &str, pattern: &str) -> String {
    let (field, action) = if family == "file" {
        ("pattern", "file.write")
    } else {
        ("repository", "git.commit")
    };
    rule(
        action,
        &format!("{{kind: {family}, {field}: {}}}", quoted(pattern)),
    )
}

fn path(pattern: &str) -> PathPattern {
    match validate(&path_rule("file", pattern)).unwrap().rules()[0].resource() {
        Selector::File(pattern) => pattern.clone(),
        _ => panic!("expected file pattern"),
    }
}

fn network(host: &str, port: Option<u16>) -> Result<NetworkSelector, ValidationError> {
    let port = port.map(|p| format!(", port: {p}")).unwrap_or_default();
    let policy = validate(&rule(
        "network.connect",
        &format!("{{kind: network, host: {}{port}}}", quoted(host)),
    ))?;
    match policy.rules()[0].resource() {
        Selector::Network(value) => Ok(value.clone()),
        _ => panic!("expected network selector"),
    }
}

#[test]
fn rejects_unsupported_versions_before_rule_validation() {
    assert_eq!(validate("version: 1\nrules: []").unwrap().version(), 1);
    for version in [0, 2, 999, u64::MAX] {
        let input =
            path_rule("file", "../bad").replace("version: 1", &format!("version: {version}"));
        let error = validate(&input).unwrap_err();
        assert_eq!(error.kind(), K::UnsupportedVersion);
        assert_eq!(error.field_path(), "version");
        assert_eq!(
            (error.location().line(), error.location().column()),
            (1, 10)
        );
    }
}

#[test]
fn validates_all_action_resource_pairs() {
    let selectors = [
        "{kind: file, pattern: 'src/**'}",
        "{kind: command, executable: cargo}",
        "{kind: git, repository: '.'}",
        "{kind: network, host: localhost}",
    ];
    let actions = [
        (0, "file.read"),
        (0, "file.write"),
        (0, "file.delete"),
        (0, "file.rename"),
        (1, "command.execute"),
        (2, "git.commit"),
        (2, "git.checkout"),
        (2, "git.reset"),
        (2, "git.push"),
        (2, "git.reset-hard"),
        (2, "git.force-push"),
        (3, "network.connect"),
    ];
    for (family, action) in actions {
        for (index, selector) in selectors.iter().enumerate() {
            let result = validate(&rule(action, selector));
            if index == family {
                let policy = result.unwrap();
                let rule = &policy.rules()[0];
                assert_eq!(
                    rule.actions()[0].resource_kind(),
                    rule.resource().resource_kind()
                );
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind(), K::IncompatibleAction, "{action} / {selector}");
                assert_eq!(error.field_path(), "rules[0].actions[0]");
            }
        }
    }
    let error = validate(&rule("file.read, network.connect", selectors[0])).unwrap_err();
    assert_eq!(error.field_path(), "rules[0].actions[1]");
    assert_eq!(
        validate(&rule("", selectors[0])).unwrap_err().kind(),
        K::EmptyActions
    );
}

#[test]
fn validates_ids_without_trimming_or_case_folding() {
    for id in ["", " ", "\t", "\u{2003}", "id\n", "a\0b", "a\u{7f}b"] {
        let input = path_rule("file", "src/**").replace("id: test", &format!("id: {}", quoted(id)));
        let error = validate(&input).unwrap_err();
        assert_eq!(error.kind(), K::InvalidRuleId, "{id:?}");
        assert_eq!(error.field_path(), "rules[0].id");
    }
    let mut input = "version: 1\nrules:\n".to_owned();
    for id in ["id", "ID", " id ", "雪"] {
        input.push_str(&format!("  - {{id: {}, effect: deny, actions: [file.read], resource: {{kind: file, pattern: '.'}}}}\n", quoted(id)));
    }
    let policy = validate(&input).unwrap();
    assert_eq!(
        policy.rules().iter().map(|r| r.id()).collect::<Vec<_>>(),
        ["id", "ID", " id ", "雪"]
    );
    input.push_str(
        "  - {id: id, effect: ask, actions: [file.read], resource: {kind: file, pattern: '.'}}\n",
    );
    let error = validate(&input).unwrap_err();
    assert_eq!(error.kind(), K::DuplicateRuleId);
    assert_eq!(error.field_path(), "rules[4].id");
    assert_eq!(
        error.original_location(),
        parsed(&input).location("rules[0].id")
    );
    // The uniqueness boundary is a document, not all policies in a process.
    assert!(validate(&path_rule("file", "src/**")).is_ok());
    assert!(validate(&path_rule("file", "src/**")).is_ok());
}

#[test]
fn preserves_rules_effects_defaults_and_source_locations() {
    let input = include_str!("fixtures/semantic/full.yaml");
    let original = parsed(input);
    let policy = validate_policy(original.clone()).unwrap();
    assert_eq!(policy.source(), original.source());
    assert_eq!(policy.defaults(), original.defaults());
    assert_eq!(policy.defaults().unwrap().file(), Some(Effect::Deny));
    assert_eq!(policy.defaults().unwrap().command(), Some(Effect::Ask));
    assert_eq!(policy.defaults().unwrap().git(), Some(Effect::Allow));
    assert_eq!(policy.defaults().unwrap().network(), None);
    for (index, (before, after)) in original.rules().iter().zip(policy.rules()).enumerate() {
        assert_eq!(before.id(), after.id());
        assert_eq!(before.description(), after.description());
        assert_eq!(before.effect(), after.effect());
        assert_eq!(before.actions(), after.actions());
        for field in [
            "",
            ".id",
            ".description",
            ".effect",
            ".actions",
            ".actions[0]",
            ".resource",
        ] {
            let field = format!("rules[{index}]{field}");
            assert_eq!(policy.location(&field), original.location(&field));
        }
    }
    assert_eq!(
        policy.rules()[0].actions(),
        [Action::File(FileAction::Write); 2]
    );
    for defaults in ["", "defaults: {}\n", "defaults: {network: ask}\n"] {
        let input = format!("version: 1\n{defaults}rules: []");
        let original = parsed(&input);
        let policy = validate_policy(original.clone()).unwrap();
        assert_eq!(policy.defaults(), original.defaults());
        assert!(policy.rules().is_empty());
    }
}

#[test]
fn accepts_shared_path_grammar_and_preserves_tokens() {
    use PathSegment::{Recursive, Tokens};
    use PathToken::{AnyCharacter, AnyCharacters, Literal};
    assert!(path(".").is_root());
    assert_eq!(path(".").original(), ".");
    let pattern = path("Src/**/.café?雪*.rs");
    assert_eq!(pattern.original(), "Src/**/.café?雪*.rs");
    assert_eq!(
        pattern.segments(),
        [
            Tokens(vec![Literal("Src".into())]),
            Recursive,
            Tokens(vec![
                Literal(".café".into()),
                AnyCharacter,
                Literal("雪".into()),
                AnyCharacters,
                Literal(".rs".into())
            ]),
        ]
    );
    for input in [
        ".",
        "**",
        "*",
        "?",
        "a/**/b",
        "**/**",
        ".git/**",
        "two words/雪",
        "a!b",
        "a/!b",
        "file.",
        "...",
    ] {
        let file = validate(&path_rule("file", input)).unwrap();
        let git = validate(&path_rule("git", input)).unwrap();
        let (Selector::File(file), Selector::Git(git)) =
            (file.rules()[0].resource(), git.rules()[0].resource())
        else {
            panic!("wrong family")
        };
        assert_eq!(file, git);
    }
    assert_ne!(path("Src/**"), path("src/**"));
}

#[test]
fn rejects_unsupported_path_constructs_in_both_families() {
    for pattern in [
        "",
        "/",
        "/src/**",
        "//server/share",
        "C:/src",
        "c:src",
        "C:",
        "\\\\server\\share",
        "a\\b",
        "a\\*",
        "..",
        "../a",
        "a/../b",
        "a/..",
        "./a",
        "a/./b",
        "a/.",
        "a//b",
        "a/",
        "a/**/",
        "***",
        "a**",
        "**b",
        "a/**b/c",
        "[ab]",
        "a]",
        "{a,b}",
        "a}",
        "!src/**",
        "!",
        "a\nb",
        "a\0b",
        "a\u{7f}b",
    ] {
        for family in ["file", "git"] {
            let error = validate(&path_rule(family, pattern)).unwrap_err();
            assert_eq!(error.kind(), K::InvalidPathPattern, "{family}: {pattern:?}");
            let field = if family == "file" {
                "pattern"
            } else {
                "repository"
            };
            assert_eq!(error.field_path(), format!("rules[0].resource.{field}"));
        }
    }
}

#[test]
fn command_values_remain_literal_and_absence_is_distinct() {
    for arguments in [
        None,
        Some(vec![]),
        Some(vec![
            "",
            "*",
            "${HOME}",
            "$(touch x)",
            "a;b",
            "--flag",
            "雪\ntext",
        ]),
    ] {
        let field = arguments
            .as_ref()
            .map(|args| {
                format!(
                    ", arguments: [{}]",
                    args.iter()
                        .map(|s| quoted(s))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .unwrap_or_default();
        let input = rule(
            "command.execute",
            &format!("{{kind: command, executable: ' tools/My Program * '{field}}}"),
        );
        let policy = validate(&input).unwrap();
        let Selector::Command(command) = policy.rules()[0].resource() else {
            panic!("wrong selector")
        };
        assert_eq!(command.executable(), " tools/My Program * ");
        assert_eq!(
            command
                .arguments()
                .map(|args| args.iter().map(String::as_str).collect::<Vec<_>>()),
            arguments
        );
    }
    for executable in ["", "  ", "\t\n", "\u{2003}", "exe\0name"] {
        let input = rule(
            "command.execute",
            &format!("{{kind: command, executable: {}}}", quoted(executable)),
        );
        assert_eq!(validate(&input).unwrap_err().kind(), K::InvalidExecutable);
    }
    let input = rule(
        "command.execute",
        "{kind: command, executable: sh, arguments: [ok, \"bad\\0arg\"]}",
    );
    let error = validate(&input).unwrap_err();
    assert_eq!(error.kind(), K::InvalidArgument);
    assert_eq!(error.field_path(), "rules[0].resource.arguments[1]");
}

#[test]
fn accepts_canonical_dns_and_ip_identities_and_port_boundaries() {
    let name = network("EXAMPLE.Com.", None).unwrap();
    assert_eq!(name.original_host(), "EXAMPLE.Com.");
    assert_eq!(name.host().dns_name(), Some("example.com"));
    assert_eq!(name.host().ip_addr(), None);
    assert_eq!(name.host(), network("example.com", None).unwrap().host());
    assert_eq!(name.port(), None);
    for name in [
        "localhost",
        "a",
        "xn--caf-dma.example",
        "a-b.c9",
        "123.example",
        "a.123",
    ] {
        assert!(network(name, None).is_ok(), "{name}");
    }
    for port in [1, 443, u16::MAX] {
        assert_eq!(
            network("example.com", Some(port))
                .unwrap()
                .port()
                .unwrap()
                .get(),
            port
        );
    }
    let v4 = network("127.0.0.1", None).unwrap();
    assert_eq!(v4.host().ip_addr(), Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    assert_eq!(v4.host().dns_name(), None);
    let v6 = network("::1", None).unwrap();
    assert_eq!(v6.host().ip_addr(), Some(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    assert_eq!(v6.host(), network("0:0:0:0:0:0:0:1", None).unwrap().host());
    assert_eq!(
        network("2001:DB8::A", None).unwrap().host(),
        network("2001:db8::a", None).unwrap().host()
    );
    assert_ne!(v4.host(), v6.host());
    let label = "a".repeat(63);
    assert!(network(&label, None).is_ok());
    let max_name = format!("{label}.{label}.{label}.{}", "a".repeat(61));
    assert_eq!(max_name.len(), 253);
    assert!(network(&max_name, None).is_ok());
    assert!(network(&format!("{max_name}."), None).is_ok());
    assert_eq!(
        network(&format!("{max_name}a"), None).unwrap_err().kind(),
        K::InvalidHost
    );
}

#[test]
fn rejects_unsupported_network_constructs() {
    for host in [
        "",
        ".",
        " ",
        "example.com..",
        ".example.com",
        "a..b",
        "-a.com",
        "a-.com",
        "a_b.com",
        "café.com",
        "*.example.com",
        "example.*",
        "https://example.com",
        "example.com:443",
        "127.0.0.1:80",
        "[::1]",
        "[::1]:80",
        "::gg",
        "::1%lo0",
        "192.0.2.0/24",
        "::/0",
        "user@example.com",
        "a/b",
        "a\\b",
        "a\nb",
        "a\0b",
        "256.0.0.1",
        "127.1",
        "01.2.3.4",
        "127.0.0.1.",
        "123",
    ] {
        let error = network(host, None).unwrap_err();
        assert_eq!(error.kind(), K::InvalidHost, "{host:?}");
        assert_eq!(error.field_path(), "rules[0].resource.host");
    }
    assert_eq!(
        network(&"a".repeat(64), None).unwrap_err().kind(),
        K::InvalidHost
    );
    let error = network("example.com", Some(0)).unwrap_err();
    assert_eq!(error.kind(), K::InvalidPort);
    assert_eq!(error.field_path(), "rules[0].resource.port");
}

#[test]
fn semantic_diagnostics_preserve_unicode_crlf_and_duplicate_origins() {
    let input = "version: 1\r\nrules:\r\n  - {id: 雪, effect: allow, actions: [file.read], resource: {kind: file, pattern: '../bad'}}\r\n";
    let error = validate(input).unwrap_err();
    let line = input.lines().nth(2).unwrap();
    assert_eq!(error.location().line(), 3);
    assert_eq!(
        error.location().column(),
        line[..line.find("'../bad'").unwrap()].chars().count() + 1
    );
    assert_eq!(error.source(), Path::new("fixtures/semantic.yaml"));
    assert_eq!(error.original_location(), None);
    let standard: &dyn std::error::Error = &error;
    assert!(standard.source().is_none());
    assert!(!standard.to_string().contains("../bad"));
    assert!(
        standard
            .to_string()
            .contains("InvalidPathPattern at rules[0].resource.pattern")
    );
    let row = "  - {id: 雪, effect: allow, actions: [file.read], resource: {kind: file, pattern: '.'}}\r\n";
    let input = format!("version: 1\r\nrules:\r\n{row}{row}");
    let error = validate(&input).unwrap_err();
    assert_eq!(
        (error.location().line(), error.location().column()),
        (4, 10)
    );
    let first = error.original_location().unwrap();
    assert_eq!((first.line(), first.column()), (3, 10));
    assert!(error.to_string().contains("first occurrence at 3:10"));
}

#[test]
fn semantic_error_precedence_is_deterministic() {
    let original = path_rule("file", "../bad");
    let cases = [
        (
            original
                .replace("id: test", "id: ''")
                .replace("[file.write]", "[]"),
            K::InvalidRuleId,
        ),
        (original.replace("[file.write]", "[]"), K::EmptyActions),
        (
            original.replace("file.write", "git.push"),
            K::IncompatibleAction,
        ),
        (original, K::InvalidPathPattern),
        (
            rule("network.connect", "{kind: network, host: '*.bad', port: 0}"),
            K::InvalidHost,
        ),
        (
            rule(
                "command.execute",
                "{kind: command, executable: '', arguments: [\"\\0\"]}",
            ),
            K::InvalidExecutable,
        ),
    ];
    for (input, kind) in cases {
        let first = validate(&input).unwrap_err();
        assert_eq!(first.kind(), kind);
        assert_eq!(validate(&input).unwrap_err(), first);
    }
    // Rules are checked in document order, not by error category globally.
    let input = "version: 1\nrules:\n - {id: first, effect: ask, actions: [file.read], resource: {kind: file, pattern: '../bad'}}\n - {id: '', effect: deny, actions: [], resource: {kind: file, pattern: '.'}}";
    assert_eq!(validate(input).unwrap_err().kind(), K::InvalidPathPattern);
    let input = "version: 1\nrules:\n - {id: same, effect: ask, actions: [file.read], resource: {kind: file, pattern: '.'}}\n - {id: same, effect: deny, actions: [], resource: {kind: file, pattern: '../bad'}}";
    assert_eq!(validate(input).unwrap_err().kind(), K::DuplicateRuleId);
}

#[test]
fn unknown_effects_remain_structural_and_ask_is_preserved() {
    let input = path_rule("file", "src/**");
    for effect in ["approve", "ALLOW", "secret-token"] {
        assert_eq!(
            parse_policy(
                Path::new("bad"),
                &input.replace("effect: allow", &format!("effect: {effect}"))
            )
            .unwrap_err()
            .kind(),
            ParseErrorKind::UnknownEffect
        );
    }
    for (name, expected) in [
        ("allow", Effect::Allow),
        ("deny", Effect::Deny),
        ("ask", Effect::Ask),
    ] {
        let policy = validate(&input.replace("effect: allow", &format!("effect: {name}"))).unwrap();
        assert_eq!(policy.rules()[0].effect(), expected);
    }
}
