//! Public parsing contract, rejection fixtures, and boundary checks.

use std::path::Path;

use policy_core::{Action, CommandAction, FileAction, GitAction, NetworkAction};
use policy_language::{
    Effect, MAX_INPUT_BYTES, MAX_NESTING_DEPTH, ParseErrorKind as K, ParsedPolicy,
    ResourceSelector, parse_policy,
};

fn parse(input: &str) -> ParsedPolicy {
    parse_policy(Path::new("fixtures/policy.yaml"), input).unwrap()
}

#[test]
fn parses_version_defaults_and_ordered_rules() {
    let policy = parse(include_str!("fixtures/valid/full.yaml"));
    assert_eq!(policy.source(), Path::new("fixtures/policy.yaml"));
    assert_eq!(policy.version(), 1);
    let defaults = policy.defaults().unwrap();
    assert_eq!(
        (
            defaults.file(),
            defaults.command(),
            defaults.git(),
            defaults.network()
        ),
        (
            Some(Effect::Deny),
            Some(Effect::Ask),
            Some(Effect::Allow),
            Some(Effect::Deny)
        )
    );
    assert_eq!(
        policy
            .rules()
            .iter()
            .map(|rule| rule.id())
            .collect::<Vec<_>>(),
        ["source", "dependencies", "git", "network"]
    );
    assert_eq!(
        policy.rules()[0].description(),
        Some("Edit café sources.\nPreserve Unicode.\n")
    );
    assert_eq!(policy.rules()[1].description(), None);
    assert_eq!(
        policy
            .rules()
            .iter()
            .map(|rule| rule.effect())
            .collect::<Vec<_>>(),
        [Effect::Allow, Effect::Ask, Effect::Deny, Effect::Deny]
    );
}

#[test]
fn represents_all_actions_and_resource_selectors() {
    let policy = parse(include_str!("fixtures/valid/full.yaml"));
    assert_eq!(
        policy.rules()[0].actions(),
        [
            Action::File(FileAction::Read),
            Action::File(FileAction::Write),
            Action::File(FileAction::Delete),
            Action::File(FileAction::Rename)
        ]
    );
    assert_eq!(
        policy.rules()[1].actions(),
        [Action::Command(CommandAction::Execute)]
    );
    assert_eq!(
        policy.rules()[2].actions(),
        [
            Action::Git(GitAction::Commit),
            Action::Git(GitAction::Checkout),
            Action::Git(GitAction::Reset),
            Action::Git(GitAction::Push),
            Action::Git(GitAction::ResetHard),
            Action::Git(GitAction::ForcePush)
        ]
    );
    assert_eq!(
        policy.rules()[3].actions(),
        [Action::Network(NetworkAction::Connect)]
    );
    assert_eq!(
        policy.rules()[0].resource(),
        &ResourceSelector::File {
            pattern: "src/**".into()
        }
    );
    assert_eq!(
        policy.rules()[1].resource(),
        &ResourceSelector::Command {
            executable: "npm".into(),
            arguments: Some(vec![
                "install".into(),
                "${PACKAGE}".into(),
                "$(touch /tmp/policy-parser-must-not-run)".into(),
                "{{ expression }}".into()
            ])
        }
    );
    assert_eq!(
        policy.rules()[2].resource(),
        &ResourceSelector::Git {
            repository: "../project/**".into()
        }
    );
    assert_eq!(
        policy.rules()[3].resource(),
        &ResourceSelector::Network {
            host: "EXAMPLE.COM".into(),
            port: Some(443)
        }
    );
}

#[test]
fn preserves_absence_without_inserting_defaults() {
    let minimal = parse(include_str!("fixtures/valid/minimal.yaml"));
    assert!(minimal.defaults().is_none());
    assert!(minimal.rules().is_empty());
    assert!(minimal.location("defaults").is_none());
    let empty = parse("version: 1\ndefaults: {}\nrules: []");
    assert!(empty.defaults().is_some());
    assert_eq!(empty.defaults().unwrap().file(), None);
    let partial = parse("version: 1\ndefaults: {file: ask}\nrules: []");
    assert_eq!(partial.defaults().unwrap().file(), Some(Effect::Ask));
    assert_eq!(partial.defaults().unwrap().command(), None);
    for selector in [
        "{kind: command, executable: sh}",
        "{kind: network, host: localhost}",
    ] {
        let policy = parse(&rule(selector));
        match policy.rules()[0].resource() {
            ResourceSelector::Command { arguments, .. } => assert_eq!(*arguments, None),
            ResourceSelector::Network { port, .. } => assert_eq!(*port, None),
            _ => panic!("unexpected selector"),
        }
    }
}

