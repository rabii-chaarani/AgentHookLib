use std::{collections::BTreeMap, path::Path, str::Chars};

use policy_core::{Action, CommandAction, FileAction, GitAction, NetworkAction};
use yaml_rust2::{
    parser::{Event, Parser},
    scanner::{Marker, TScalarStyle},
};

use crate::{
    Effect, MAX_INPUT_BYTES, MAX_NESTING_DEPTH, ParseError, ParseErrorKind, ParsedPolicy,
    PolicyDefaults, PolicyRule, ResourceSelector, SourceLocation,
};

fn location(marker: Marker) -> SourceLocation {
    SourceLocation {
        line: marker.line(),
        // yaml-rust2 0.13 emits zero-based columns despite its Marker docs.
        column: marker.col() + 1,
    }
}

fn child_path(parent: &str, key: &str) -> String {
    if parent == "$" {
        key.to_owned()
    } else {
        format!("{parent}.{key}")
    }
}

struct Diagnostics<'a> {
    source: &'a Path,
}

impl Diagnostics<'_> {
    fn error(&self, kind: ParseErrorKind, location: SourceLocation, path: &str) -> ParseError {
        ParseError {
            kind,
            source: self.source.to_owned(),
            location,
            field_path: Some(path.to_owned()),
            original_location: None,
        }
    }
}

struct Node {
    location: SourceLocation,
    path: String,
    value: Value,
}

enum Value {
    Null,
    Scalar(String),
    Sequence(Vec<Node>),
    Mapping(Vec<Field>),
}

struct Field {
    key: String,
    location: SourceLocation,
    value: Node,
}

struct Reader<'a> {
    parser: Parser<Chars<'a>>,
    diagnostics: Diagnostics<'a>,
    locations: BTreeMap<String, SourceLocation>,
}

