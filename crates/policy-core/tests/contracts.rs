//! Public contract tests for agent-neutral authorization values.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use policy_core::*;

fn root() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"C:\agent-policy-test-root")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/agent-policy-test-root")
    }
}

fn context() -> Context {
    Context::new("repo", root(), root().join("src"), None, None).unwrap()
}

fn principal() -> Principal {
    Principal::new(AgentKind::Codex, "session-1").unwrap()
}

fn request(action: Action, resource: Resource) -> Result<AuthorizationRequest, ValidationError> {
    AuthorizationRequest::new(principal(), action, resource, context())
}

fn file(destination: Option<PathBuf>) -> Resource {
    Resource::File(FileResource::new("src/../tests/case.rs", destination).unwrap())
}

fn source(layer: PolicyLayer) -> PolicySource {
    let path = match layer {
        PolicyLayer::Repository => "agent-policy.yaml",
        PolicyLayer::Task => ".agent/task-policy.yaml",
    };
    PolicySource::new(layer, path).unwrap()
}

#[test]
fn principal_preserves_agent_and_session() {
    for agent in [AgentKind::Codex, AgentKind::Claude] {
        let value = Principal::new(agent, " session-42 ").unwrap();
        assert_eq!(value.kind(), agent);
        assert_eq!(value.session_id(), " session-42 ");
    }
}

#[test]
fn file_actions_enforce_rename_shape() {
    for action in [FileAction::Read, FileAction::Write, FileAction::Delete] {
        let req = request(Action::File(action), file(None)).unwrap();
        assert_eq!(req.action(), Action::File(action));
        let Resource::File(resource) = req.resource() else {
            panic!("wrong resource")
        };
        assert_eq!(resource.path().as_os_str(), "src/../tests/case.rs");
        assert_eq!(resource.destination(), None);
        assert_eq!(
            request(Action::File(action), file(Some("new.rs".into()))),
            Err(ValidationError::UnexpectedRenameDestination)
        );
    }
    assert_eq!(
        request(Action::File(FileAction::Rename), file(None)),
        Err(ValidationError::MissingRenameDestination)
    );
    let req = request(
        Action::File(FileAction::Rename),
        file(Some("../outside/new.rs".into())),
    )
    .unwrap();
    let Resource::File(resource) = req.resource() else {
        panic!("wrong resource")
    };
    assert_eq!(
        resource.destination().unwrap().as_os_str(),
        "../outside/new.rs"
    );
}

#[test]
fn command_preserves_executable_arguments_and_context() {
    let args = vec![
        OsString::from(""),
        OsString::from("two words"),
        OsString::from("$(do-not-run)"),
        OsString::from(";"),
    ];
    let resource = CommandResource::new(" ../bin/tool ", args.clone()).unwrap();
    let req = request(
        Action::Command(CommandAction::Execute),
        Resource::Command(resource),
    )
    .unwrap();
    let Resource::Command(command) = req.resource() else {
        panic!("wrong resource")
    };
    assert_eq!(command.executable(), " ../bin/tool ");
    assert_eq!(command.arguments(), args);
    assert_eq!(req.context().working_directory(), root().join("src"));
    assert_eq!(req.principal(), &principal());
}

#[test]
fn git_actions_preserve_repository_identity() {
    for action in [
        GitAction::Commit,
        GitAction::Checkout,
        GitAction::Reset,
        GitAction::Push,
        GitAction::ResetHard,
        GitAction::ForcePush,
    ] {
        let resource = GitResource::new("../repo/.git/..").unwrap();
        let req = request(Action::Git(action), Resource::Git(resource)).unwrap();
        assert_eq!(req.action(), Action::Git(action));
        let Resource::Git(repo) = req.resource() else {
            panic!("wrong resource")
        };
        assert_eq!(repo.repository().as_os_str(), "../repo/.git/..");
    }
    assert_ne!(
        Action::Git(GitAction::Reset),
        Action::Git(GitAction::ResetHard)
    );
    assert_ne!(
        Action::Git(GitAction::Push),
        Action::Git(GitAction::ForcePush)
    );
}

