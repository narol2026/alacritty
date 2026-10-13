//! Protocol-specific OSC interpretation.
//!
//! The VTE parser only splits an OSC sequence into parameters.  This module
//! is the single place where OSC 7 and OSC 633 gain meaning.  Adding a new
//! OSC protocol should extend [`OscMessage`] and [`parse`] here instead of
//! teaching the parser or terminal grid about it.

use std::str;

/// Semantic result of an OSC sequence understood by the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscMessage {
    /// OSC 7: the shell reported its current working directory.
    WorkingDirectory(String),
    /// OSC 633 ; A: shell prompt started.
    PromptStart,
    /// OSC 633 ; B: user input started.
    CommandStart,
    /// OSC 633 ; E: command line reported by the shell.
    CommandLine(String),
    /// OSC 633 ; C: command executed.
    ///
    /// The parser sees only the bare `C` marker, so it initially carries an
    /// empty string.  [`crate::term::Term`] replaces it with the command text
    /// before forwarding the event.
    CommandExecuted(String),
    /// OSC 633 ; D: command finished, with an optional exit code.
    CommandFinished(Option<i32>),
}

/// Result of parsing an OSC sequence.
///
/// `recognized` is true when the OSC code belongs to this module even if the
/// payload is malformed or currently unsupported; `message` is populated when
/// the sequence can be dispatched.
pub(super) struct ParseResult {
    pub(super) message: Option<OscMessage>,
    pub(super) recognized: bool,
}

/// Parse an OSC sequence.
///
/// `params` includes the OSC code as its first element, as delivered by
/// [`crate::vte::ansi::Handler::osc`].
pub(super) fn parse(code: &[u8], params: &[&[u8]], _bell_terminated: bool) -> ParseResult {
    match code {
        b"7" => ParseResult {
            recognized: true,
            message: parse_working_directory(params),
        },
        b"633" => {
            let message = parse_633(params);
            ParseResult {
                recognized: message.is_some(),
                message,
            }
        },
        _ => ParseResult {
            recognized: false,
            message: None,
        },
    }
}

fn parse_working_directory(params: &[&[u8]]) -> Option<OscMessage> {
    let raw = str::from_utf8(params.get(1)?).ok()?;
    let path = parse_osc7_path(raw)?;
    Some(OscMessage::WorkingDirectory(path))
}

fn parse_633(params: &[&[u8]]) -> Option<OscMessage> {
    match params.get(1).copied()? {
        b"A" => Some(OscMessage::PromptStart),
        b"B" => Some(OscMessage::CommandStart),
        b"C" => Some(OscMessage::CommandExecuted(String::new())),
        b"D" => {
            let exit_code = params
                .get(2)
                .and_then(|raw| str::from_utf8(raw).ok())
                .and_then(|raw| raw.parse::<i32>().ok());
            Some(OscMessage::CommandFinished(exit_code))
        }
        b"E" if params.len() >= 3 => {
            let command = params[2..]
                .iter()
                .map(|raw| String::from_utf8_lossy(raw))
                .collect::<Vec<_>>()
                .join(";");
            Some(OscMessage::CommandLine(command))
        }
        _ => None,
    }
}

/// Parse the local path from a `file://hostname/path` URL string.
///
/// Returns `None` when the string does not start with `file://` or does not
/// contain a path segment after the host component.
fn parse_osc7_path(raw: &str) -> Option<String> {
    let after_scheme = raw.strip_prefix("file://")?;

    // The first `/` after the hostname separates the host from the path.
    //   file://hostname/path   or   file:///path  (empty hostname)
    let path_start = after_scheme.find('/')?;
    let encoded = &after_scheme[path_start..];

    Some(percent_decode(encoded))
}

/// Minimal percent-decode for `file://` URL paths.
///
/// Handles `%XX` hex escapes. Bytes that don't form valid UTF-8 are passed
/// through as replacement characters.
pub(super) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1]);
            let lo = hex_val(bytes[i + 2]);
            if let (Some(h), Some(l)) = (hi, lo) {
                result.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        result.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

#[inline]
fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}
