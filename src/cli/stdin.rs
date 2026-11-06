//! `--invoice -`: the invoice read from standard input instead of the command
//! line, so it does not appear in the process list or a caller's logs (spec
//! 010). The read is bounded and strict: at most [`STDIN_LIMIT`] bytes of
//! UTF-8 holding exactly one non-empty line once ASCII whitespace is trimmed,
//! read until end of input, so the writer must close stdin after the line (a
//! second line is refused, not ignored). Stdin then carries the invoice, so it
//! cannot carry an answer: the form needs `--yes` in every mode and is refused
//! before stdin is touched without it, or when stdin is a terminal (which
//! never ends the input by itself). Nothing read is ever echoed; a rejection
//! carries no text.

use std::io::{self, IsTerminal, Read};
use std::str;

use super::write::{refuse, refuse_invoice};
use crate::outcome::{Envelope, ErrorCode};

/// The value of an option that means "read it from standard input".
pub const STDIN_VALUE: &str = "-";

/// The long options that accept [`STDIN_VALUE`]; discovery marks them.
pub const STDIN_OPTIONS: [&str; 1] = ["invoice"];

/// The most standard input may hold, in bytes (8 KiB).
pub const STDIN_LIMIT: usize = 8 * 1024;

/// Why standard input could not be taken as the value. It carries nothing that
/// was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidStdin;

/// Why `--invoice -` gave no invoice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StdinRefusal {
    /// The form is not allowed here: no `--yes`, or a terminal as stdin.
    /// Stdin was not read; this says nothing about any invoice.
    NotAllowed,
    /// What stdin held is not one invoice line: the invoice is refused.
    Content,
}

impl StdinRefusal {
    /// The outcome of `command` for this refusal: `invalid_arguments`, with the
    /// reason `invalid_invoice` only when the content was refused.
    pub(super) fn outcome<D>(self, command: &'static str) -> Envelope<D, ()> {
        match self {
            Self::NotAllowed => refuse(command, ErrorCode::InvalidArguments),
            Self::Content => refuse_invoice(command),
        }
    }
}

/// The invoice of a command: `value` itself, or the line on standard input
/// when it is [`STDIN_VALUE`]. Without `yes`, or with a terminal as stdin, that
/// form is not allowed and stdin is not read; content that is not one invoice
/// line is refused as an invoice.
pub(super) fn invoice(value: Option<String>, yes: bool) -> Result<Option<String>, StdinRefusal> {
    invoice_from(value, yes, io::stdin().is_terminal(), || io::stdin().lock())
}

/// `terminal` says whether stdin is a terminal, which never ends the input on
/// its own: `-` is then refused at once rather than waiting for end of input.
fn invoice_from<R: Read>(
    value: Option<String>,
    yes: bool,
    terminal: bool,
    input: impl FnOnce() -> R,
) -> Result<Option<String>, StdinRefusal> {
    match value {
        Some(value) if value == STDIN_VALUE => {
            if !yes || terminal {
                return Err(StdinRefusal::NotAllowed);
            }
            read_line(input())
                .map(Some)
                .map_err(|_| StdinRefusal::Content)
        }
        other => Ok(other),
    }
}

