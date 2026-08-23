//! What went wrong, and where.

use alloc::string::String;
use core::fmt;

/// The category of a parse failure.
///
/// Matching on the kind is the supported way to branch; the [`Error`]'s
/// `Display` text is for humans and is not a stable contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The document ended while a value was still open.
    UnexpectedEof,
    /// A character appeared where the grammar does not allow one.
    UnexpectedChar,
    /// A complete value was read, then more non-whitespace followed it.
    ///
    /// Its own kind rather than a generic syntax error, because accepting a
    /// document's first value and discarding the rest is the failure a caller
    /// most needs to distinguish: nothing looked wrong to the sender.
    TrailingData,
    /// A number that does not match JSON's grammar (`5.`, `1e`, `+1`, a bare `-`).
    InvalidNumber,
    /// A number JSON allows but this crate will not represent inexactly.
    ///
    /// An integer outside `u64`/`i64`, or a float literal that overflows to
    /// infinity. Silently degrading either is the precision loss this crate
    /// exists to refuse; see [`ParseOptions::with_lossy_big_integers`].
    ///
    /// [`ParseOptions::with_lossy_big_integers`]: crate::ParseOptions::with_lossy_big_integers
    NumberOutOfRange,
    /// A raw `U+0000`..=`U+001F` inside a string. JSON requires these escaped.
    ControlCharacterInString,
    /// A backslash escape JSON does not define, or a malformed `\uXXXX`.
    InvalidEscape,
    /// A `\uXXXX` surrogate with no matching partner.
    ///
    /// Unrepresentable in a Rust `String`. A reader that substitutes `U+FFFD`
    /// here silently changes the document.
    LoneSurrogate,
    /// Nesting exceeded the configured cap.
    ///
    /// A refusal rather than a grammar rule: unbounded recursive descent dies
    /// by stack overflow on a document an attacker writes in a few hundred
    /// bytes, and a stack overflow is not something a caller can catch.
    DepthLimitExceeded,
}

impl ErrorKind {
    /// A short, stable description of the kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnexpectedEof => "unexpected end of input",
            Self::UnexpectedChar => "unexpected character",
            Self::TrailingData => "trailing data after the top-level value",
            Self::InvalidNumber => "invalid number",
            Self::NumberOutOfRange => "number out of range",
            Self::ControlCharacterInString => "unescaped control character in string",
            Self::InvalidEscape => "invalid escape sequence",
            Self::LoneSurrogate => "lone surrogate in \\u escape",
            Self::DepthLimitExceeded => "nesting is deeper than the configured limit",
        }
    }
}

/// A parse failure, located in the source text.
///
/// `line` and `column` are 1-based and count **characters**, not bytes. A byte
/// offset reported as a column points into the middle of a character for anyone
/// whose data is not ASCII, which is most senders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// What went wrong.
    pub kind: ErrorKind,
    /// 1-based line.
    pub line: usize,
    /// 1-based column, in characters.
    pub column: usize,
    /// Byte offset from the start of the input.
    pub offset: usize,
    /// Extra context, when there is any worth carrying.
    pub detail: Option<String>,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, input: &str, offset: usize) -> Self {
        let (line, column) = locate(input, offset);
        Self {
            kind,
            line,
            column,
            offset,
            detail: None,
        }
    }

    pub(crate) fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Resolve a byte offset to a 1-based (line, column-in-characters).
///
/// Cold path only — it rescans the prefix so the parser's hot loop carries no
/// position bookkeeping at all.
fn locate(input: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(input.len());
    let mut line = 1;
    let mut column = 1;

    for (index, ch) in input.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at line {}, column {}",
            self.kind.as_str(),
            self.line,
            self.column
        )?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}