fn rule(selector: &str) -> String {
    format!(
        "version: 1\nrules:\n  - id: test\n    effect: allow\n    actions: []\n    resource: {selector}\n"
    )
}

#[test]
fn preserves_source_locations_for_future_semantic_validation() {
    let policy = parse(include_str!("fixtures/valid/full.yaml"));
    for (path, expected) in [
        ("version", (1, 10)),
        ("rules[0]", (8, 5)),
        ("rules[0].actions[1]", (13, 26)),
        ("rules[0].resource.pattern", (16, 16)),
        ("defaults.file", (3, 9)),
    ] {
        let loc = policy.location(path).unwrap();
        assert_eq!((loc.line(), loc.column()), expected, "{path}");
    }
    let line =
        "rules: [{id: 雪, effect: allow, actions: [], resource: {kind: file, pattern: café}}]";
    let unicode = parse(&format!("version: 1\r\n{line}\r\n"));
    let loc = unicode.location("rules[0].resource.pattern").unwrap();
    assert_eq!(loc.line(), 2);
    assert_eq!(
        loc.column(),
        line[..line.find("café").unwrap()].chars().count() + 1
    );
}

#[test]
fn rejects_tags_and_anchors_on_keys_scalars_and_collections() {
    for input in [
        "version: !!int 1\nrules: []",
        "! version: 1\nrules: []",
        "version: 1\nrules: ! []",
        "!!map {version: 1, rules: []}",
        "version: 1\nrules: []\ndefaults: !!map {}",
    ] {
        assert_eq!(
            parse_policy(Path::new("bad"), input).unwrap_err().kind(),
            K::ExplicitTag,
            "{input}"
        );
    }
    for input in [
        "version: &v 1\nrules: []",
        "&doc {version: 1, rules: []}",
        "version: 1\nrules: &r []",
        "version: 1\nrules: []\ndefaults: &d {}",
        "version: &v 1\nrules: [*v]",
    ] {
        assert_eq!(
            parse_policy(Path::new("bad"), input).unwrap_err().kind(),
            K::AnchorOrAlias,
            "{input}"
        );
    }
    let escaped_key = "version: 1\n\"versi\\u006fn\": 2\nrules: []";
    assert_eq!(
        parse_policy(Path::new("bad"), escaped_key)
            .unwrap_err()
            .kind(),
        K::DuplicateKey
    );
}

#[test]
fn leaves_semantic_validation_to_change_03() {
    let policy = parse(include_str!("fixtures/valid/unvalidated.yaml"));
    assert_eq!(policy.version(), 999);
    assert_eq!(policy.rules()[0].id(), policy.rules()[1].id());
    assert_eq!(
        policy.rules()[0].actions(),
        [Action::Network(NetworkAction::Connect)]
    );
    assert!(matches!(
        policy.rules()[0].resource(),
        ResourceSelector::File { .. }
    ));
    assert!(policy.rules()[1].actions().is_empty());
    assert!(matches!(
        policy.rules()[1].resource(),
        ResourceSelector::Network { port: Some(0), .. }
    ));
    assert_eq!(
        parse(&rule("{kind: file, pattern: ''}").replace("id: test", "id: ''")).rules()[0].id(),
        ""
    );
}