#[test]
fn network_preserves_destination_and_port() {
    for port in [None, Some(1), Some(443), Some(u16::MAX)] {
        let resource = NetworkResource::new(" Host.Example ", port).unwrap();
        let req = request(
            Action::Network(NetworkAction::Connect),
            Resource::Network(resource),
        )
        .unwrap();
        let Resource::Network(network) = req.resource() else {
            panic!("wrong resource")
        };
        assert_eq!(network.host(), " Host.Example ");
        assert_eq!(network.port(), port);
    }
    assert_eq!(
        NetworkResource::new("example.test", Some(0)),
        Err(ValidationError::ZeroPort)
    );
}

#[test]
fn every_action_rejects_every_incompatible_resource_family() {
    let actions = [
        Action::File(FileAction::Read),
        Action::File(FileAction::Write),
        Action::File(FileAction::Delete),
        Action::File(FileAction::Rename),
        Action::Command(CommandAction::Execute),
        Action::Git(GitAction::Commit),
        Action::Git(GitAction::Checkout),
        Action::Git(GitAction::Reset),
        Action::Git(GitAction::Push),
        Action::Git(GitAction::ResetHard),
        Action::Git(GitAction::ForcePush),
        Action::Network(NetworkAction::Connect),
    ];
    for action in actions {
        let resources = [
            file(if action == Action::File(FileAction::Rename) {
                Some("new.rs".into())
            } else {
                None
            }),
            Resource::Command(CommandResource::new("tool", vec![]).unwrap()),
            Resource::Git(GitResource::new("repo").unwrap()),
            Resource::Network(NetworkResource::new("host", None).unwrap()),
        ];
        for resource in resources {
            let family = resource.kind();
            let result = request(action, resource);
            if action.resource_kind() == family {
                assert!(result.is_ok(), "{action:?} / {family:?}: {result:?}");
            } else {
                assert_eq!(
                    result,
                    Err(ValidationError::IncompatibleActionResource {
                        action,
                        resource: family
                    })
                );
            }
        }
    }
}

#[test]
fn context_preserves_explicit_and_absent_values() {
    let supplied_root = root().join("./repository/..");
    let cwd = root().join("../outside/./work");
    let value = Context::new(
        " repo ",
        supplied_root.clone(),
        cwd.clone(),
        Some(" feature/x ".into()),
        Some(" task ".into()),
    )
    .unwrap();
    assert_eq!(value.repository(), " repo ");
    assert_eq!(
        value.repository_root().as_os_str(),
        supplied_root.as_os_str()
    );
    assert_eq!(value.working_directory().as_os_str(), cwd.as_os_str());
    assert_eq!(value.branch(), Some(" feature/x "));
    assert_eq!(value.task(), Some(" task "));
    let req = AuthorizationRequest::new(
        principal(),
        Action::File(FileAction::Read),
        file(None),
        value.clone(),
    )
    .unwrap();
    assert_eq!(req.context(), &value);
    assert_eq!(context().branch(), None);
    assert_eq!(context().task(), None);
}

#[test]
fn blank_identifiers_are_rejected_without_fabrication() {
    for blank in ["", " ", "\t\n", "\u{2003}"] {
        assert_eq!(
            Principal::new(AgentKind::Codex, blank),
            Err(ValidationError::Blank {
                field: "session_id"
            })
        );
        assert_eq!(
            Context::new(blank, root(), root(), None, None),
            Err(ValidationError::Blank {
                field: "repository"
            })
        );
        assert_eq!(
            Context::new("repo", root(), root(), Some(blank.into()), None),
            Err(ValidationError::Blank { field: "branch" })
        );
        assert_eq!(
            Context::new("repo", root(), root(), None, Some(blank.into())),
            Err(ValidationError::Blank { field: "task" })
        );
        assert_eq!(
            NetworkResource::new(blank, None),
            Err(ValidationError::Blank {
                field: "network_host"
            })
        );
        assert_eq!(
            RuleReference::new(source(PolicyLayer::Repository), blank),
            Err(ValidationError::Blank { field: "rule_id" })
        );
        assert_eq!(
            AuthorizationError::new(AuthorizationErrorKind::EvaluationFailed, blank),
            Err(ValidationError::Blank {
                field: "diagnostic"
            })
        );
    }
}

