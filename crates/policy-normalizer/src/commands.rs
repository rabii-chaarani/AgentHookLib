//! Parse bounded POSIX-like command syntax into policy operations.
//!
//! Parsing is pure: this module never invokes a shell, executes a command,
//! evaluates an expansion, or decides whether an operation is permitted.
#![forbid(unsafe_code)]

use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fmt,
    path::Path,
};

use policy_core::{Action, CommandAction, CommandResource, Context, GitAction};

/// A parsed command and the context that applies to all of its operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    context: Context,
    operations: Vec<ParsedOperation>,
}

impl ParsedCommand {
    /// Return the original request context, including its working directory.
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// Return every operation in source order.
    pub fn operations(&self) -> &[ParsedOperation] {
        &self.operations
    }
}

/// One semantic classification paired with its original parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedOperation {
    action: Action,
    invocation: CommandResource,
}

impl ParsedOperation {
    /// Return the semantic operation. This is a classification, not permission.
    pub fn action(&self) -> Action {
        self.action
    }

    /// Return the invocation's executable and argument vector.
    pub fn invocation(&self) -> &CommandResource {
        &self.invocation
    }
}

/// The category of a command parsing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommandParseErrorKind {
    /// The input or one of its command segments is empty.
    EmptyCommand,
    /// Quotes, separators, or command structure are malformed.
    MalformedSyntax,
    /// The syntax is outside the parser's supported grammar.
    UnsupportedSyntax,
    /// The invocation is valid syntax but its operation cannot be classified safely.
    UnsupportedOperation,
}

/// A command parsing failure with a byte offset and no echoed input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandParseError {
    kind: CommandParseErrorKind,
    offset: usize,
}

impl CommandParseError {
    fn new(kind: CommandParseErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// Return the failure category.
    pub fn kind(&self) -> CommandParseErrorKind {
        self.kind
    }

    /// Return the UTF-8 byte offset where parsing failed.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for CommandParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self.kind {
            CommandParseErrorKind::EmptyCommand => "command is empty",
            CommandParseErrorKind::MalformedSyntax => "command syntax is malformed",
            CommandParseErrorKind::UnsupportedSyntax => "command syntax is unsupported",
            CommandParseErrorKind::UnsupportedOperation => "command operation is unsupported",
        };
        write!(f, "{reason} at byte offset {}", self.offset)
    }
}

impl Error for CommandParseError {}

