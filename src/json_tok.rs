//! `json_tok` – tolerant, state-carrying tokenizer that survives `push()`.
//
//  It never produces &str slices that outlive `&self.src`; instead every
//  token carries a `start..end` byte range back into the parser's buffer,
//  so the buffer can grow and re-allocate safely.

use crate::json_types::JsonError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Comma,
    StrChunk,
    StrEnd,
    NumChunk,
    NumEnd,
    IdentChunk,
    IdentEnd,
    Eof,
}

#[derive(Debug, Clone, Copy)]
pub struct Tok {
    pub kind: Kind,
    pub start: usize, // byte offsets in the *current* buffer
    pub end: usize,
}

#[derive(Debug, Clone, Copy)]
enum LState {
    Start,
    InIdent {
        start: usize, // first byte of the current chunk
    },
    InString {
        quote: u8,    // opening quote byte (b'\"' or b'\\'')
        start: usize, // chunk-start offset
        escape: bool, // last byte was '\\'
        u_digits: u8, // >0 while reading \\uXXXX hex digits
    },
    InNumber {
        start: usize, // first digit of this number
        seen_dot: bool,
        seen_exp: bool,
    },
    InLineComment,
    InBlockComment {
        star: bool,
    }, // saw ‘*’ last byte?
    Done,
}
#[inline]
fn peek(bytes: &[u8], i: usize) -> Option<u8> {
    bytes.get(i).copied()
}
#[inline]
fn single(tok: &mut Tokenizer, k: Kind) -> Result<Tok, JsonError> {
    let start = tok.pos;
    tok.pos += 1;
    Ok(Tok {
        kind: k,
        start,
        end: start + 1,
    })
}
#[inline]
fn eof(pos: usize) -> Tok {
    Tok {
        kind: Kind::Eof,
        start: pos,
        end: pos,
    }
}

#[derive(Debug, Clone)]
pub struct Tokenizer {
    state: LState,
    pos: usize, // absolute byte position
}

impl Tokenizer {
    pub fn new() -> Self {
        Self {
            state: LState::Start,
            pos: 0,
        }
    }

    /// Re-initialise after `push()` with the same logical state.
    pub fn reset_if_done(&mut self) {
        if matches!(self.state, LState::Done) {
            self.state = LState::Start;
        }
    }