/// The one line `input` holds, without surrounding ASCII whitespace. At most
/// `STDIN_LIMIT + 1` bytes are read: one more than the limit tells an oversize
/// input apart without reading it to the end.
pub fn read_line(input: impl Read) -> Result<String, InvalidStdin> {
    let mut bytes = Vec::new();
    input
        .take(STDIN_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| InvalidStdin)?;
    if bytes.len() > STDIN_LIMIT {
        return Err(InvalidStdin);
    }
    let text = str::from_utf8(&bytes).map_err(|_| InvalidStdin)?;
    let line = text.trim_ascii();
    if line.is_empty() || line.contains(['\n', '\r']) {
        return Err(InvalidStdin);
    }
    Ok(line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_is_read_without_surrounding_whitespace() {
        for input in [
            "lnbc1abc",
            "lnbc1abc\n",
            "lnbc1abc\r\n",
            "  \tlnbc1abc \r\n\n",
            "\n\nlnbc1abc",
        ] {
            assert_eq!(
                read_line(input.as_bytes()).unwrap(),
                "lnbc1abc",
                "{input:?}"
            );
        }
        // Inner text is kept as it is; the command's own validation judges it.
        assert_eq!(
            read_line("buyer@example.com\n".as_bytes()).unwrap(),
            "buyer@example.com"
        );
    }

    #[test]
    fn empty_blank_and_several_lines_are_rejected() {
        for input in [
            "",
            "\n",
            " \t\r\n ",
            "a\nb",
            "a\n\nb\n",
            "a\rb",
            "lnbc1\nlnbc1\n",
        ] {
            assert_eq!(read_line(input.as_bytes()), Err(InvalidStdin), "{input:?}");
        }
    }

    #[test]
    fn the_limit_is_8_kib_and_nothing_past_one_more_byte_is_read() {
        let exact = "q".repeat(STDIN_LIMIT);
        assert_eq!(read_line(exact.as_bytes()).unwrap().len(), STDIN_LIMIT);
        let over = "q".repeat(STDIN_LIMIT + 1);
        assert_eq!(read_line(over.as_bytes()), Err(InvalidStdin));
        // Whitespace counts towards the limit: it is input, trimmed or not.
        let padded = format!("{}\n", "q".repeat(STDIN_LIMIT));
        assert_eq!(read_line(padded.as_bytes()), Err(InvalidStdin));

        let mut huge = io::repeat(b'q').take(10 * STDIN_LIMIT as u64);
        assert_eq!(read_line(&mut huge), Err(InvalidStdin));
        let mut rest = Vec::new();
        huge.read_to_end(&mut rest).unwrap();
        assert_eq!(rest.len(), 10 * STDIN_LIMIT - (STDIN_LIMIT + 1));
    }

    #[test]
    fn invalid_utf8_is_rejected() {
        assert_eq!(read_line(&b"lnbc1\xff\xfe\n"[..]), Err(InvalidStdin));
    }

    #[test]
    fn the_dash_needs_yes_and_without_it_stdin_is_never_touched() {
        let untouched = || -> &'static [u8] { panic!("stdin was read") };
        assert_eq!(
            invoice_from(Some("-".into()), false, false, untouched),
            Err(StdinRefusal::NotAllowed)
        );
        assert_eq!(
            invoice_from(Some("-".into()), true, false, || "lnbc1abc\n".as_bytes()),
            Ok(Some("lnbc1abc".to_string()))
        );
        assert_eq!(
            invoice_from(Some("-".into()), true, false, || "".as_bytes()),
            Err(StdinRefusal::Content)
        );
    }

    /// A terminal never sends end of input on its own: with `--yes`, `-` on a
    /// terminal fails at once instead of waiting, and stdin is not read.
    #[test]
    fn a_terminal_as_stdin_fails_at_once_without_being_read() {
        let untouched = || -> &'static [u8] { panic!("stdin was read") };
        for yes in [false, true] {
            assert_eq!(
                invoice_from(Some("-".into()), yes, true, untouched),
                Err(StdinRefusal::NotAllowed)
            );
        }
    }

    #[test]
    fn any_other_value_is_kept_as_it_is_and_stdin_is_never_touched() {
        let untouched = || -> &'static [u8] { panic!("stdin was read") };
        for yes in [false, true] {
            assert_eq!(invoice_from(None, yes, false, untouched), Ok(None));
            assert_eq!(
                invoice_from(Some(" lnbc1abc ".into()), yes, true, untouched),
                Ok(Some(" lnbc1abc ".to_string()))
            );
            assert_eq!(
                invoice_from(Some("--".into()), yes, true, untouched),
                Ok(Some("--".to_string()))
            );
        }
    }
}
