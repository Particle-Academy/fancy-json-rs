//! The reader: strict RFC 8259, with a depth cap.
//!
//! Strict means strict. No comments, no trailing commas, no unquoted keys, no
//! single quotes, no `NaN`, no `Infinity`, no leading `+`, no leading zeros.
//! Every one of those is something a sender did not write, and this crate's
//! consumer executes what it reads.

use alloc::string::String;
use alloc::vec::Vec;

use crate::error::{Error, ErrorKind};
use crate::value::{Map, Number, Value};

/// The nesting depth accepted unless a caller says otherwise.
///
/// Deep enough for any document a person authors, shallow enough that
/// recursive descent cannot reach the end of the stack.
pub const DEFAULT_MAX_DEPTH: usize = 128;

/// Knobs for [`parse_with`].
#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    max_depth: usize,
    lossy_big_integers: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPTH,
            lossy_big_integers: false,
        }
    }
}

impl ParseOptions {
    /// The defaults: [`DEFAULT_MAX_DEPTH`], and exact integers only.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum nesting depth.
    #[must_use]
    pub const fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// The configured maximum nesting depth.
    #[must_use]
    pub const fn max_depth(self) -> usize {
        self.max_depth
    }

    /// Read an out-of-range integer as an `f64` instead of refusing it.
    ///
    /// Off by default. On, an integer literal too large for `u64`/`i64` becomes
    /// a [`Number::Float`] — which [`Value::as_u64`] and [`Value::as_i64`] then
    /// decline to return, so a lossy read can never be mistaken for an exact
    /// one. Turn it on to read somebody else's document; leave it off when the
    /// number is a quantity you will act on.
    ///
    /// [`Value::as_u64`]: crate::Value::as_u64
    /// [`Value::as_i64`]: crate::Value::as_i64
    #[must_use]
    pub const fn with_lossy_big_integers(mut self, lossy: bool) -> Self {
        self.lossy_big_integers = lossy;
        self
    }

    /// Whether out-of-range integers are read lossily.
    #[must_use]
    pub const fn lossy_big_integers(self) -> bool {
        self.lossy_big_integers
    }
}

/// Parse a JSON document with the default options.
///
/// # Errors
///
/// Returns an [`Error`] locating the first fault. Anything other than exactly
/// one well-formed value, optionally surrounded by whitespace, is a fault.
pub fn parse(input: &str) -> Result<Value, Error> {
    parse_with(input, ParseOptions::new())
}

/// Parse a JSON document.
///
/// # Errors
///
/// As [`parse`], subject to `options`.
pub fn parse_with(input: &str, options: ParseOptions) -> Result<Value, Error> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        input,
        at: 0,
        depth: 0,
        options,
    };
    parser.skip_whitespace();
    let value = parser.value()?;
    parser.skip_whitespace();
    if parser.at < parser.bytes.len() {
        return Err(parser.error(ErrorKind::TrailingData));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    input: &'a str,
    at: usize,
    depth: usize,
    options: ParseOptions,
}