    /// Return next token *or* `Tok {kind:Eof}` if no progress possible.
    /// `src` is the *entire* current buffer.
    pub fn next_tok(&mut self, src: &str) -> Result<Tok, JsonError> {
        use LState::*;
        let bytes = src.as_bytes();

        loop {
            match self.state {
                Start => {
                    if self.pos >= bytes.len() {
                        self.state = Done;
                        return Ok(eof(self.pos));
                    }
                    match bytes[self.pos] {
                        b'{' => return single(self, Kind::LBrace),
                        b'}' => return single(self, Kind::RBrace),
                        b'[' => return single(self, Kind::LBracket),
                        b']' => return single(self, Kind::RBracket),
                        b':' => return single(self, Kind::Colon),
                        b',' => return single(self, Kind::Comma),
                        b'"' | b'\'' => {
                            // ── need one byte of look-ahead; if we don’t have it yet, ask for more
                            if peek(bytes, self.pos + 1).is_none() {
                                return Ok(eof(self.pos)); // ←  tell the caller “need more data”
                            }

                            // If the next non-consumed byte is ':'
                            if matches!(peek(bytes, self.pos + 1), Some(b':')) {
                                // skip this quote – it’s spur­ious (came after an un-quoted key)
                                self.pos += 1;
                                continue;
                            }

                            // otherwise this really *is* the start of a string
                            self.pos += 1; // consume the quote
                            self.state = LState::InString {
                                quote: bytes[self.pos - 1],
                                start: self.pos,
                                escape: false,
                                u_digits: 0,
                            };
                            continue;
                        }
                        b'0'..=b'9' | b'-' | b'.' => {
                            self.state = InNumber {
                                start: self.pos,
                                seen_dot: false,
                                seen_exp: false,
                            };
                        }
                        b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                            self.state = InIdent { start: self.pos }
                        }
                        b'/' if peek(bytes, self.pos + 1) == Some(b'/') => {
                            self.state = InLineComment;
                            self.pos += 2;
                        }
                        b'/' if peek(bytes, self.pos + 1) == Some(b'*') => {
                            self.state = InBlockComment { star: false };
                            self.pos += 2;
                        }
                        b if b.is_ascii_whitespace() => {
                            self.pos += 1;
                        }
                        b => return Err(JsonError::UnexpectedChar(b as char)),
                    }
                }

                LState::InString {
                    quote,
                    ref mut start,
                    ref mut escape,
                    ref mut u_digits,
                } => {
                    // keep scanning the current buffer
                    while self.pos < bytes.len() {
                        let b = bytes[self.pos];

                        /* ───── currently inside an escape sequence ───── */
                        if *escape {
                            if *u_digits > 0 {
                                // consuming one hex digit of a \uXXXX escape
                                *u_digits -= 1;
                                if *u_digits == 0 {
                                    *escape = false; // finished the four digits
                                }
                            } else if b == b'u' {
                                // saw the 'u' of a \uXXXX sequence – expect 4 hex digits next
                                *u_digits = 4;
                            } else {
                                // simple two-character escape (“\n”, “\t”, “\\”, “\"”, …)
                                *escape = false;
                            }
                            self.pos += 1;
                            continue; // stay inside the string
                        }

                        /* ───── backslash starts an escape ───── */
                        if b == b'\\' {
                            *escape = true;
                            self.pos += 1;
                            continue;
                        }

                        /* ───── closing quote ───── */
                        if b == quote {
                            let tok = Tok {
                                kind: Kind::StrEnd,
                                start: *start,
                                end: self.pos,
                            };
                            self.pos += 1; // consume the quote itself
                            self.state = LState::Start; // leave string mode
                            return Ok(tok);
                        }

                        /* ordinary UTF-8 byte inside the string */
                        self.pos += 1;
                    }

                    /* ───── reached the end of this buffer ───── */
                    if *start == self.pos {
                        // we didn't actually consume anything new
                        return Ok(eof(self.pos));
                    }

                    // emit a StrChunk for the substring we just scanned
                    let tok = Tok {
                        kind: Kind::StrChunk,
                        start: *start,
                        end: self.pos,
                    };
                    *start = self.pos; // the next chunk (next buffer) will start here
                    return Ok(tok);
                }

                InNumber {
                    start,
                    ref mut seen_dot,
                    ref mut seen_exp,
                } => {
                    let mut progressed = false;
                    let mut at_start = self.pos == start; // ← helper
                    let mut in_exp = *seen_exp &&            // true once we've read 'e' or 'E'
                       peek(bytes, self.pos - 1).map(|b| b == b'e' || b == b'E')
                       .unwrap_or(false);

                    while self.pos < bytes.len() {
                        match bytes[self.pos] {
                            // ---------- sign handling ----------
                            b'+' | b'-' if at_start || in_exp => {
                                self.pos += 1;
                                progressed = true;
                                at_start = false; // only the very first time
                                in_exp = false; // one sign per exponent
                            }

                            // ---------- normal digits ----------
                            b'0'..=b'9' => {
                                self.pos += 1;
                                progressed = true;
                                at_start = false;
                                in_exp = false;
                            }

                            // ---------- dot ----------
                            b'.' if !*seen_dot && !*seen_exp => {
                                *seen_dot = true;
                                self.pos += 1;
                                progressed = true;
                                at_start = false;
                            }

                            // ---------- exponent ----------
                            b'e' | b'E' if !*seen_exp => {
                                *seen_exp = true;
                                self.pos += 1;
                                progressed = true;
                                at_start = false;
                                in_exp = true; // expect optional sign / digits next
                            }

                            _ => break, // delimiter
                        }
                    }

                    //----------------------------------------
                    // Guarantee forward progress or bail out
                    //----------------------------------------
                    if !progressed && self.pos == bytes.len() {
                        // we’re stuck at the very end of the slice: ask caller for more bytes
                        return Ok(eof(self.pos));
                    }

                    if self.pos == bytes.len() {
                        // slice ended mid-number → NumChunk

                        self.state = LState::InNumber {
                            start: self.pos, // <- new start!
                            seen_dot: *seen_dot,
                            seen_exp: *seen_exp,
                        };
                        return Ok(Tok {
                            kind: Kind::NumChunk,
                            start,
                            end: self.pos,
                        });
                    }

                    // delimiter reached → NumEnd
                    let end = self.pos;
                    self.state = Start;
                    return Ok(Tok {
                        kind: Kind::NumEnd,
                        start,
                        end,
                    });
                }

                InIdent { start } => {
                    let mut progressed = false;

                    // ── absorb identifier bytes until we hit a delimiter ──
                    while self.pos < bytes.len() {
                        match bytes[self.pos] {
                            // structural punctuation – always terminates an identifier
                            b'{' | b'}' | b'[' | b']' | b',' | b':' |
                            b'"' | b'\'' |                // quoted string begins
                            b'/' |                        // start of a comment
                            b'\n' | b'\r'                 // ← keep NEW-LINES as delimiters
                                => break,
                            _ => {
                                self.pos += 1;
                                progressed = true;
                            }
                        }
                    }

                    /* need more bytes? (we're at buffer edge and took no step forward) */
                    if !progressed && self.pos == bytes.len() {
                        return Ok(eof(self.pos));
                    }

                    /* buffer finished mid-identifier → emit IdentChunk */
                    if self.pos == bytes.len() {
                        self.state = LState::InIdent { start: self.pos }; // next chunk begins here
                        return Ok(Tok {
                            kind: Kind::IdentChunk,
                            start,
                            end: self.pos,
                        });
                    }

                    /* delimiter reached (but not consumed) → IdentEnd */
                    let end = self.pos; // delimiter still un-consumed
                    self.state = LState::Start; // let outer loop handle the delimiter next
                    return Ok(Tok {
                        kind: Kind::IdentEnd,
                        start,
                        end,
                    });
                }

                InLineComment => {
                    while let Some(b) = peek(bytes, self.pos) {
                        self.pos += 1;
                        if b == b'\n' {
                            break;
                        }
                    }
                    self.state = Start;
                }

                InBlockComment { ref mut star } => {
                    while let Some(b) = peek(bytes, self.pos) {
                        self.pos += 1;
                        *star = b == b'*';
                        if *star && peek(bytes, self.pos) == Some(b'/') {
                            self.pos += 1;
                            self.state = Start;
                            break;
                        }
                    }
                    if self.pos >= bytes.len() {
                        return Err(JsonError::UnexpectedEof);
                    }
                }

                Done => return Ok(eof(self.pos)),
            }
        }
    }
}
