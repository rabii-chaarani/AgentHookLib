//! Pure version-one selector grammar. This module does not match resources.

use std::{net::IpAddr, num::NonZeroU16};

use policy_core::ResourceKind;

/// A token within a single path segment; never consumes a separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathToken {
    /// Literal text, preserving spelling and case.
    Literal(String),
    /// `*`: zero or more Unicode scalar values within this segment.
    AnyCharacters,
    /// `?`: exactly one Unicode scalar value within this segment.
    AnyCharacter,
}

/// One segment of a repository-relative pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    /// An ordinary segment containing literals and single-segment wildcards.
    Tokens(Vec<PathToken>),
    /// `**`: zero or more complete path segments, including hidden names.
    Recursive,
}

/// A validated repository-relative pattern, without filesystem normalization.
///
/// An empty segment list represents `.` (the repository root). Consumers must
/// apply the actual filesystem's case semantics to both selectors and resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPattern {
    original: String,
    segments: Vec<PathSegment>,
}

impl PathPattern {
    /// Return the original decoded spelling, including case.
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Return the parsed grammar; `.` has no segments.
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    /// Whether the selector names only the repository root.
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    pub(super) fn parse(input: &str) -> Option<Self> {
        let drive_prefix = input.as_bytes().get(1) == Some(&b':')
            && input
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic);
        if input.is_empty()
            || drive_prefix
            || input.starts_with('!')
            || input
                .chars()
                .any(|c| c.is_control() || matches!(c, '\\' | '[' | ']' | '{' | '}'))
        {
            return None;
        }
        let mut segments = Vec::new();
        if input != "." {
            for segment in input.split('/') {
                if matches!(segment, "" | "." | "..") {
                    return None;
                }
                if segment == "**" {
                    segments.push(PathSegment::Recursive);
                    continue;
                }
                if segment.contains("**") {
                    return None;
                }
                let mut tokens = Vec::new();
                let mut literal = String::new();
                for character in segment.chars() {
                    let wildcard = match character {
                        '*' => Some(PathToken::AnyCharacters),
                        '?' => Some(PathToken::AnyCharacter),
                        _ => None,
                    };
                    if let Some(token) = wildcard {
                        if !literal.is_empty() {
                            tokens.push(PathToken::Literal(std::mem::take(&mut literal)));
                        }
                        tokens.push(token);
                    } else {
                        literal.push(character);
                    }
                }
                if !literal.is_empty() {
                    tokens.push(PathToken::Literal(literal));
                }
                segments.push(PathSegment::Tokens(tokens));
            }
        }
        Some(Self {
            original: input.into(),
            segments,
        })
    }
}

/// Literal executable and optional exact argument list; no shell interpretation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSelector {
    executable: String,
    arguments: Option<Vec<String>>,
}

impl CommandSelector {
    /// Return the literal executable selector; no PATH lookup is implied.
    pub fn executable(&self) -> &str {
        &self.executable
    }

    /// `None` means any argv; `Some` requires exactly these values in this order.
    pub fn arguments(&self) -> Option<&[String]> {
        self.arguments.as_deref()
    }

    pub(super) fn new(executable: String, arguments: Option<Vec<String>>) -> Self {
        Self {
            executable,
            arguments,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HostIdentity {
    Dns(String),
    Ip(IpAddr),
}

/// Canonical host identity: lowercase DNS without a final dot, or a typed IP.
/// Equality compares identities, so equivalent spellings compare equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkHost(HostIdentity);

impl NetworkHost {
    /// Return the canonical DNS name, when this is a DNS identity.
    pub fn dns_name(&self) -> Option<&str> {
        match &self.0 {
            HostIdentity::Dns(name) => Some(name),
            HostIdentity::Ip(_) => None,
        }
    }

    /// Return the parsed IP, when this is an IP identity.
    pub const fn ip_addr(&self) -> Option<IpAddr> {
        match self.0 {
            HostIdentity::Ip(ip) => Some(ip),
            HostIdentity::Dns(_) => None,
        }
    }

    pub(super) fn parse(input: &str) -> Option<Self> {
        if let Ok(ip) = input.parse::<IpAddr>() {
            return Some(Self(HostIdentity::Ip(ip)));
        }
        let name = input.strip_suffix('.').unwrap_or(input);
        // Do not reinterpret a malformed numeric address as a DNS name.
        if name.is_empty()
            || name.len() > 253
            || !name.is_ascii()
            || name.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        {
            return None;
        }
        for label in name.split('.') {
            if label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return None;
            }
        }
        Some(Self(HostIdentity::Dns(name.to_ascii_lowercase())))
    }
}

/// Exact network destination, retaining source spelling without resolving DNS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSelector {
    original_host: String,
    host: NetworkHost,
    port: Option<NonZeroU16>,
}

impl NetworkSelector {
    /// Return the original decoded host spelling.
    pub fn original_host(&self) -> &str {
        &self.original_host
    }
    /// Return the canonical identity used for host equality.
    pub fn host(&self) -> &NetworkHost {
        &self.host
    }
    /// Return the explicit port; `None` leaves the port unconstrained.
    pub const fn port(&self) -> Option<NonZeroU16> {
        self.port
    }

    pub(super) fn new(original_host: String, host: NetworkHost, port: Option<NonZeroU16>) -> Self {
        Self {
            original_host,
            host,
            port,
        }
    }
}

/// A semantically validated selector for one of the four resource families.
/// Variant payloads have private constructors and read-only accessors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatedResourceSelector {
    /// A repository-relative filesystem pattern.
    File(PathPattern),
    /// A literal executable and optional exact argv.
    Command(CommandSelector),
    /// A repository-relative Git repository pattern.
    Git(PathPattern),
    /// An exact DNS/IP identity and optional nonzero port.
    Network(NetworkSelector),
}

impl ValidatedResourceSelector {
    /// Return this selector's compatible action family.
    pub const fn resource_kind(&self) -> ResourceKind {
        match self {
            Self::File(_) => ResourceKind::File,
            Self::Command(_) => ResourceKind::Command,
            Self::Git(_) => ResourceKind::Git,
            Self::Network(_) => ResourceKind::Network,
        }
    }
}