#[test]
fn rejects_structural_fixture_corpus_with_locations() {
    let cases = [
        (
            include_str!("fixtures/invalid/malformed.yaml"),
            K::InvalidYaml,
        ),
        (
            include_str!("fixtures/invalid/empty.yaml"),
            K::EmptyDocument,
        ),
        (
            include_str!("fixtures/invalid/multiple.yaml"),
            K::MultipleDocuments,
        ),
        (
            include_str!("fixtures/invalid/complex-key.yaml"),
            K::InvalidMappingKey,
        ),
        (
            include_str!("fixtures/invalid/unknown-field.yaml"),
            K::UnknownField,
        ),
        (
            include_str!("fixtures/invalid/missing-field.yaml"),
            K::MissingField,
        ),
        (
            include_str!("fixtures/invalid/wrong-shape.yaml"),
            K::InvalidType,
        ),
        (
            include_str!("fixtures/invalid/unknown-action.yaml"),
            K::UnknownAction,
        ),
        (
            include_str!("fixtures/invalid/unknown-effect.yaml"),
            K::UnknownEffect,
        ),
        (
            include_str!("fixtures/invalid/unknown-resource.yaml"),
            K::UnknownResourceKind,
        ),
        (include_str!("fixtures/invalid/tag.yaml"), K::ExplicitTag),
        (
            include_str!("fixtures/invalid/executable-tag.yaml"),
            K::ExplicitTag,
        ),
        (
            include_str!("fixtures/invalid/anchor.yaml"),
            K::AnchorOrAlias,
        ),
        (include_str!("fixtures/invalid/alias.yaml"), K::InvalidYaml),
        (include_str!("fixtures/invalid/merge.yaml"), K::MergeKey),
        (
            include_str!("fixtures/invalid/duplicate.yaml"),
            K::DuplicateKey,
        ),
        (
            include_str!("fixtures/invalid/nested-duplicate.yaml"),
            K::DuplicateKey,
        ),
        (include_str!("fixtures/invalid/null.yaml"), K::InvalidType),
    ];
    for (input, kind) in cases {
        let source = Path::new("invalid.yaml");
        let error = parse_policy(source, input).unwrap_err();
        assert_eq!(error.kind(), kind, "{input}");
        assert_eq!(error.source(), source);
        assert!(error.location().line() >= 1);
        assert!(error.location().column() >= 1);
        assert!(error.field_path().is_some());
        assert_eq!(parse_policy(source, input).unwrap_err(), error);
    }
}

#[test]
fn diagnoses_exact_fields_and_duplicate_origins() {
    for (input, path, line, column, first) in [
        (
            include_str!("fixtures/invalid/duplicate.yaml"),
            "version",
            2,
            1,
            Some((1, 1)),
        ),
        (
            include_str!("fixtures/invalid/nested-duplicate.yaml"),
            "rules[0].resource.pattern",
            9,
            7,
            Some((8, 7)),
        ),
        (
            include_str!("fixtures/invalid/unknown-field.yaml"),
            "extra",
            3,
            1,
            None,
        ),
        (
            include_str!("fixtures/invalid/missing-field.yaml"),
            "rules",
            1,
            1,
            None,
        ),
        (
            include_str!("fixtures/invalid/wrong-shape.yaml"),
            "rules",
            2,
            8,
            None,
        ),
        (
            include_str!("fixtures/invalid/unknown-action.yaml"),
            "rules[0].actions[0]",
            5,
            15,
            None,
        ),
        (
            include_str!("fixtures/invalid/malformed.yaml"),
            "rules[0]",
            3,
            1,
            None,
        ),
    ] {
        let error = parse_policy(Path::new("bad.yaml"), input).unwrap_err();
        assert_eq!(error.field_path(), Some(path));
        assert_eq!(
            (error.location().line(), error.location().column()),
            (line, column),
            "{input}"
        );
        assert_eq!(
            error
                .original_location()
                .map(|loc| (loc.line(), loc.column())),
            first
        );
    }
}

#[test]
fn rejects_unknown_and_duplicate_keys_at_each_mapping_level() {
    for input in [
        "version: 1\nrules: []\ndefaults: {files: deny}".to_owned(),
        rule("{kind: file, pattern: '**'}").replace("id: test", "id: test\n    unexpected: true"),
        rule("{kind: file, pattern: '**', host: example.com}"),
        rule("{kind: command, executable: sh, pattern: '**'}"),
        rule("{kind: git, repository: '.', arguments: []}"),
        rule("{kind: network, host: example.com, executable: sh}"),
    ] {
        assert_eq!(
            parse_policy(Path::new("bad"), &input).unwrap_err().kind(),
            K::UnknownField
        );
    }
    for input in [
        "version: 1\nrules: []\ndefaults: {file: deny, file: allow}".to_owned(),
        rule("{kind: file, pattern: '**'}").replace("id: test", "id: test\n    id: another"),
    ] {
        assert_eq!(
            parse_policy(Path::new("bad"), &input).unwrap_err().kind(),
            K::DuplicateKey
        );
    }
}