/// Parse command text into ordered semantic operations without executing it.
///
/// Quoting and the `;`, `&&`, `||`, and `|` operators are recognized. Every
/// constituent is returned separately, so a caller can require a decision for
/// each operation. The returned context is the supplied context unchanged.
///
/// Unsupported or malformed syntax returns an error and no partial result.
pub fn parse_command_line(
    source: &str,
    context: &Context,
) -> Result<ParsedCommand, CommandParseError> {
    if source.trim().is_empty() {
        return Err(CommandParseError::new(
            CommandParseErrorKind::EmptyCommand,
            0,
        ));
    }
    let tokens = lex(source)?;
    if tokens.is_empty() {
        return Err(CommandParseError::new(
            CommandParseErrorKind::EmptyCommand,
            0,
        ));
    }

    let mut operations = Vec::new();
    let mut segment = Vec::new();
    let mut trailing_separator = None;

    for token in tokens {
        match token.kind {
            TokenKind::Word => {
                trailing_separator = None;
                segment.push(token);
            }
            TokenKind::Separator(separator) => {
                if segment.is_empty() {
                    return Err(CommandParseError::new(
                        CommandParseErrorKind::MalformedSyntax,
                        token.start,
                    ));
                }
                operations.push(parse_segment(&segment)?);
                segment.clear();
                trailing_separator = Some((separator, token.start));
            }
        }
    }

    if !segment.is_empty() {
        operations.push(parse_segment(&segment)?);
    } else if let Some((separator, offset)) = trailing_separator
        && separator != Separator::Sequence
    {
        return Err(CommandParseError::new(
            CommandParseErrorKind::MalformedSyntax,
            offset,
        ));
    }

    if operations.is_empty() {
        return Err(CommandParseError::new(
            CommandParseErrorKind::EmptyCommand,
            0,
        ));
    }

    Ok(ParsedCommand {
        context: context.clone(),
        operations,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Separator {
    Sequence,
    And,
    Or,
    Pipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Word,
    Separator(Separator),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    value: String,
    start: usize,
    quoted: bool,
    kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quote {
    Single,
    Double,
}

fn lex(source: &str) -> Result<Vec<Token>, CommandParseError> {
    let mut tokens = Vec::new();
    let mut value = String::new();
    let mut token_started = false;
    let mut token_quoted = false;
    let mut token_start = 0;
    let mut quote = None;
    let mut quote_start = 0;
    let mut chars = source.char_indices().peekable();

    while let Some((offset, ch)) = chars.next() {
        if ch == '\0' {
            return Err(CommandParseError::new(
                CommandParseErrorKind::MalformedSyntax,
                offset,
            ));
        }
        if let Some(active_quote) = quote {
            match active_quote {
                Quote::Single => {
                    if ch == '\'' {
                        quote = None;
                    } else {
                        value.push(ch);
                    }
                }
                Quote::Double => match ch {
                    '"' => quote = None,
                    '$' | '`' => {
                        return Err(CommandParseError::new(
                            CommandParseErrorKind::UnsupportedSyntax,
                            offset,
                        ));
                    }
                    '\\' => {
                        let Some((next_offset, next)) = chars.next() else {
                            return Err(CommandParseError::new(
                                CommandParseErrorKind::MalformedSyntax,
                                offset,
                            ));
                        };
                        match next {
                            '\n' => {}
                            '"' | '\\' | '$' | '`' => value.push(next),
                            other => {
                                value.push('\\');
                                value.push(other);
                            }
                        }
                        let _ = next_offset;
                    }
                    other => value.push(other),
                },
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                if !token_started {
                    token_start = offset;
                    token_started = true;
                }
                token_quoted = true;
                quote_start = offset;
                quote = Some(if ch == '\'' {
                    Quote::Single
                } else {
                    Quote::Double
                });
            }
            '\\' => {
                if !token_started {
                    token_start = offset;
                    token_started = true;
                }
                token_quoted = true;
                let Some((_, next)) = chars.next() else {
                    return Err(CommandParseError::new(
                        CommandParseErrorKind::MalformedSyntax,
                        offset,
                    ));
                };
                if next != '\n' {
                    value.push(next);
                }
            }
            c if c.is_whitespace() => {
                flush_word(
                    &mut tokens,
                    &mut value,
                    &mut token_started,
                    &mut token_quoted,
                    token_start,
                );
                if ch == '\n'
                    && !matches!(
                        tokens.last(),
                        Some(Token {
                            kind: TokenKind::Separator(_),
                            ..
                        })
                    )
                {
                    tokens.push(Token {
                        value: String::new(),
                        start: offset,
                        quoted: false,
                        kind: TokenKind::Separator(Separator::Sequence),
                    });
                }
            }
            ';' => {
                flush_word(
                    &mut tokens,
                    &mut value,
                    &mut token_started,
                    &mut token_quoted,
                    token_start,
                );
                tokens.push(Token {
                    value: String::new(),
                    start: offset,
                    quoted: false,
                    kind: TokenKind::Separator(Separator::Sequence),
                });
            }
            '|' => {
                flush_word(
                    &mut tokens,
                    &mut value,
                    &mut token_started,
                    &mut token_quoted,
                    token_start,
                );
                let separator = if matches!(chars.peek(), Some((_, '|'))) {
                    chars.next();
                    Separator::Or
                } else {
                    Separator::Pipe
                };
                tokens.push(Token {
                    value: String::new(),
                    start: offset,
                    quoted: false,
                    kind: TokenKind::Separator(separator),
                });
            }
            '&' => {
                flush_word(
                    &mut tokens,
                    &mut value,
                    &mut token_started,
                    &mut token_quoted,
                    token_start,
                );
                if matches!(chars.peek(), Some((_, '&'))) {
                    chars.next();
                    tokens.push(Token {
                        value: String::new(),
                        start: offset,
                        quoted: false,
                        kind: TokenKind::Separator(Separator::And),
                    });
                } else {
                    return Err(CommandParseError::new(
                        CommandParseErrorKind::UnsupportedSyntax,
                        offset,
                    ));
                }
            }
            '<' | '>' | '(' | ')' | '{' | '}' | '^' | '[' | ']' => {
                return Err(CommandParseError::new(
                    CommandParseErrorKind::UnsupportedSyntax,
                    offset,
                ));
            }
            '$' | '`' => {
                return Err(CommandParseError::new(
                    CommandParseErrorKind::UnsupportedSyntax,
                    offset,
                ));
            }
            '*' | '?' => {
                return Err(CommandParseError::new(
                    CommandParseErrorKind::UnsupportedSyntax,
                    offset,
                ));
            }
            '~' if !token_started => {
                return Err(CommandParseError::new(
                    CommandParseErrorKind::UnsupportedSyntax,
                    offset,
                ));
            }
            '!' => {
                return Err(CommandParseError::new(
                    CommandParseErrorKind::UnsupportedSyntax,
                    offset,
                ));
            }
            '#' if !token_started => {
                return Err(CommandParseError::new(
                    CommandParseErrorKind::UnsupportedSyntax,
                    offset,
                ));
            }
            '%' if has_cmd_variable_expansion(source, offset) => {
                return Err(CommandParseError::new(
                    CommandParseErrorKind::UnsupportedSyntax,
                    offset,
                ));
            }
            other => {
                if !token_started {
                    token_start = offset;
                    token_started = true;
                }
                value.push(other);
            }
        }
    }

    if quote.is_some() {
        return Err(CommandParseError::new(
            CommandParseErrorKind::MalformedSyntax,
            quote_start,
        ));
    }

    flush_word(
        &mut tokens,
        &mut value,
        &mut token_started,
        &mut token_quoted,
        token_start,
    );
    Ok(tokens)
}

fn flush_word(
    tokens: &mut Vec<Token>,
    value: &mut String,
    token_started: &mut bool,
    token_quoted: &mut bool,
    token_start: usize,
) {
    if !*token_started {
        return;
    }
    tokens.push(Token {
        value: std::mem::take(value),
        start: token_start,
        quoted: *token_quoted,
        kind: TokenKind::Word,
    });
    *token_started = false;
    *token_quoted = false;
}

fn has_cmd_variable_expansion(source: &str, offset: usize) -> bool {
    let Some(rest) = source.get(offset + 1..) else {
        return false;
    };
    if matches!(rest.chars().next(), Some('%' | '*' | '~' | '0'..='9')) {
        return true;
    }
    rest.chars()
        .take_while(|ch| !ch.is_whitespace() && !matches!(ch, ';' | '&' | '|'))
        .any(|ch| ch == '%')
}

fn parse_segment(tokens: &[Token]) -> Result<ParsedOperation, CommandParseError> {
    let Some(first) = tokens.first() else {
        return Err(CommandParseError::new(
            CommandParseErrorKind::EmptyCommand,
            0,
        ));
    };

    if is_control_word(first) {
        return Err(CommandParseError::new(
            CommandParseErrorKind::UnsupportedSyntax,
            first.start,
        ));
    }
    if is_assignment_word(&first.value) {
        return Err(CommandParseError::new(
            CommandParseErrorKind::UnsupportedSyntax,
            first.start,
        ));
    }

    let target_index = unwrap_supported_wrappers(tokens)?;
    let action = classify(tokens, target_index)?;
    let executable = OsString::from(first.value.as_str());
    let arguments = tokens[1..]
        .iter()
        .map(|token| OsString::from(token.value.as_str()))
        .collect();
    let invocation = CommandResource::new(executable, arguments)
        .map_err(|_| CommandParseError::new(CommandParseErrorKind::MalformedSyntax, first.start))?;

    Ok(ParsedOperation { action, invocation })
}

fn is_control_word(token: &Token) -> bool {
    !token.quoted
        && matches!(
            token.value.as_str(),
            "if" | "then"
                | "elif"
                | "else"
                | "fi"
                | "for"
                | "while"
                | "until"
                | "case"
                | "esac"
                | "select"
                | "function"
                | "do"
                | "done"
                | "coproc"
        )
}

fn is_assignment_word(value: &str) -> bool {
    let Some((name, _)) = value.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn unwrap_supported_wrappers(tokens: &[Token]) -> Result<usize, CommandParseError> {
    let mut index = 0;
    loop {
        let token = &tokens[index];
        let executable = executable_name(&token.value);

        if is_shell_interpreter(executable) {
            return Err(CommandParseError::new(
                CommandParseErrorKind::UnsupportedSyntax,
                token.start,
            ));
        }
        if is_unsupported_wrapper(executable) {
            return Err(CommandParseError::new(
                CommandParseErrorKind::UnsupportedOperation,
                token.start,
            ));
        }

        let wrapper = if name_matches(executable, "env") {
            Some("env")
        } else if name_matches(executable, "command") {
            Some("command")
        } else if name_matches(executable, "exec") {
            Some("exec")
        } else if name_matches(executable, "time") {
            Some("time")
        } else {
            None
        };

        let Some(wrapper) = wrapper else {
            return Ok(index);
        };
        let Some(next) = tokens.get(index + 1) else {
            return Err(CommandParseError::new(
                CommandParseErrorKind::UnsupportedOperation,
                token.start,
            ));
        };
        if next.value.starts_with('-') || (wrapper == "env" && next.value.contains('=')) {
            return Err(CommandParseError::new(
                CommandParseErrorKind::UnsupportedOperation,
                next.start,
            ));
        }
        index += 1;
    }
}

fn classify(tokens: &[Token], executable_index: usize) -> Result<Action, CommandParseError> {
    let executable = &tokens[executable_index];
    if is_stateful_builtin(executable_name(&executable.value)) {
        return Err(CommandParseError::new(
            CommandParseErrorKind::UnsupportedOperation,
            executable.start,
        ));
    }
    if !name_matches(executable_name(&executable.value), "git") {
        return Ok(Action::Command(CommandAction::Execute));
    }

    let Some(subcommand) = tokens.get(executable_index + 1) else {
        return Ok(Action::Command(CommandAction::Execute));
    };
    let args = &tokens[executable_index + 2..];
    let action = match subcommand.value.as_str() {
        "commit" => GitAction::Commit,
        "checkout" => GitAction::Checkout,
        "reset" => classify_reset(args)?,
        "push" => classify_push(args)?,
        _ => return Ok(Action::Command(CommandAction::Execute)),
    };
    Ok(Action::Git(action))
}

fn executable_name(executable: &str) -> &str {
    Path::new(executable)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(executable)
}

fn name_matches(actual: &str, expected: &str) -> bool {
    if cfg!(windows) {
        windows_executable_stem(actual).eq_ignore_ascii_case(expected)
    } else {
        actual == expected
    }
}

fn windows_executable_stem(actual: &str) -> &str {
    actual
        .len()
        .checked_sub(4)
        .filter(|&start| {
            actual
                .get(start..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".exe"))
        })
        .map(|start| &actual[..start])
        .unwrap_or(actual)
}

fn unsupported_option(token: &Token) -> CommandParseError {
    CommandParseError::new(CommandParseErrorKind::UnsupportedOperation, token.start)
}

fn matches_long_option(option: &str, full: &str, shortest: &str) -> bool {
    option.len() >= shortest.len() && full.starts_with(option)
}

fn classify_reset(args: &[Token]) -> Result<GitAction, CommandParseError> {
    let mut options = true;
    let mut hard = false;
    for token in args {
        let value = token.value.as_str();
        if options && value == "--" {
            options = false;
        } else if options && value.starts_with('-') {
            if matches!(value, "--hard" | "--har") {
                hard = true;
            } else if !matches!(
                value,
                "--soft" | "--mixed" | "--merge" | "--keep" | "--patch" | "-p" | "-q" | "-N"
            ) {
                return Err(unsupported_option(token));
            }
        }
    }
    Ok(if hard {
        GitAction::ResetHard
    } else {
        GitAction::Reset
    })
}

fn classify_push(args: &[Token]) -> Result<GitAction, CommandParseError> {
    let mut options = true;
    let mut force = false;
    let mut lease = false;
    let mut forced_refspec = false;
    let mut remote_supplied = false;
    let mut next_is_value = false;

    for token in args {
        let value = token.value.as_str();
        if next_is_value {
            next_is_value = false;
            continue;
        }
        if options && value == "--" {
            options = false;
            continue;
        }
        if options && value.starts_with("--") {
            let (option, attached_value) = value
                .split_once('=')
                .map_or((value, None), |(name, value)| (name, Some(value)));
            match option {
                "--force" if attached_value.is_none() => force = true,
                "--no-force" if attached_value.is_none() => force = false,
                name if matches_long_option(name, "--force-with-lease", "--force-w")
                    && attached_value.is_none_or(|v| !v.is_empty()) =>
                {
                    lease = true;
                }
                name if matches_long_option(name, "--no-force-with-lease", "--no-force-w")
                    && attached_value.is_none() =>
                {
                    lease = false;
                }
                name if (matches_long_option(name, "--force-if-includes", "--force-i")
                    || matches_long_option(name, "--no-force-if-includes", "--no-force-i"))
                    && attached_value.is_none() => {}
                "--repo" | "--receive-pack" | "--exec" | "--push-option" => {
                    if option == "--repo" {
                        remote_supplied = true;
                    }
                    next_is_value = attached_value.is_none();
                }
                "--all" | "--mirror" | "--tags" | "--delete" | "--dry-run" | "--porcelain"
                | "--quiet" | "--verbose" | "--prune" | "--follow-tags" | "--set-upstream"
                | "--no-verify" | "--atomic" | "--progress" | "--ipv4" | "--ipv6" | "--thin"
                | "--no-thin"
                    if attached_value.is_none() => {}
                _ => return Err(unsupported_option(token)),
            }
            continue;
        }
        if options && value.starts_with('-') && value.len() > 1 {
            let short_options = &value[1..];
            for (index, option) in short_options.char_indices() {
                match option {
                    'f' => force = true,
                    'q' | 'v' | 'n' | 'u' | 'd' => {}
                    'o' => {
                        next_is_value = index + option.len_utf8() == short_options.len();
                        break;
                    }
                    _ => return Err(unsupported_option(token)),
                }
            }
            continue;
        }
        if !remote_supplied {
            remote_supplied = true;
        } else if value.starts_with('+') && value.len() > 1 {
            forced_refspec = true;
        }
    }
    if next_is_value {
        return Err(args.last().map_or(
            CommandParseError::new(CommandParseErrorKind::UnsupportedOperation, 0),
            unsupported_option,
        ));
    }
    Ok(if force || lease || forced_refspec {
        GitAction::ForcePush
    } else {
        GitAction::Push
    })
}

fn is_shell_interpreter(name: &str) -> bool {
    [
        "sh",
        "bash",
        "dash",
        "zsh",
        "ksh",
        "fish",
        "cmd",
        "cmd.exe",
        "powershell",
        "pwsh",
    ]
    .iter()
    .any(|candidate| name_matches(name, candidate))
}

fn is_unsupported_wrapper(name: &str) -> bool {
    [
        "sudo", "su", "doas", "nice", "nohup", "timeout", "xargs", "setsid", "stdbuf", "chroot",
        "builtin",
    ]
    .iter()
    .any(|candidate| name_matches(name, candidate))
}

fn is_stateful_builtin(name: &str) -> bool {
    [
        "cd", "pushd", "popd", "export", "unset", "source", ".", "eval", "set", "read", "umask",
        "ulimit", "trap", "exit", "return", "break", "continue", "shift", "alias", "unalias",
    ]
    .iter()
    .any(|candidate| name_matches(name, candidate))
}

#[cfg(test)]
mod tests {
    use super::windows_executable_stem;

    #[test]
    fn windows_exe_suffix_is_case_insensitive() {
        assert_eq!(windows_executable_stem("GIT.EXE"), "GIT");
        assert_eq!(windows_executable_stem("git.ExE"), "git");
        assert_eq!(windows_executable_stem("git"), "git");
        assert_eq!(windows_executable_stem("git.exe.old"), "git.exe.old");
    }
}