impl Reader<'_> {
    fn next(&mut self, path: &str) -> Result<(Event, SourceLocation), ParseError> {
        let (event, marker) = self.parser.next_token().map_err(|error| {
            // Scanner lookahead may hit its own flow-depth bound before
            // emitting the events on which our smaller bound is enforced.
            let kind = if error.info() == "recursion limit exceeded" {
                ParseErrorKind::NestingTooDeep
            } else {
                ParseErrorKind::InvalidYaml
            };
            self.diagnostics
                .error(kind, location(*error.marker()), path)
        })?;
        let loc = location(marker);
        let (anchor, tag) = match &event {
            Event::Scalar(_, _, anchor, tag)
            | Event::SequenceStart(anchor, tag)
            | Event::MappingStart(anchor, tag) => (*anchor, tag.is_some()),
            Event::Alias(_) => (1, false),
            _ => (0, false),
        };
        if tag {
            return Err(self
                .diagnostics
                .error(ParseErrorKind::ExplicitTag, loc, path));
        }
        if anchor != 0 {
            return Err(self
                .diagnostics
                .error(ParseErrorKind::AnchorOrAlias, loc, path));
        }
        Ok((event, loc))
    }

    fn node(
        &mut self,
        event: Event,
        loc: SourceLocation,
        path: String,
        depth: usize,
    ) -> Result<Node, ParseError> {
        if matches!(event, Event::MappingStart(..) | Event::SequenceStart(..))
            && depth >= MAX_NESTING_DEPTH
        {
            return Err(self
                .diagnostics
                .error(ParseErrorKind::NestingTooDeep, loc, &path));
        }
        let value = match event {
            Event::Scalar(text, style, _, _) => {
                if style == TScalarStyle::Plain
                    && matches!(text.as_str(), "" | "~" | "null" | "Null" | "NULL")
                {
                    Value::Null
                } else {
                    Value::Scalar(text)
                }
            }
            Event::SequenceStart(..) => {
                let mut values = Vec::new();
                loop {
                    let item_path = format!("{path}[{}]", values.len());
                    let (event, loc) = self.next(&item_path)?;
                    if event == Event::SequenceEnd {
                        break;
                    }
                    values.push(self.node(event, loc, item_path, depth + 1)?);
                }
                Value::Sequence(values)
            }
            Event::MappingStart(..) => {
                let mut fields = Vec::new();
                let mut keys = BTreeMap::new();
                loop {
                    let (event, key_loc) = self.next(&path)?;
                    if event == Event::MappingEnd {
                        break;
                    }
                    let Event::Scalar(key, style, _, _) = event else {
                        return Err(self.diagnostics.error(
                            ParseErrorKind::InvalidMappingKey,
                            key_loc,
                            &path,
                        ));
                    };
                    if style == TScalarStyle::Plain
                        && matches!(key.as_str(), "" | "~" | "null" | "Null" | "NULL")
                    {
                        return Err(self.diagnostics.error(
                            ParseErrorKind::InvalidMappingKey,
                            key_loc,
                            &path,
                        ));
                    }
                    let value_path = child_path(&path, &key);
                    if key == "<<" {
                        return Err(self.diagnostics.error(
                            ParseErrorKind::MergeKey,
                            key_loc,
                            &value_path,
                        ));
                    }
                    if let Some(first) = keys.insert(key.clone(), key_loc) {
                        let mut error = self.diagnostics.error(
                            ParseErrorKind::DuplicateKey,
                            key_loc,
                            &value_path,
                        );
                        error.original_location = Some(first);
                        return Err(error);
                    }
                    let (event, loc) = self.next(&value_path)?;
                    let value = self.node(event, loc, value_path, depth + 1)?;
                    fields.push(Field {
                        key,
                        location: key_loc,
                        value,
                    });
                }
                Value::Mapping(fields)
            }
            _ => {
                return Err(self
                    .diagnostics
                    .error(ParseErrorKind::InvalidYaml, loc, &path));
            }
        };
        // Block mappings are marked at the colon, after their first key.
        // Flow mappings start at `{`, which precedes their first key.
        let loc = match &value {
            Value::Mapping(fields) => fields.first().map_or(loc, |first| {
                if (first.location.line, first.location.column) < (loc.line, loc.column) {
                    first.location
                } else {
                    loc
                }
            }),
            _ => loc,
        };
        self.locations.insert(path.clone(), loc);
        Ok(Node {
            location: loc,
            path,
            value,
        })
    }

    fn document(&mut self) -> Result<Node, ParseError> {
        self.next("$")?; // StreamStart
        let (event, loc) = self.next("$")?;
        if event == Event::StreamEnd {
            return Err(self
                .diagnostics
                .error(ParseErrorKind::EmptyDocument, loc, "$"));
        }
        let (event, loc) = self.next("$")?;
        let node = self.node(event, loc, "$".to_owned(), 0)?;
        if matches!(node.value, Value::Null) {
            return Err(self
                .diagnostics
                .error(ParseErrorKind::EmptyDocument, loc, "$"));
        }
        self.next("$")?; // DocumentEnd
        let (event, loc) = self.next("$")?;
        if event != Event::StreamEnd {
            return Err(self
                .diagnostics
                .error(ParseErrorKind::MultipleDocuments, loc, "$"));
        }
        Ok(node)
    }
}

impl Node {
    fn error(&self, diagnostics: &Diagnostics<'_>, kind: ParseErrorKind) -> ParseError {
        diagnostics.error(kind, self.location, &self.path)
    }

    fn mapping(&self, diagnostics: &Diagnostics<'_>) -> Result<&[Field], ParseError> {
        match &self.value {
            Value::Mapping(fields) => Ok(fields),
            _ => Err(self.error(diagnostics, ParseErrorKind::InvalidType)),
        }
    }

