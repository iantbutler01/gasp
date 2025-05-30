//! Pure SAX-style scanner over `Tokenizer` output.
//! Leaves *all* value construction to an external Builder/adapter.

use crate::json_tok::{Kind, Tokenizer};
use crate::json_types::JsonError;

/// Loss‑less structural / scalar events.
#[derive(Debug)]
pub enum Event<'a> {
    // structural
    StartObj,
    EndObj,
    StartArr,
    EndArr,

    // incremental string
    StrChunk(&'a str), // may emit many
    StrEnd(&'a str),

    // scalars
    NumberChunk(&'a str), // may emit many
    NumberEnd(&'a str),
    IdentChunk(&'a str), // may emit many
    IdentEnd(&'a str),   // null, true, false
}

/// Returned by [`Scanner::next_step`].
#[derive(Debug)]
pub enum Step<'a> {
    Event(Event<'a>), // produced exactly once per token *of interest*
    NeedMore,         // hit Eof → caller must `push()` more bytes & retry
    Error(JsonError), // unrecoverable tokenizer error
}

#[derive(Clone, Copy, Debug)]
enum Container {
    Obj,
    Arr,
}

/// Stateless *until* you feed it bytes via [`push`].
/// NOTE: Lifetime `'a` ties the returned `&str` slices to *your* buffer –
/// the caller owns the storage; scanner never allocates.
#[derive(Debug)]
pub struct Scanner {
    pub lexer: Tokenizer,
    buf: String, // full accumulated text so far
    stack: Vec<Container>,
    pub in_string: bool, // true ⇒ we are between StrChunk/StrEnd boundaries
}

impl Scanner {
    /// Create a fresh scanner over an *empty* buffer.
    pub fn new() -> Self {
        Self {
            lexer: Tokenizer::new(),
            buf: "".to_string(),
            stack: Vec::with_capacity(8),
            in_string: false,
        }
    }

    pub fn push(&mut self, chunk: String) {
        self.buf.push_str(&chunk); // **append** – never replace
        self.lexer.reset_if_done(); // resume at first new byte
    }

    /// Consume *one* meaningful token and turn it into a [`Step`].
    /// Caller may call repeatedly until it gets `NeedMore`.
    pub fn next_step(&mut self) -> Step {
        let tok = self.lexer.next_tok(&self.buf);

        let tok = match tok {
            Ok(t) => t,
            Err(e) => return Step::Error(e),
        };

        match tok.kind {
            Kind::Eof => Step::NeedMore,

            /*──────────── containers ────────────*/
            Kind::LBrace => {
                self.stack.push(Container::Obj);
                Step::Event(Event::StartObj)
            }
            Kind::RBrace => {
                self.stack.pop();
                Step::Event(Event::EndObj)
            }
            Kind::LBracket => {
                self.stack.push(Container::Arr);
                Step::Event(Event::StartArr)
            }
            Kind::RBracket => {
                self.stack.pop();
                Step::Event(Event::EndArr)
            }

            /*──────────── strings ───────────────*/
            Kind::StrChunk => {
                self.in_string = true;
                let slice = &self.buf[tok.start..tok.end];
                Step::Event(Event::StrChunk(slice))
            }

            Kind::StrEnd => {
                self.in_string = false;
                let slice = &self.buf[tok.start..tok.end];
                Step::Event(Event::StrEnd(slice))
            }
            /*──────────── idents (null, true, false, misc) ─────*/
            Kind::IdentChunk => {
                let slice = &self.buf[tok.start..tok.end];
                Step::Event(Event::IdentChunk(slice))
            }
            Kind::IdentEnd => {
                let slice = &self.buf[tok.start..tok.end];
                Step::Event(Event::IdentEnd(slice))
            }
            /*──────────── numbers & scalars ─────*/
            Kind::NumChunk => {
                let slice = &self.buf[tok.start..tok.end];
                Step::Event(Event::NumberChunk(slice))
            }
            Kind::NumEnd => {
                let slice = &self.buf[tok.start..tok.end];
                Step::Event(Event::NumberEnd(slice))
            }

            /*──────────── ignored tokens ────────*/
            Kind::Colon | Kind::Comma => {
                // Structural punctuation – not surfaced; builder relies on `stack`.
                self.next_step() // tail‑call: fetch next event recursively
            }
        }
    }
}