#[test]
fn rejects_missing_required_fields_and_invalid_scalar_shapes() {
    for input in [
        "rules: []".to_owned(),
        "version: 1".into(),
        rule("{kind: file}"),
        rule("{kind: command}"),
        rule("{kind: git}"),
        rule("{kind: network}"),
        rule("{}"),
        rule("{kind: file, pattern: '**'}").replace("    effect: allow\n", ""),
        rule("{kind: file, pattern: '**'}").replace("    actions: []\n", ""),
        "version: 1\nrules: [{}]".into(),
    ] {
        assert_eq!(
            parse_policy(Path::new("bad"), &input).unwrap_err().kind(),
            K::MissingField,
            "{input}"
        );
    }
    for input in [
        "version: -1\nrules: []".to_owned(),
        "version: 1.5\nrules: []".into(),
        "version: 0x01\nrules: []".into(),
        "version: 18446744073709551616\nrules: []".into(),
        "version: 1\nrules: [null]".into(),
        "version: 1\nrules: []\ndefaults: {file: ~}".into(),
        rule("{kind: network, host: localhost, port: 65536}"),
        rule("{kind: network, host: localhost, port: null}"),
        rule("{kind: command, executable: sh, arguments: null}"),
        rule("{kind: command, executable: sh, arguments: [null]}"),
        rule("{kind: file, pattern: '**'}").replace("id: test", "id: []"),
        rule("{kind: file, pattern: '**'}").replace("id: test", "id: test\n    description: null"),
    ] {
        assert_eq!(
            parse_policy(Path::new("bad"), &input).unwrap_err().kind(),
            K::InvalidType,
            "{input}"
        );
    }
}

#[test]
fn textual_scalars_are_not_interpreted_or_normalized() {
    for text in [
        "true",
        "001",
        "2026-09-24",
        "../OUTSIDE/**",
        "${HOME}",
        "$(echo danger)",
        "{{eval()}}",
        "café/雪",
    ] {
        let policy = parse(&rule(&format!("{{kind: file, pattern: '{text}'}}")));
        assert_eq!(
            policy.rules()[0].resource(),
            &ResourceSelector::File {
                pattern: text.into()
            }
        );
    }
    let policy = parse(&rule(
        "{kind: command, executable: true, arguments: [001, false, 'null', ''] }",
    ));
    assert_eq!(
        policy.rules()[0].resource(),
        &ResourceSelector::Command {
            executable: "true".into(),
            arguments: Some(vec!["001".into(), "false".into(), "null".into(), "".into()])
        }
    );
    let escaped = parse(&rule("{kind: file, pattern: \"a\\nb\"}"));
    assert_eq!(
        escaped.rules()[0].resource(),
        &ResourceSelector::File {
            pattern: "a\nb".into()
        }
    );
}

#[test]
fn enforces_input_byte_limit_at_the_boundary() {
    let mut input = "version: 1\nrules: []\n#".to_owned();
    input.push_str(&"x".repeat(MAX_INPUT_BYTES - input.len()));
    assert!(parse_policy(Path::new("limit"), &input).is_ok());
    input.push('x');
    assert_eq!(
        parse_policy(Path::new("limit"), &input).unwrap_err().kind(),
        K::InputTooLarge
    );
    let input = "雪".repeat(MAX_INPUT_BYTES / 3 + 1);
    assert_eq!(
        parse_policy(Path::new("limit"), &input).unwrap_err().kind(),
        K::InputTooLarge
    );
}

#[test]
fn enforces_collection_depth_before_schema_decoding() {
    for (depth, kind) in [
        (MAX_NESTING_DEPTH, K::InvalidType),
        (MAX_NESTING_DEPTH + 1, K::NestingTooDeep),
        (10_000, K::NestingTooDeep),
    ] {
        let input = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        assert_eq!(
            parse_policy(Path::new("depth"), &input).unwrap_err().kind(),
            kind,
            "depth {depth}"
        );
    }
}

#[test]
fn diagnostics_are_standard_errors_without_scalar_payloads() {
    let input =
        rule("{kind: file, pattern: '**'}").replace("effect: allow", "effect: secret-token");
    let error = parse_policy(Path::new("policy.yaml"), &input).unwrap_err();
    let standard: &dyn std::error::Error = &error;
    assert!(standard.source().is_none());
    assert!(!standard.to_string().contains("secret-token"));
    assert!(
        standard
            .to_string()
            .contains("policy.yaml:4:13: UnknownEffect at rules[0].effect")
    );
}