    fn fields(&self, diagnostics: &Diagnostics<'_>, allowed: &[&str]) -> Result<(), ParseError> {
        for field in self.mapping(diagnostics)? {
            if !allowed.contains(&field.key.as_str()) {
                return Err(diagnostics.error(
                    ParseErrorKind::UnknownField,
                    field.location,
                    &field.value.path,
                ));
            }
        }
        Ok(())
    }

    fn optional(&self, key: &str) -> Option<&Node> {
        match &self.value {
            Value::Mapping(fields) => fields
                .iter()
                .find(|field| field.key == key)
                .map(|field| &field.value),
            _ => None,
        }
    }

    fn required(&self, diagnostics: &Diagnostics<'_>, key: &str) -> Result<&Node, ParseError> {
        self.optional(key).ok_or_else(|| {
            diagnostics.error(
                ParseErrorKind::MissingField,
                self.location,
                &child_path(&self.path, key),
            )
        })
    }

    fn text(&self, diagnostics: &Diagnostics<'_>) -> Result<&str, ParseError> {
        match &self.value {
            Value::Scalar(text) => Ok(text),
            _ => Err(self.error(diagnostics, ParseErrorKind::InvalidType)),
        }
    }

    fn sequence(&self, diagnostics: &Diagnostics<'_>) -> Result<&[Node], ParseError> {
        match &self.value {
            Value::Sequence(values) => Ok(values),
            _ => Err(self.error(diagnostics, ParseErrorKind::InvalidType)),
        }
    }

    fn integer<T: std::str::FromStr>(
        &self,
        diagnostics: &Diagnostics<'_>,
    ) -> Result<T, ParseError> {
        let text = self.text(diagnostics)?;
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(self.error(diagnostics, ParseErrorKind::InvalidType));
        }
        text.parse()
            .map_err(|_| self.error(diagnostics, ParseErrorKind::InvalidType))
    }

    fn effect(&self, diagnostics: &Diagnostics<'_>) -> Result<Effect, ParseError> {
        match self.text(diagnostics)? {
            "allow" => Ok(Effect::Allow),
            "deny" => Ok(Effect::Deny),
            "ask" => Ok(Effect::Ask),
            _ => Err(self.error(diagnostics, ParseErrorKind::UnknownEffect)),
        }
    }

    fn action(&self, diagnostics: &Diagnostics<'_>) -> Result<Action, ParseError> {
        Ok(match self.text(diagnostics)? {
            "file.read" => Action::File(FileAction::Read),
            "file.write" => Action::File(FileAction::Write),
            "file.delete" => Action::File(FileAction::Delete),
            "file.rename" => Action::File(FileAction::Rename),
            "command.execute" => Action::Command(CommandAction::Execute),
            "git.commit" => Action::Git(GitAction::Commit),
            "git.checkout" => Action::Git(GitAction::Checkout),
            "git.reset" => Action::Git(GitAction::Reset),
            "git.push" => Action::Git(GitAction::Push),
            "git.reset-hard" => Action::Git(GitAction::ResetHard),
            "git.force-push" => Action::Git(GitAction::ForcePush),
            "network.connect" => Action::Network(NetworkAction::Connect),
            _ => return Err(self.error(diagnostics, ParseErrorKind::UnknownAction)),
        })
    }

    fn defaults(&self, diagnostics: &Diagnostics<'_>) -> Result<PolicyDefaults, ParseError> {
        self.fields(diagnostics, &["file", "command", "git", "network"])?;
        let effect = |key| {
            self.optional(key)
                .map(|node| node.effect(diagnostics))
                .transpose()
        };
        Ok(PolicyDefaults {
            file: effect("file")?,
            command: effect("command")?,
            git: effect("git")?,
            network: effect("network")?,
        })
    }

    fn selector(&self, diagnostics: &Diagnostics<'_>) -> Result<ResourceSelector, ParseError> {
        self.mapping(diagnostics)?;
        let kind = self.required(diagnostics, "kind")?;
        let text = |key| {
            self.required(diagnostics, key)?
                .text(diagnostics)
                .map(str::to_owned)
        };
        Ok(match kind.text(diagnostics)? {
            "file" => {
                self.fields(diagnostics, &["kind", "pattern"])?;
                ResourceSelector::File {
                    pattern: text("pattern")?,
                }
            }
            "command" => {
                self.fields(diagnostics, &["kind", "executable", "arguments"])?;
                ResourceSelector::Command {
                    executable: text("executable")?,
                    arguments: self
                        .optional("arguments")
                        .map(|node| {
                            node.sequence(diagnostics)?
                                .iter()
                                .map(|item| item.text(diagnostics).map(str::to_owned))
                                .collect()
                        })
                        .transpose()?,
                }
            }
            "git" => {
                self.fields(diagnostics, &["kind", "repository"])?;
                ResourceSelector::Git {
                    repository: text("repository")?,
                }
            }
            "network" => {
                self.fields(diagnostics, &["kind", "host", "port"])?;
                ResourceSelector::Network {
                    host: text("host")?,
                    port: self
                        .optional("port")
                        .map(|node| node.integer(diagnostics))
                        .transpose()?,
                }
            }
            _ => return Err(kind.error(diagnostics, ParseErrorKind::UnknownResourceKind)),
        })
    }

    fn rule(&self, diagnostics: &Diagnostics<'_>) -> Result<PolicyRule, ParseError> {
        self.fields(
            diagnostics,
            &["id", "description", "effect", "actions", "resource"],
        )?;
        Ok(PolicyRule {
            id: self
                .required(diagnostics, "id")?
                .text(diagnostics)?
                .to_owned(),
            description: self
                .optional("description")
                .map(|node| node.text(diagnostics).map(str::to_owned))
                .transpose()?,
            effect: self.required(diagnostics, "effect")?.effect(diagnostics)?,
            actions: self
                .required(diagnostics, "actions")?
                .sequence(diagnostics)?
                .iter()
                .map(|node| node.action(diagnostics))
                .collect::<Result<_, _>>()?,
            resource: self
                .required(diagnostics, "resource")?
                .selector(diagnostics)?,
        })
    }
}