#[test]
fn context_requires_nonempty_absolute_paths() {
    for (path, expected) in [
        (
            PathBuf::new(),
            ValidationError::EmptyPath {
                field: "repository_root",
            },
        ),
        (
            PathBuf::from("relative"),
            ValidationError::PathNotAbsolute {
                field: "repository_root",
            },
        ),
    ] {
        assert_eq!(
            Context::new("repo", path, root(), None, None),
            Err(expected)
        );
    }
    assert_eq!(
        Context::new("repo", root(), "", None, None),
        Err(ValidationError::EmptyPath {
            field: "working_directory"
        })
    );
    assert_eq!(
        Context::new("repo", root(), "relative", None, None),
        Err(ValidationError::PathNotAbsolute {
            field: "working_directory"
        })
    );
}

#[test]
fn empty_resources_and_policy_sources_are_rejected() {
    assert_eq!(
        FileResource::new("", None),
        Err(ValidationError::EmptyPath { field: "file_path" })
    );
    assert_eq!(
        FileResource::new("source", Some(PathBuf::new())),
        Err(ValidationError::EmptyPath {
            field: "rename_destination"
        })
    );
    assert_eq!(
        GitResource::new(""),
        Err(ValidationError::EmptyPath {
            field: "git_repository"
        })
    );
    assert_eq!(
        CommandResource::new("", vec![]),
        Err(ValidationError::EmptyExecutable)
    );
    assert_eq!(
        PolicySource::new(PolicyLayer::Repository, ""),
        Err(ValidationError::EmptyPath {
            field: "policy_source"
        })
    );
    // Whitespace-only filesystem names and executables are valid OS values.
    assert_eq!(FileResource::new(" ", None).unwrap().path(), Path::new(" "));
    assert_eq!(CommandResource::new(" ", vec![]).unwrap().executable(), " ");
}

#[test]
fn decisions_preserve_outcomes_and_provenance() {
    let repo_rule = RuleReference::new(source(PolicyLayer::Repository), " same-id ").unwrap();
    let task_rule = RuleReference::new(source(PolicyLayer::Task), " same-id ").unwrap();
    assert_ne!(repo_rule, task_rule);
    assert_eq!(repo_rule.rule_id(), " same-id ");
    assert_eq!(repo_rule.source().layer(), PolicyLayer::Repository);
    assert_eq!(repo_rule.source().path(), Path::new("agent-policy.yaml"));
    assert_eq!(task_rule.source().layer(), PolicyLayer::Task);
    assert_eq!(
        task_rule.source().path(),
        Path::new(".agent/task-policy.yaml")
    );
    let explicit_reasons = vec![
        DecisionReason::MatchedRule(repo_rule),
        DecisionReason::MatchedRule(task_rule),
    ];
    for kind in [
        DecisionKind::Allow,
        DecisionKind::Deny,
        DecisionKind::ApprovalRequired,
    ] {
        let decision = PolicyDecision::new(kind, explicit_reasons.clone()).unwrap();
        assert_eq!(decision.kind(), kind);
        assert_eq!(decision.reasons(), explicit_reasons);
        assert_eq!(
            PolicyDecision::new(kind, vec![]),
            Err(ValidationError::MissingExplanation)
        );
    }
    let default_reason = DecisionReason::ActionDefault {
        source: source(PolicyLayer::Repository),
        action: Action::File(FileAction::Read),
    };
    let decision = PolicyDecision::new(DecisionKind::Allow, vec![default_reason.clone()]).unwrap();
    assert_eq!(decision.reasons(), &[default_reason]);
    let denied = PolicyDecision::new(
        DecisionKind::Deny,
        vec![DecisionReason::NoApplicablePermission],
    )
    .unwrap();
    assert_eq!(denied.reasons(), &[DecisionReason::NoApplicablePermission]);
}