impl Parser<'_> {
    fn error(&self, kind: ErrorKind) -> Error {
        Error::new(kind, self.input, self.at)
    }

    fn error_at(&self, kind: ErrorKind, at: usize) -> Error {
        Error::new(kind, self.input, at)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// The four characters JSON calls whitespace, and only those.
    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), Error> {
        match self.peek() {
            Some(found) if found == byte => {
                self.at += 1;
                Ok(())
            }
            Some(_) => Err(self.error(ErrorKind::UnexpectedChar)),
            None => Err(self.error(ErrorKind::UnexpectedEof)),
        }
    }

    fn value(&mut self) -> Result<Value, Error> {
        match self
            .peek()
            .ok_or_else(|| self.error(ErrorKind::UnexpectedEof))?
        {
            b'n' => self.literal("null", Value::Null),
            b't' => self.literal("true", Value::Bool(true)),
            b'f' => self.literal("false", Value::Bool(false)),
            b'"' => Ok(Value::String(self.string()?)),
            b'[' => self.array(),
            b'{' => self.object(),
            b'-' | b'0'..=b'9' => self.number(),
            // Everything else: `NaN`, `Infinity`, `undefined`, `'`, `+`, `.`,
            // a bare identifier, a comment's `/`. All one kind, because they
            // are all the same mistake — this is not JavaScript.
            _ => Err(self.error(ErrorKind::UnexpectedChar)),
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, Error> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error(ErrorKind::UnexpectedChar))
        }
    }

    /// Enter a container, refusing to go deeper than the cap.
    fn descend(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > self.options.max_depth {
            return Err(self
                .error(ErrorKind::DepthLimitExceeded)
                .with_detail("raise ParseOptions::with_max_depth only for a document you trust"));
        }
        Ok(())
    }

    fn array(&mut self) -> Result<Value, Error> {
        self.descend()?;
        self.at += 1; // '['
        let mut items: Vec<Value> = Vec::new();

        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            self.depth -= 1;
            return Ok(Value::Array(items));
        }

        loop {
            self.skip_whitespace();
            items.push(self.value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    self.depth -= 1;
                    return Ok(Value::Array(items));
                }
                Some(_) => return Err(self.error(ErrorKind::UnexpectedChar)),
                None => return Err(self.error(ErrorKind::UnexpectedEof)),
            }
        }
    }

    fn object(&mut self) -> Result<Value, Error> {
        self.descend()?;
        self.at += 1; // '{'
        let mut map = Map::new();

        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            self.depth -= 1;
            return Ok(Value::Object(map));
        }

        loop {
            self.skip_whitespace();
            // A key is a STRING. Not a bare identifier, not a number.
            if self.peek() != Some(b'"') {
                return Err(match self.peek() {
                    Some(_) => self.error(ErrorKind::UnexpectedChar),
                    None => self.error(ErrorKind::UnexpectedEof),
                });
            }
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let value = self.value()?;
            map.insert(key, value);

            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    self.depth -= 1;
                    return Ok(Value::Object(map));
                }
                Some(_) => return Err(self.error(ErrorKind::UnexpectedChar)),
                None => return Err(self.error(ErrorKind::UnexpectedEof)),
            }
        }
    }

    fn string(&mut self) -> Result<String, Error> {
        self.at += 1; // opening quote
        let mut out = String::new();
        let mut run_start = self.at;

        loop {
            let Some(byte) = self.peek() else {
                return Err(self.error(ErrorKind::UnexpectedEof));
            };

            match byte {
                b'"' => {
                    out.push_str(&self.input[run_start..self.at]);
                    self.at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    out.push_str(&self.input[run_start..self.at]);
                    self.at += 1;
                    self.escape(&mut out)?;
                    run_start = self.at;
                }
                // U+0000..=U+001F must be escaped. Not 0x7F: JSON's control
                // range stops at 0x1F, and silently rejecting DEL would refuse
                // documents the grammar allows.
                0x00..=0x1F => return Err(self.error(ErrorKind::ControlCharacterInString)),
                // Any other byte, including every UTF-8 continuation byte, is
                // copied verbatim by the run above. The input is a `&str`, so
                // it is already valid UTF-8 and the slice boundaries land on
                // character boundaries by construction.
                _ => self.at += 1,
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), Error> {
        let start = self.at - 1;
        let Some(byte) = self.peek() else {
            return Err(self.error(ErrorKind::UnexpectedEof));
        };
        self.at += 1;

        let ch = match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => return self.unicode_escape(out, start),
            _ => return Err(self.error_at(ErrorKind::InvalidEscape, start)),
        };
        out.push(ch);
        Ok(())
    }

    fn unicode_escape(&mut self, out: &mut String, start: usize) -> Result<(), Error> {
        let first = self.hex4(start)?;

        // Not a surrogate: a complete scalar on its own.
        if !(0xD800..=0xDFFF).contains(&first) {
            let ch = char::from_u32(first)
                .ok_or_else(|| self.error_at(ErrorKind::InvalidEscape, start))?;
            out.push(ch);
            return Ok(());
        }

        // A low surrogate cannot lead. There is no partner to look for.
        if first >= 0xDC00 {
            return Err(self.error_at(ErrorKind::LoneSurrogate, start));
        }

        // A high surrogate must be followed by `\uXXXX` holding a low one.
        if self.peek() != Some(b'\\') || self.bytes.get(self.at + 1) != Some(&b'u') {
            return Err(self.error_at(ErrorKind::LoneSurrogate, start));
        }
        self.at += 2;
        let second = self.hex4(start)?;
        if !(0xDC00..=0xDFFF).contains(&second) {
            return Err(self.error_at(ErrorKind::LoneSurrogate, start));
        }

        let combined = 0x1_0000 + ((first - 0xD800) << 10) + (second - 0xDC00);
        let ch = char::from_u32(combined)
            .ok_or_else(|| self.error_at(ErrorKind::LoneSurrogate, start))?;
        out.push(ch);
        Ok(())
    }

    fn hex4(&mut self, start: usize) -> Result<u32, Error> {
        let end = self.at + 4;
        if end > self.bytes.len() {
            return Err(self.error_at(ErrorKind::InvalidEscape, start));
        }
        let mut value: u32 = 0;
        for &byte in &self.bytes[self.at..end] {
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'f' => u32::from(byte - b'a') + 10,
                b'A'..=b'F' => u32::from(byte - b'A') + 10,
                _ => return Err(self.error_at(ErrorKind::InvalidEscape, start)),
            };
            value = value * 16 + digit;
        }
        self.at = end;
        Ok(value)
    }

    /// `-? (0 | [1-9][0-9]*) (\.[0-9]+)? ([eE][+-]?[0-9]+)?`
    ///
    /// Written out by hand rather than delegated, because the grammar's whole
    /// job here is to refuse the spellings other languages accept.
    fn number(&mut self) -> Result<Value, Error> {
        let start = self.at;

        let negative = self.peek() == Some(b'-');
        if negative {
            self.at += 1;
        }

        // The integer part. A leading zero stands alone — `01` is not a
        // number, it is `0` followed by trailing data, and saying so is what
        // stops a zero-padded id being read as a different number.
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return Err(self.error_at(ErrorKind::InvalidNumber, start)),
        }

        let integer_end = self.at;
        let mut is_float = false;

        if self.peek() == Some(b'.') {
            self.at += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error_at(ErrorKind::InvalidNumber, start));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
            is_float = true;
        }

        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error_at(ErrorKind::InvalidNumber, start));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
            is_float = true;
        }

        let text = &self.input[start..self.at];

        if !is_float {
            let digits = &self.input[if negative { start + 1 } else { start }..integer_end];
            if let Some(number) = integer_from(digits, negative) {
                return Ok(Value::Number(number));
            }
            if !self.options.lossy_big_integers {
                return Err(self
                    .error_at(ErrorKind::NumberOutOfRange, start)
                    .with_detail("an integer literal outside u64/i64 would lose precision as a float; set ParseOptions::with_lossy_big_integers to accept that"));
            }
        }

        // `from_str` on a slice the grammar above already validated. It is
        // shortest-round-trip correct and lives in `core`, so this stays
        // no_std.
        let float: f64 = text
            .parse()
            .map_err(|_| self.error_at(ErrorKind::InvalidNumber, start))?;

        Number::from_f64(float).map(Value::Number).ok_or_else(|| {
            self.error_at(ErrorKind::NumberOutOfRange, start)
                .with_detail("the literal overflows to infinity, which JSON cannot represent")
        })
    }
}

/// Build an exact integer from its digits, or `None` when it does not fit.
fn integer_from(digits: &str, negative: bool) -> Option<Number> {
    let magnitude: u64 = digits.parse().ok()?;

    if !negative {
        return Some(Number::PosInt(magnitude));
    }

    // `-0` is the integer zero. Keeping a separate `NegInt(0)` would give one
    // number two representations, and equality would then depend on how it was
    // spelled.
    if magnitude == 0 {
        return Some(Number::PosInt(0));
    }

    // i64::MIN's magnitude is one past i64::MAX, so the check is on the
    // magnitude and the negation happens in i128.
    #[allow(
        clippy::cast_sign_loss,
        reason = "i64::MAX is positive; the cast is to compare magnitudes"
    )]
    let limit = (i64::MAX as u64) + 1;
    if magnitude > limit {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the guard above bounds the magnitude at i64::MIN's, so the negation fits"
    )]
    let value = -(i128::from(magnitude)) as i64;
    Some(Number::NegInt(value))
}