/// Parse exactly one UTF-8 YAML policy document without I/O or execution.
///
/// `source` is only a diagnostic label. Success establishes structural validity,
/// not supported versions, unique rule identifiers, compatible action/resource
/// pairs, valid selector grammar, or permission to execute. Empty action lists
/// and blank identifiers are retained for the semantic validation stage.
///
/// # Errors
/// Returns a source-located error for malformed or unsupported YAML constructs,
/// invalid policy structure or names, and input or nesting limit violations.
pub fn parse_policy(source: &Path, input: &str) -> Result<ParsedPolicy, ParseError> {
    let diagnostics = Diagnostics { source };
    if input.len() > MAX_INPUT_BYTES {
        return Err(diagnostics.error(
            ParseErrorKind::InputTooLarge,
            SourceLocation { line: 1, column: 1 },
            "$",
        ));
    }
    let mut reader = Reader {
        parser: Parser::new_from_str(input),
        diagnostics,
        locations: BTreeMap::new(),
    };
    let document = reader.document()?;
    let diagnostics = &reader.diagnostics;
    document.fields(diagnostics, &["version", "defaults", "rules"])?;
    let version = document
        .required(diagnostics, "version")?
        .integer(diagnostics)?;
    let defaults = document
        .optional("defaults")
        .map(|node| node.defaults(diagnostics))
        .transpose()?;
    let rules = document
        .required(diagnostics, "rules")?
        .sequence(diagnostics)?
        .iter()
        .map(|node| node.rule(diagnostics))
        .collect::<Result<_, _>>()?;
    Ok(ParsedPolicy {
        source: source.to_owned(),
        version,
        defaults,
        rules,
        locations: reader.locations,
    })
}