#[derive(Clone)]
struct FixedService {
    outcome: Result<PolicyDecision, AuthorizationError>,
}

impl PolicyService for FixedService {
    fn authorize(
        &self,
        request: &AuthorizationRequest,
    ) -> Result<PolicyDecision, AuthorizationError> {
        assert_eq!(request.principal().session_id(), "session-1");
        self.outcome.clone()
    }
}

#[test]
fn service_port_is_object_safe_and_preserves_decisions_and_errors() {
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn PolicyService>();
    let req = request(Action::File(FileAction::Read), file(None)).unwrap();
    for kind in [
        DecisionKind::Allow,
        DecisionKind::Deny,
        DecisionKind::ApprovalRequired,
    ] {
        let expected = PolicyDecision::new(
            kind,
            vec![DecisionReason::ActionDefault {
                source: source(PolicyLayer::Repository),
                action: req.action(),
            }],
        )
        .unwrap();
        let service: Box<dyn PolicyService> = Box::new(FixedService {
            outcome: Ok(expected.clone()),
        });
        assert_eq!(service.authorize(&req), Ok(expected));
    }
    for kind in [
        AuthorizationErrorKind::InvalidRequest,
        AuthorizationErrorKind::PolicyUnavailable,
        AuthorizationErrorKind::EvaluationFailed,
        AuthorizationErrorKind::UnsupportedOperation,
    ] {
        let error = AuthorizationError::new(kind, " diagnostic ").unwrap();
        assert_eq!(error.kind(), kind);
        assert_eq!(error.diagnostic(), " diagnostic ");
        let service: Box<dyn PolicyService> = Box::new(FixedService {
            outcome: Err(error.clone()),
        });
        assert_eq!(service.authorize(&req), Err(error));
    }
}

#[test]
fn errors_implement_standard_error_without_exposing_request_values() {
    let error = ValidationError::PathNotAbsolute {
        field: "repository_root",
    };
    let standard: &dyn std::error::Error = &error;
    assert_eq!(standard.to_string(), "repository_root must be absolute");
    assert!(standard.source().is_none());
    let failure = AuthorizationError::new(
        AuthorizationErrorKind::EvaluationFailed,
        "policy evaluation failed",
    )
    .unwrap();
    let standard: &dyn std::error::Error = &failure;
    assert_eq!(
        standard.to_string(),
        "EvaluationFailed: policy evaluation failed"
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_and_arguments_are_preserved_on_unix() {
    use std::os::unix::ffi::OsStringExt;
    let value = OsString::from_vec(vec![b'x', 0xff, b'y']);
    verify_native_os_values(value);
}

#[cfg(windows)]
#[test]
fn non_utf8_paths_and_arguments_are_preserved_on_windows() {
    use std::os::windows::ffi::OsStringExt;
    let value = OsString::from_wide(&[b'x' as u16, 0xd800, b'y' as u16]);
    verify_native_os_values(value);
}

#[cfg(any(unix, windows))]
fn verify_native_os_values(value: OsString) {
    assert!(value.to_str().is_none());
    let path = PathBuf::from(&value);
    let resource = FileResource::new(path.clone(), Some(path.clone())).unwrap();
    assert_eq!(resource.path().as_os_str(), &value);
    assert_eq!(resource.destination().unwrap().as_os_str(), &value);
    let command = CommandResource::new(value.clone(), vec![value.clone()]).unwrap();
    assert_eq!(command.executable(), &value);
    assert_eq!(command.arguments(), std::slice::from_ref(&value));
    assert_eq!(
        GitResource::new(path.clone())
            .unwrap()
            .repository()
            .as_os_str(),
        &value
    );
    let absolute = root().join(&value);
    let context = Context::new("repo", absolute.clone(), absolute.clone(), None, None).unwrap();
    assert_eq!(context.repository_root().as_os_str(), absolute.as_os_str());
    assert_eq!(
        context.working_directory().as_os_str(),
        absolute.as_os_str()
    );
    assert_eq!(
        PolicySource::new(PolicyLayer::Repository, path)
            .unwrap()
            .path()
            .as_os_str(),
        &value
    );
}
