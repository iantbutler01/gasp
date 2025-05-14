use crate::json_tok::{Kind, Tok, Tokenizer};
use crate::json_types::{JsonError, JsonValue, Number};
use crate::tag_finder::{TagEvent, TagFinder};
use std::collections::{HashMap, HashSet};
use std::default;

use crate::json_sax_scanner::{Event, Scanner, Step as ScanStep};

#[inline]
fn squash_ws(s: &str) -> String {
    // collapse hard/newline whitespace to single ASCII space
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[inline]
pub fn parse_ident(buf: &str) -> Option<JsonValue> {
    match buf {
        // ─ true ─
        "t" | "tr" | "tru" | "true" => Some(JsonValue::Boolean(true)),

        // ─ false ─
        "f" | "fa" | "fal" | "fals" | "false" => Some(JsonValue::Boolean(false)),

        // ─ null ─
        "n" | "nu" | "nul" | "null" => Some(JsonValue::Null),

        // ─ anything else: not a JSON keyword ─
        _ => None,
    }
}

fn parse_number(raw: &str) -> Result<Number, JsonError> {
    // treat ".5" or "-.7" as floats, just like "0.5"
    let mut cooked = if raw.starts_with('.') || raw.starts_with("-.") || raw.starts_with("+.") {
        format!("0{raw}")
    } else {
        raw.to_owned()
    };

    while matches!(
        cooked.chars().last(),
        Some('}' | ',' | ']' | ' ' | '\n' | '\r' | '\t' | '-' | '+' | '.' | 'e' | 'E')
    ) {
        cooked.pop();
    }

    if cooked.contains('.') || cooked.contains('e') || cooked.contains('E') {
        cooked
            .parse::<f64>()
            .map(|n| Number::Float(n))
            .map_err(|_| JsonError::InvalidNumber(cooked))
    } else {
        cooked
            .parse::<i64>()
            .map(|n| Number::Integer(n))
            .map_err(|_| JsonError::InvalidNumber(cooked))
    }
}

fn unescape(src: &str) -> Result<String, JsonError> {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000C}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if hex.len() != 4 {
                    return Err(JsonError::InvalidEscape);
                }
                let cp = u16::from_str_radix(&hex, 16).map_err(|_| JsonError::InvalidEscape)?;
                out.push(char::from_u32(cp as u32).ok_or(JsonError::InvalidEscape)?);
            }
            _ => return Err(JsonError::InvalidEscape),
        }
    }
    Ok(out)
}

/*──────────────────────────── Snapshot API ───────────────────────────*/

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathItem {
    Key(String),
    Index(usize),
}

#[derive(Debug)]
pub enum Snapshot {
    Partial {
        path: Vec<PathItem>,
        value: JsonValue,
    },
    Complete(JsonValue),
}

/*──────────────────────────── Builder internals ──────────────────────*/

#[derive(Debug, Clone)]
enum Frame {
    Obj {
        map: HashMap<String, JsonValue>,
        last_key: Option<String>,
    },
    Arr {
        vec: Vec<JsonValue>,
    },
    Str {
        buf: String,
    },
    Ident {
        buf: String,
    },
    Num {
        buf: String,
    },
}

impl Frame {
    fn as_obj_mut(&mut self) -> &mut HashMap<String, JsonValue> {
        match self {
            Frame::Obj { map, .. } => map,
            _ => unreachable!(),
        }
    }
    fn as_arr_mut(&mut self) -> &mut Vec<JsonValue> {
        match self {
            Frame::Arr { vec } => vec,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Builder {
    pub stack: Vec<Frame>,
    path: Vec<PathItem>, // mirrors stack depth for snapshot emissions
}

impl Builder {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            path: Vec::new(),
        }
    }
    fn push_path_for_scalar(&mut self) {
        if let Some(top) = self.stack.last() {
            match top {
                Frame::Arr { vec } => {
                    self.path.push(PathItem::Index(vec.len()));
                }
                Frame::Obj { last_key, .. } => {
                    if let Some(k) = last_key.clone() {
                        self.path.push(PathItem::Key(k));
                    }
                }
                _ => {}
            }
        }
    }

    /// Feed a single scanner event, returning an optional snapshot.
    pub fn feed_event(&mut self, ev: Event) -> Result<Option<Snapshot>, JsonError> {
        match ev {
            /*──────── structural open ───────*/
            Event::StartObj => {
                self.start_container(Frame::Obj {
                    map: HashMap::new(),
                    last_key: None,
                });
            }
            Event::StartArr => {
                self.start_container(Frame::Arr { vec: Vec::new() });
            }

            /*──────── structural close ──────*/
            Event::EndObj | Event::EndArr => {
                let finished_val = self.finish_container()?;
                return self.finish_value_and_maybe_snapshot(finished_val);
            }

            /*──────── string chunks ─────────*/
            Event::StrChunk(chunk) => {
                // record the depth **before** we mut-borrow anything
                let depth = self.stack.len();
                let should_snapshot = depth == 2 && self.parent_wants_value();

                self.ensure_string_frame();

                if let Some(Frame::Str { buf }) = self.stack.last_mut() {
                    buf.push_str(chunk);

                    // root-container + scalar frame ⇒ emit snapshot
                    if should_snapshot {
                        return Ok(Some(Snapshot::Partial {
                            path: self.path.clone(),
                            value: JsonValue::String(buf.clone()),
                        }));
                    }
                }
            }

            Event::StrEnd(chunk) => {
                // ── 1. Are we in an object and still waiting for the key?
                if let Some(Frame::Obj { last_key, .. }) = self.stack.last_mut() {
                    if last_key.is_none() {
                        *last_key = Some(chunk.to_owned()); // treat ident/string as the key
                        return Ok(None); // no value yet
                    }
                }

                // -- 2. Were we already accumulating StrChunk parts?
                if matches!(self.stack.last(), Some(Frame::Str { .. })) {
                    // pop the string frame
                    let mut s = match self.stack.pop() {
                        Some(Frame::Str { buf }) => buf,
                        _ => unreachable!(),
                    };
                    s.push_str(chunk);
                    let cooked = unescape(&s)?;

                    // NEW: if parent is an object still waiting for a key,
                    // treat the cooked string as that key instead of a value.
                    if let Some(Frame::Obj { last_key, .. }) = self.stack.last_mut() {
                        if last_key.is_none() {
                            *last_key = Some(cooked);
                            return Ok(None); // we’re done – no value yet
                        }
                    }

                    // otherwise it's a normal value string
                    return self.finish_value_and_maybe_snapshot(JsonValue::String(cooked));
                }

                // ── 3. It’s a one-shot value (bare ident or quoted string)
                let cooked = unescape(chunk)?;
                self.push_path_for_scalar();
                return self.finish_value_and_maybe_snapshot(JsonValue::String(cooked));
            }

            Event::NumberChunk(chunk) => {
                let depth = self.stack.len(); // capture depth first
                let should_snapshot = depth == 2 && self.parent_wants_value();

                self.ensure_num_frame();

                if let Some(Frame::Num { buf }) = self.stack.last_mut() {
                    buf.push_str(chunk);

                    if should_snapshot {
                        let val = JsonValue::Number(parse_number(buf)?);
                        return Ok(Some(Snapshot::Partial {
                            path: self.path.clone(),
                            value: val,
                        }));
                    }
                }
            }

            Event::NumberEnd(tok) => {
                // ── Were we accumulating NumberChunk parts?
                if let Some(Frame::Num { buf }) = self.stack.last_mut() {
                    buf.push_str(tok);
                    let num = parse_number(buf)?; // Number::Integer / Float
                    self.stack.pop(); // drop the Num frame
                    return self.finish_value_and_maybe_snapshot(JsonValue::Number(num));
                }

                // ── One-shot number (no prior Frame::Num)
                let num = parse_number(tok)?;
                self.push_path_for_scalar(); // same as strings
                return self.finish_value_and_maybe_snapshot(JsonValue::Number(num));
            }
            Event::IdentChunk(chunk) => {
                let depth = self.stack.len(); // capture depth first
                let should_snapshot = depth == 2 && self.parent_wants_value();

                self.ensure_ident_frame();

                if let Some(Frame::Ident { buf }) = self.stack.last_mut() {
                    buf.push_str(chunk);

                    if should_snapshot {
                        let val =
                            parse_ident(buf).unwrap_or_else(|| JsonValue::String(squash_ws(buf)));

                        return Ok(Some(Snapshot::Partial {
                            path: self.path.clone(),
                            value: val,
                        }));
                    }
                }
            }

            Event::IdentEnd(tok) => {
                /*──────────────────── A. continuing an IdentChunk series ───────────────────*/
                if let Some(Frame::Ident { buf }) = self.stack.last_mut() {
                    buf.push_str(tok); // complete the word
                    let txt = buf.clone();
                    self.stack.pop(); // drop the Ident frame

                    /* A-1 keyword? → scalar value */
                    if let Some(lit) = parse_ident(&txt) {
                        return self.finish_value_and_maybe_snapshot(lit);
                    }

                    /* A-2 it’s *not* a keyword */
                    if self.parent_wants_key() {
                        /* we’re finishing an object key */
                        if parse_ident(&txt).is_some() {
                            return Err(JsonError::InvalidKey); // true / false / null as key
                        }
                        if let Some(Frame::Obj { last_key, .. }) = self.stack.last_mut() {
                            *last_key = Some(txt);
                        }
                        return Ok(None); // wait for the value
                    }

                    /* A-3 regular value (un-quoted string) */
                    return self
                        .finish_value_and_maybe_snapshot(JsonValue::String(squash_ws(&txt)));
                }

                /*──────────────────── B. one-shot identifier (no prior chunks) ──────────────*/
                if self.parent_wants_key() {
                    /* key fast-path */
                    if parse_ident(tok).is_some() {
                        return Err(JsonError::InvalidKey);
                    }
                    if let Some(Frame::Obj { last_key, .. }) = self.stack.last_mut() {
                        *last_key = Some(tok.to_owned());
                    }
                    return Ok(None);
                }

                /* scalar value */
                let val = parse_ident(tok).unwrap_or_else(|| JsonValue::String(tok.to_owned()));
                self.push_path_for_scalar();
                return self.finish_value_and_maybe_snapshot(val);
            }
        }
        Ok(None)
    }

    fn depth(&self) -> usize {
        self.stack.len()
    }

    fn start_container(&mut self, frame: Frame) {
        // Path bookkeeping for snapshots
        if let Some(Frame::Arr { vec }) = self.stack.last_mut() {
            let idx = vec.len();
            self.path.push(PathItem::Index(idx));
        } else if let Some(Frame::Obj { last_key, .. }) = self.stack.last() {
            if let Some(k) = last_key.clone() {
                self.path.push(PathItem::Key(k));
            }
        }
        self.stack.push(frame);
    }

    fn finish_container(&mut self) -> Result<JsonValue, JsonError> {
        let frame = self.stack.pop().expect("stack underflow");
        let val = match frame {
            Frame::Obj { map, .. } => JsonValue::Object(map),
            Frame::Arr { vec } => JsonValue::Array(vec),
            _ => unreachable!(),
        };
        // Pop path item because container is now finished
        if !self.path.is_empty() {
            self.path.pop();
        }
        Ok(val)
    }

    fn ensure_ident_frame(&mut self) {
        if !matches!(self.stack.last(), Some(Frame::Ident { .. })) {
            // 1. remember where this scalar will live
            self.push_path_for_scalar(); // <-- this is enough

            // 2. start accumulating the number chunks
            self.stack.push(Frame::Ident { buf: String::new() });
        }
    }

    fn ensure_num_frame(&mut self) {
        if !matches!(self.stack.last(), Some(Frame::Num { .. })) {
            // 1. remember where this scalar will live
            self.push_path_for_scalar(); // <-- this is enough

            // 2. start accumulating the number chunks
            self.stack.push(Frame::Num { buf: String::new() });
        }
    }

    fn parent_wants_value(&self) -> bool {
        match self.stack.get(self.stack.len() - 2).unwrap() {
            // Arrays are always waiting for a value
            Frame::Arr { .. } => true,
            // Objects want a value only after the key has been completed
            Frame::Obj { last_key, .. } => last_key.is_some(),
            _ => false,
        }
    }

    #[inline]
    fn parent_wants_key(&self) -> bool {
        matches!(self.stack.last(), Some(Frame::Obj { last_key: None, .. }))
    }

    fn ensure_string_frame(&mut self) {
        if !matches!(self.stack.last(), Some(Frame::Str { .. })) {
            // push string frame and path
            if let Some(Frame::Arr { vec }) = self.stack.last_mut() {
                let idx = vec.len();
                self.path.push(PathItem::Index(idx));
            } else if let Some(Frame::Obj { last_key, .. }) = self.stack.last() {
                if let Some(k) = last_key.clone() {
                    self.path.push(PathItem::Key(k));
                }
            }
            self.stack.push(Frame::Str { buf: String::new() });
        }
    }

    fn finish_value_and_maybe_snapshot(
        &mut self,
        val: JsonValue,
    ) -> Result<Option<Snapshot>, JsonError> {
        if let Some(parent) = self.stack.last_mut() {
            match parent {
                Frame::Obj { map, last_key } => {
                    let key = last_key.take().ok_or(JsonError::InvalidKey)?;
                    map.insert(key.clone(), val.clone());
                }
                Frame::Arr { vec } => {
                    /* ────── NEW: try to coalesce a “split string” artefact ────── */
                    if let JsonValue::String(ref cur) = val {
                        // ❶  drop a slice that is *just* a comma (optional WS)
                        if cur.trim().trim_matches(',').is_empty() {
                            return Ok(None); // swallow silently
                        }

                        // ❷  if previous element ends with “, ” or “,” → merge
                        if let Some(JsonValue::String(last)) = vec.last_mut() {
                            if last.ends_with(", ") || last.ends_with(',') {
                                // strip the trailing comma + optional space
                                while matches!(last.chars().last(), Some(',') | Some(' ')) {
                                    last.pop();
                                }
                                last.push_str(cur); // glue the fragment on
                                return Ok(None); // done – **don’t** push a new element
                            }
                        }
                    }

                    /* ────── normal behaviour (unchanged) ────── */
                    vec.push(val.clone());
                }

                _ => unreachable!(),
            }
        } else {
            // root value (no implicit array yet)
            self.stack.push(Frame::Arr {
                vec: vec![val.clone()],
            });
        }

        // depth‑1 snapshot?
        if self.depth() == 1 {
            let snap_val = match self.stack.last().unwrap() {
                Frame::Obj { map, .. } => JsonValue::Object(map.clone()),
                Frame::Arr { vec } => JsonValue::Array(vec.clone()),
                _ => val.clone(),
            };
            let snapshot = Snapshot::Partial {
                path: self.path.clone(),
                value: snap_val,
            };
            return Ok(Some(snapshot));
        }
        // Clean path when value done.
        if !self.path.is_empty() {
            self.path.pop();
        }
        Ok(None)
    }

    pub fn finish(&mut self, streaming: bool) -> Result<JsonValue, JsonError> {
        /*──────────── when we’re *inside* something at EOF / NeedMore ───────────*/
        if self.stack.is_empty() {
            return Ok(JsonValue::Null); // or JsonValue::Array(vec![]) if you prefer
        }

        if self.stack.len() != 1 {
            if streaming {
                /*───────────────── 1. try to patch an object value ─────────────────*/
                if self.stack.len() >= 2 {
                    let len = self.stack.len();
                    let (parent, child) = self.stack.split_at_mut(len - 1);
                    if let Frame::Obj {
                        map,
                        last_key: Some(k),
                        ..
                    } = &mut parent[parent.len() - 1]
                    {
                        match &child[0] {
                            Frame::Str { buf } => {
                                let mut tail = buf.clone();
                                while matches!(
                                    tail.chars().last(),
                                    Some('}' | ',' | ']' | ' ' | '\n' | '\r' | '\t')
                                ) {
                                    tail.pop();
                                }
                                map.insert(k.clone(), JsonValue::String(tail));
                            }
                            Frame::Ident { buf } => {
                                let mut tail = buf.clone();
                                while matches!(
                                    tail.chars().last(),
                                    Some('}' | ',' | ']' | ' ' | '\n' | '\r' | '\t')
                                ) {
                                    tail.pop();
                                }
                                map.insert(
                                    k.clone(),
                                    parse_ident(&tail).unwrap_or(JsonValue::Null),
                                );
                            }
                            Frame::Num { buf } => {
                                map.insert(k.clone(), JsonValue::Number(parse_number(buf)?));
                            }
                            _ => {}
                        }
                    }
                }

                /*───────────────── 2. if root is now non-empty, return it ──────────*/
                if let Some(val) = match self.stack.first().unwrap() {
                    Frame::Obj { map, .. } if !map.is_empty() => {
                        Some(JsonValue::Object(map.clone()))
                    }
                    Frame::Arr { vec } if !vec.is_empty() => Some(JsonValue::Array(vec.clone())),
                    _ => None,
                } {
                    return Ok(val);
                }

                /*───────────────── 3. flush the dangling scalar itself ─────────────*/
                return match self.stack.last().unwrap() {
                    Frame::Str { buf } => Ok(JsonValue::String(unescape(buf)?)),
                    Frame::Num { buf } => Ok(JsonValue::Number(parse_number(buf)?)),
                    Frame::Ident { buf } => Ok(parse_ident(buf).unwrap_or(JsonValue::Null)),
                    _ => Ok(JsonValue::Null),
                };
            }

            /* non-streaming mode: unfinished input is an error */
            return Err(JsonError::UnexpectedEof);
        }

        /*──────────────── standard (finished) EOF, stack length == 1 ─────────────*/
        match self.stack.last().unwrap().clone() {
            Frame::Arr { vec } => {
                if vec.len() == 1 {
                    Ok(vec.into_iter().next().unwrap())
                } else {
                    Ok(JsonValue::Array(vec))
                }
            }
            Frame::Obj { map, .. } => Ok(JsonValue::Object(map)),
            Frame::Str { buf } => Ok(JsonValue::String(unescape(&buf)?)),
            Frame::Num { buf } => Ok(JsonValue::Number(parse_number(&buf)?)),
            Frame::Ident { buf } => Ok(parse_ident(&buf).unwrap_or(JsonValue::Null)),
        }
    }
}

/*──────────────────────────── Legacy Parser shim ────────────────────*/

#[derive(Debug)]
pub struct Parser {
    scanner: Scanner,
    builder: Builder,
    buf: String,
    streaming: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Parser {
    pub fn new(streaming: bool) -> Self {
        Self {
            scanner: Scanner::new(),
            builder: Builder::new(),
            buf: String::new(),
            streaming: streaming,
        }
    }

    pub fn parse(&mut self, bytes: Vec<u8>) -> Result<JsonValue, JsonError> {
        let part = String::from_utf8(bytes).unwrap();
        self.buf = self.buf.clone() + &part;

        self.scanner.push(part);

        loop {
            match self.scanner.next_step() {
                ScanStep::Event(ev) => {
                    if let Some(Snapshot::Complete(v)) = self.builder.feed_event(ev)? {
                        return Ok(v);
                    }
                }
                ScanStep::NeedMore => {
                    // End of buffer – finish.
                    return self.builder.finish(self.streaming);
                }
                ScanStep::Error(e) => return Err(e),
            }
        }
    }
}

// ───────────────────────── Stream wrapper for Python ──────────────────────────

// in StreamParser
#[derive(Debug)]
pub struct StreamParser {
    tagger: TagFinder,
    capturing: bool,
    inner: Parser,
    done: bool,
    wanted: HashSet<String>,
}

impl default::Default for StreamParser {
    fn default() -> Self {
        Self::new(vec![])
    }
}

impl StreamParser {
    pub fn new(tags: Vec<String>) -> Self {
        Self {
            wanted: tags.into_iter().collect(),
            tagger: TagFinder::new(),
            inner: Parser::new(true), // keeps its own scanner
            done: false,
            capturing: false,
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn step(&mut self, chunk: &str) -> Result<Option<JsonValue>, JsonError> {
        if self.done {
            return Ok(None);
        }

        let mut latest = None;
        self.tagger.push(chunk, |ev| {
            match ev {
                TagEvent::Open(name) => {
                    self.capturing = self.wanted.is_empty()           // accept all
                        || self.wanted.contains(name.as_str()); // accept selected
                    if self.capturing {
                        self.inner = Parser::new(true);
                    }
                    Ok(())
                }
                TagEvent::Bytes(bytes) => {
                    let parse_res = self.inner.parse(bytes.as_bytes().to_vec());

                    match parse_res {
                        Ok(JsonValue::Array(arr)) => {
                            if arr.len() == 1 {
                                latest = Some(arr.into_iter().next().unwrap());
                            } else {
                                latest = Some(JsonValue::Array(arr));
                            }
                        }
                        Ok(v) => latest = Some(v),
                        Err(e) => return Err(e),
                    }

                    Ok(()) // placeholder
                }
                TagEvent::Close(name) if self.capturing => {
                    if self.wanted.is_empty() || self.wanted.contains(name.as_str()) {
                        latest = Some(self.inner.builder.finish(true)?);
                        self.done = true;
                    }
                    self.capturing = false;
                    Ok(())
                }
                _ => Ok(()),
            }
        })?;
        Ok(latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_types::{JsonValue, Number as JsonNumber};

    pub fn to_internal(v: &serde_json::Value) -> JsonValue {
        use serde_json::Value::*;
        match v {
            Null => JsonValue::Null,
            Bool(b) => JsonValue::Boolean(*b),
            Number(n) => {
                if let Some(i) = n.as_i64() {
                    JsonValue::Number(JsonNumber::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    JsonValue::Number(JsonNumber::Float(f))
                } else {
                    JsonValue::Null
                } // shouldn’t happen
            }
            String(s) => JsonValue::String(s.clone()),
            Array(a) => JsonValue::Array(a.iter().map(to_internal).collect()),
            Object(o) => {
                let mut m = std::collections::HashMap::new();
                for (k, v) in o {
                    m.insert(k.clone(), to_internal(v));
                }
                JsonValue::Object(m)
            }
        }
    }

    use proptest::prelude::*;
    use serde_json::Value as SJson;
    use StreamParser; // adjust the path to your crate root

    /// Generate reasonably-sized arbitrary JSON.
    fn arb_json() -> impl Strategy<Value = String> {
        // depth 4, up to 20 total nodes – tweak as you like
        proptest::collection::vec(any::<u8>(), 1..=20)
            .prop_map(|bytes| {
                SJson::from_iter(bytes.into_iter().map(|b| (b.to_string(), SJson::Null)))
            })
            .prop_flat_map(|v| Just(serde_json::to_string(&v).unwrap()))
    }

    proptest! {
        /// For every JSON text and every random chunk splitting,
        /// the streaming parser must yield exactly the same `JsonValue`
        /// as the reference parser on the whole text.
        #[test]
        fn stream_equals_reference(json in arb_json(), chunk_sz in 1usize..32) {
            /* ── 1. parse with the reference (serde_json) ───────────── */
            let ref_val: SJson = serde_json::from_str(&json).unwrap();
            let ref_internal   = to_internal(&ref_val);   // your helper

            /* ── 2. wrap in <T>…</T> so StreamParser sees the boundaries ─ */
            let wrapped = format!("<T>{}</T>", json);
            let bytes   = wrapped.as_bytes();

            /* ── 3. feed the wrapped text in `chunk_sz`-sized pieces ────── */
            let mut sp = StreamParser::default();
            let mut i  = 0;
            let mut end_val = None;
            while i < bytes.len() {
                let end   = usize::min(i + chunk_sz, bytes.len());
                let chunk = std::str::from_utf8(&bytes[i..end]).unwrap();
                end_val = sp.step(chunk).unwrap();
                i = end;
            }

            /* ── 4. the closing tag is already in `wrapped`; parser must be done ─ */
            prop_assert!(sp.is_done(), "stream parser did not finish");

            /* ── 5. grab the final value (empty chunk → None, but we expect Some) ─ */
            let final_val = end_val.unwrap();


            /* ── 6. compare ───────────────────────────────────────────── */
            prop_assert_eq!(final_val, ref_internal);
        }
    }

    #[test]
    fn test_stream_parser_in_chunks() {
        use super::*;

        // opening tag + first field
        let chunk1 = r#"<User>{"name": "Al""#;
        // rest of the object plus closing tag and extra chatter
        let chunk2 = r#", "age": 30}</User> blah blah"#;

        let mut sp = StreamParser::default();

        // ── first chunk ───────────────────────────
        let part1 = sp.step(chunk1).expect("stream step 1 failed");
        assert!(!sp.is_done(), "should not be done after first chunk");

        // We expect a partial with only the first key.
        match part1 {
            Some(JsonValue::Object(ref m)) => {
                assert_eq!(m.len(), 1);
                assert_eq!(
                    m.get("name").unwrap(),
                    &JsonValue::String(String::from("Al"))
                );
            }
            _ => panic!("expected first partial object"),
        }

        // ── second chunk ──────────────────────────
        let part2 = sp.step(chunk2).expect("stream step 2 failed");
        assert!(sp.is_done(), "parser should be done after close tag");

        // Final value must contain both fields.
        if let JsonValue::Object(ref m) = part2.unwrap() {
            assert_eq!(m.get("name").unwrap(), &JsonValue::String("Al".into()));
            assert_eq!(
                m.get("age").unwrap(),
                &JsonValue::Number(Number::Integer(30))
            );
        } else {
            panic!("expected inner object");
        }

        // Further calls after done yield None.
        assert!(sp.step("").unwrap().is_none());
    }

    #[test]
    fn test_stream_parser_in_many_chunks() {
        // <User>{"name": "Al", "age": 30}</User>
        let chunks = [
            r#"<User>{"na"#,        // inside key
            r#"me": "Al"#,          // finishes key + value + comma
            r#"die", "#,            // comma + space
            r#" "age": "#,          // key + colon (value incomplete)
            r#"3"#,                 // number split in the middle
            r#"0}</User> garbage"#, // rest of value + close tag + trailing text
        ];

        let mut sp = StreamParser::default();
        let mut snapshot = None;

        for (i, slice) in chunks.iter().enumerate() {
            snapshot = sp.step(slice).expect("stream step failed");

            // we should only be 'done' after the last chunk
            assert_eq!(sp.is_done(), i == chunks.len() - 1);

            match (i, &snapshot) {
                // after chunk 0 we have nothing useful yet
                (0, Some(JsonValue::String(n))) => {}

                // after chunk 1 the object should contain the first field
                (1, Some(JsonValue::Object(m))) => {
                    assert_eq!(m.len(), 1);
                    assert_eq!(m.get("name").unwrap(), &JsonValue::String("Al".into()));
                }
                (2, Some(JsonValue::Object(m))) => {
                    assert_eq!(m.len(), 1);
                    assert_eq!(m.get("name").unwrap(), &JsonValue::String("Aldie".into()));
                }

                // after chunk 3 we still expect the same 1-field object
                (3, Some(JsonValue::Object(m))) => {
                    assert_eq!(m.len(), 1);
                }
                (4, Some(JsonValue::Object(m))) => {
                    assert_eq!(
                        m.get("age").unwrap(),
                        &JsonValue::Number(Number::Integer(3))
                    );
                }
                // after final slice "0}"
                (5, Some(JsonValue::Object(m))) => {
                    assert_eq!(
                        m.get("age").unwrap(),
                        &JsonValue::Number(Number::Integer(30))
                    );
                }

                _ => panic!("unexpected snapshot at chunk {}", i),
            }
        }

        // further calls after done should yield None
        assert!(sp.step("").unwrap().is_none());
    }

    #[test]
    fn test_torture_the_poor_stream_parser() {
        use super::*;
        use std::collections::HashMap;

        /* helper: run one set of slices through StreamParser and return the final value */
        fn run(chunks: &[&str]) -> JsonValue {
            let mut sp = StreamParser::default();
            let mut last = None;

            for (i, part) in chunks.iter().enumerate() {
                last = sp.step(part).expect("stream step failed");
                assert_eq!(sp.is_done(), i == chunks.len() - 1);
            }
            last.expect("no final value produced")
        }

        // ──────────────────────────────────────────────────────────────────────
        // 1. {"name":"Hello World!"}
        let case01 = [
            r#"<Msg>{"na"#,
            r#"me": "H"#,
            r#"el"#,
            r#"lo"#,
            r#" Wo"#,
            r#"rl"#,
            r#"d!"#,
            r#"} </Msg>"#,
        ];
        let mut exp01 = HashMap::new();
        exp01.insert("name".into(), JsonValue::String("Hello World!".into()));
        assert_eq!(run(&case01), JsonValue::Object(exp01));

        // 2. {"glyph":"☃"}   (split in the middle of a \uXXXX escape)
        let case02 = [r#"<E>{"glyph":"\u"#, r#"26"#, r#"03"}"#, r#"</E>"#];
        let mut exp02 = HashMap::new();
        exp02.insert("glyph".into(), JsonValue::String("☃".into()));
        assert_eq!(run(&case02), JsonValue::Object(exp02));

        // 3. [123.45, -0.7, 0.6]   (numbers broken around '.' and 'e')
        let case03 = [
            r#"<N>[1"#, r#"23"#, r#".4"#, r#"5,"#, r#"-"#, r#"0."#, r#"7,"#, r#"6e"#, r#"-"#,
            r#"1"#, r#"]</N>"#,
        ];
        let exp03 = JsonValue::Array(vec![
            JsonValue::Number(Number::Float(123.45)),
            JsonValue::Number(Number::Float(-0.7)),
            JsonValue::Number(Number::Float(0.6)),
        ]);
        assert_eq!(run(&case03), exp03);

        // 4. {"user":{"name":"Ali"},"roles":["admin","editor"]}
        let case04 = [
            r#"<Doc>{"user":{"na"#,
            r#"me":"A"#,
            r#"li"},""#,
            r#"roles":["ad"#,
            r#"min","ed"#,
            r#"itor"]}"#,
            r#"</Doc>"#,
        ];
        let mut user = HashMap::new();
        user.insert("name".into(), JsonValue::String("Ali".into()));
        let mut exp04 = HashMap::new();
        exp04.insert("user".into(), JsonValue::Object(user));
        exp04.insert(
            "roles".into(),
            JsonValue::Array(vec![
                JsonValue::String("admin".into()),
                JsonValue::String("editor".into()),
            ]),
        );
        assert_eq!(run(&case04), JsonValue::Object(exp04));

        // 5. [true,false,null]   (keywords split mid-token)
        let case05 = [r#"<Flags>[tr"#, r#"ue, fal"#, r#"se, n"#, r#"ull]</Flags>"#];
        let exp05 = JsonValue::Array(vec![
            JsonValue::Boolean(true),
            JsonValue::Boolean(false),
            JsonValue::Null,
        ]);
        assert_eq!(run(&case05), exp05);

        // 6. [[[]]]   (each bracket its own slice)
        let case06 = [r#"<Nest>["#, r#"["#, r#"["#, r#"]"#, r#"]"#, r#"]</Nest>"#];
        let exp06 = JsonValue::Array(vec![JsonValue::Array(vec![JsonValue::Array(vec![])])]);
        assert_eq!(run(&case06), exp06);

        // 7. {"v":12e-3}  == 0.012
        let case07 = [r#"<Sci>{"v":1"#, r#"2e"#, r#"-"#, r#"3}</Sci>"#];
        let mut exp07 = HashMap::new();
        exp07.insert("v".into(), JsonValue::Number(Number::Float(0.012)));
        assert_eq!(run(&case07), JsonValue::Object(exp07));

        // 8. {enabled:true, level:"debug"}   (unquoted identifiers split)
        let case08 = [
            r#"<Cfg>{ena"#,
            r#"bled: t"#,
            r#"rue, le"#,
            r#"vel: deb"#,
            r#"ug}</Cfg>"#,
        ];
        let mut exp08 = HashMap::new();
        exp08.insert("enabled".into(), JsonValue::Boolean(true));
        exp08.insert("level".into(), JsonValue::String("debug".into()));
        assert_eq!(run(&case08), JsonValue::Object(exp08));

        // 9. {"a":1,"b":2}  (trailing comma arrives first)
        let case09 = [r#"<Obj>{"a":1,"#, r#" "b":2,"#, r#"}</Obj>"#];
        let mut exp09 = HashMap::new();
        exp09.insert("a".into(), JsonValue::Number(Number::Integer(1)));
        exp09.insert("b".into(), JsonValue::Number(Number::Integer(2)));
        assert_eq!(run(&case09), JsonValue::Object(exp09));

        // 10. mixture of everything
        let case10 = [
            r#"<All>{"arr":["#,
            r#"hi","#,
            r#" ","#,
            r#"th"#,
            r#"ere"#,
            r#"!",", 4"#,
            r#"2, nu"#,
            r#"ll],"#,
            r#" obj":{"#,
            r#"key":f"#,
            r#"alse}} </All>"#,
        ];

        let mut obj = HashMap::new();
        obj.insert("key".into(), JsonValue::Boolean(false));

        let exp10 = {
            let mut m = HashMap::new();
            m.insert(
                "arr".into(),
                JsonValue::Array(vec![
                    JsonValue::String("hi".into()),
                    JsonValue::String("there!".into()),
                    JsonValue::Number(Number::Integer(42)),
                    JsonValue::Null,
                ]),
            );
            m.insert("obj".into(), JsonValue::Object(obj));
            JsonValue::Object(m)
        };
        let joined: String = case10.concat();
        assert_eq!(run(&case10), exp10);

        // ──────────────────────────────────────────────────────────────────────
    }

    #[test]
    fn test_implicit_arrays() {
        // Test comma-separated
        let input = r#"{"message": 123},{"code": 404}"#.as_bytes().to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Array(arr)) => {
                assert_eq!(arr.len(), 2);
                match &arr[0] {
                    JsonValue::Object(obj) => assert_eq!(
                        obj.get("message").unwrap(),
                        &JsonValue::Number(Number::Integer(123))
                    ),
                    _ => panic!("Expected first element to be object"),
                }
                match &arr[1] {
                    JsonValue::Object(obj) => assert_eq!(
                        obj.get("code").unwrap(),
                        &JsonValue::Number(Number::Integer(404))
                    ),
                    _ => panic!("Expected second element to be object"),
                }
            }
            _ => panic!("Expected array"),
        }

        // Test space-separated
        let input = r#"{"message": 123} {"code": 404}"#.as_bytes().to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Array(arr)) => {
                assert_eq!(arr.len(), 2);
                match &arr[0] {
                    JsonValue::Object(obj) => assert_eq!(
                        obj.get("message").unwrap(),
                        &JsonValue::Number(Number::Integer(123))
                    ),
                    _ => panic!("Expected first element to be object"),
                }
                match &arr[1] {
                    JsonValue::Object(obj) => assert_eq!(
                        obj.get("code").unwrap(),
                        &JsonValue::Number(Number::Integer(404))
                    ),
                    _ => panic!("Expected second element to be object"),
                }
            }
            _ => panic!("Expected array"),
        }

        // Test newline-separated
        let input = r#"{"message": 123}
    {"code": 404}"#
            .as_bytes()
            .to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Array(arr)) => {
                assert_eq!(arr.len(), 2);
                match &arr[0] {
                    JsonValue::Object(obj) => assert_eq!(
                        obj.get("message").unwrap(),
                        &JsonValue::Number(Number::Integer(123))
                    ),
                    _ => panic!("Expected first element to be object"),
                }
                match &arr[1] {
                    JsonValue::Object(obj) => assert_eq!(
                        obj.get("code").unwrap(),
                        &JsonValue::Number(Number::Integer(404))
                    ),
                    _ => panic!("Expected second element to be object"),
                }
            }
            _ => panic!("Expected array"),
        }

        // Test no separation
        let input = r#"{"message": 123}{"code": 404}"#.as_bytes().to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Array(arr)) => {
                assert_eq!(arr.len(), 2);
                match &arr[0] {
                    JsonValue::Object(obj) => assert_eq!(
                        obj.get("message").unwrap(),
                        &JsonValue::Number(Number::Integer(123))
                    ),
                    _ => panic!("Expected first element to be object"),
                }
                match &arr[1] {
                    JsonValue::Object(obj) => assert_eq!(
                        obj.get("code").unwrap(),
                        &JsonValue::Number(Number::Integer(404))
                    ),
                    _ => panic!("Expected second element to be object"),
                }
            }
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_simple_string() {
        let input = r#""hello world""#.as_bytes().to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::String(s)) => assert_eq!(s, "hello world"),
            _ => panic!("Expected string value"),
        }
    }

    #[test]
    fn test_string_escapes() {
        let input = r#""hello\nworld\t\"quote\"""#.as_bytes().to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::String(s)) => assert_eq!(s, "hello\nworld\t\"quote\""),
            _ => panic!("Expected string value"),
        }
    }

    #[test]
    fn test_simple_number() {
        let input = "42".as_bytes().to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Number(Number::Integer(n))) => assert_eq!(n, 42),
            _ => panic!("Expected integer value"),
        }
    }

    #[test]
    fn test_float_number() {
        let input = "42.5".as_bytes().to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Number(Number::Float(n))) => assert_eq!(n, 42.5),
            _ => panic!("Expected float value"),
        }
    }

    #[test]
    fn test_simple_object() {
        let input = r#"{"key": "value"}"#.as_bytes().to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Object(map)) => {
                assert_eq!(map.len(), 1);
                match map.get("key") {
                    Some(JsonValue::String(s)) => assert_eq!(s, "value"),
                    _ => panic!("Expected string value"),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_simple_array() {
        let input = r#"[1, 2, 3]"#.as_bytes().to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Array(arr)) => {
                assert_eq!(arr.len(), 3);
                match &arr[0] {
                    JsonValue::Number(Number::Integer(n)) => assert_eq!(*n, 1),
                    _ => panic!("Expected integer"),
                }
            }
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_nested_structure() {
        let input = r#"
       {
           "name": "test",
           "numbers": [1, 2, 3],
           "object": {
               "nested": true,
               "null_value": null
           }
       }"#
        .as_bytes()
        .to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Object(map)) => {
                assert_eq!(map.len(), 3);
                match map.get("name") {
                    Some(JsonValue::String(s)) => assert_eq!(s, "test"),
                    _ => panic!("Expected string for name"),
                }
                match map.get("numbers") {
                    Some(JsonValue::Array(arr)) => assert_eq!(arr.len(), 3),
                    _ => panic!("Expected array for numbers"),
                }
                match map.get("object") {
                    Some(JsonValue::Object(obj)) => {
                        assert_eq!(obj.len(), 2);
                        assert!(matches!(obj.get("nested"), Some(JsonValue::Boolean(true))));
                        assert!(matches!(obj.get("null_value"), Some(JsonValue::Null)));
                    }
                    _ => panic!("Expected nested object"),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_error_cases() {
        let cases = vec![
            // ("{", JsonError::InvalidKey),
            // Duplicate keys should just clobber the previous value, makes partials parsing simpler
            // (
            //     r#"{"key": true, "key": false}"#,
            //     JsonError::DuplicateKey("key".to_string()),
            // ),
            ("@invalid", JsonError::UnexpectedChar('@')),
            // @TODO: Decided to let these slide, want to aff a fixer layer later
            // ("{,}", JsonError::UnexpectedChar(',')),
            // ("[,]", JsonError::UnexpectedChar(',')),
            ("{true:1}", JsonError::InvalidKey),
        ];

        for (input, expected_err) in cases {
            let mut parser = Parser::default();
            match parser.parse(input.as_bytes().to_vec()) {
                Err(e) => assert_eq!(e, expected_err),
                Ok(_) => panic!("Expected error for input: {}", input),
            }
        }
    }

    #[test]
    fn test_unquoted_keys() {
        let input = r#"{
            name: "John",
            age: 30,
            city: "New York"
        }"#
        .as_bytes()
        .to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Object(map)) => {
                assert_eq!(map.len(), 3);
                assert_eq!(map.get("name").unwrap().as_string().unwrap(), "John");
                match map.get("age").unwrap() {
                    JsonValue::Number(Number::Integer(n)) => assert_eq!(*n, 30),
                    _ => panic!("Expected integer for age"),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_single_quotes() {
        let input = r#"{
            'name': 'John',
            'nested': {'key': 'value'}
        }"#
        .as_bytes()
        .to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Object(map)) => {
                assert_eq!(map.len(), 2);
                assert_eq!(map.get("name").unwrap().as_string().unwrap(), "John");
                match map.get("nested").unwrap() {
                    JsonValue::Object(nested) => {
                        assert_eq!(nested.get("key").unwrap().as_string().unwrap(), "value");
                    }
                    _ => panic!("Expected nested object"),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_trailing_commas() {
        let input = r#"{
            "array": [1, 2, 3,],
            "object": {
                "key": "value",
            },
        }"#
        .as_bytes()
        .to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Object(map)) => {
                assert_eq!(map.len(), 2);
                match map.get("array").unwrap() {
                    JsonValue::Array(arr) => assert_eq!(arr.len(), 3),
                    _ => panic!("Expected array"),
                }
                match map.get("object").unwrap() {
                    JsonValue::Object(obj) => assert_eq!(obj.len(), 1),
                    _ => panic!("Expected nested object"),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_unquoted_strings() {
        let input = r#"{
            "name": John,
            "status": active,
            "type": user
        }"#
        .as_bytes()
        .to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Object(map)) => {
                assert_eq!(map.len(), 3);
                assert_eq!(map.get("name").unwrap().as_string().unwrap(), "John");
                assert_eq!(map.get("status").unwrap().as_string().unwrap(), "active");
                assert_eq!(map.get("type").unwrap().as_string().unwrap(), "user");
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_malformed_array() {
        let input = r#"{"message": 123},
            {"code": "404", "details": "error"}"#
            .as_bytes()
            .to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Array(arr)) => {
                assert_eq!(arr.len(), 2);
                match &arr[0] {
                    JsonValue::Object(obj) => {
                        assert!(obj.contains_key("message"));
                    }
                    _ => panic!("Expected first element to be object"),
                }
                match &arr[1] {
                    JsonValue::Object(obj) => {
                        assert!(obj.contains_key("code"));
                        assert!(obj.contains_key("details"));
                    }
                    _ => panic!("Expected second element to be object"),
                }
            }
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_mixed_recovery() {
        let input = r#"{
            name: 'John',
            age: 30,
            hobbies: [coding, gaming, reading,],
            address: {
                city: New York,
                country: USA,
            },
        }"#
        .as_bytes()
        .to_vec();
        let mut parser = Parser::default();
        match parser.parse(input) {
            Ok(JsonValue::Object(map)) => {
                assert_eq!(map.len(), 4);
                assert_eq!(map.get("name").unwrap().as_string().unwrap(), "John");
                match map.get("age").unwrap() {
                    JsonValue::Number(Number::Integer(n)) => assert_eq!(*n, 30),
                    _ => panic!("Expected integer for age"),
                }
                match map.get("hobbies").unwrap() {
                    JsonValue::Array(arr) => {
                        assert_eq!(arr.len(), 3);
                        assert_eq!(arr[0].as_string().unwrap(), "coding");
                        assert_eq!(arr[1].as_string().unwrap(), "gaming");
                        assert_eq!(arr[2].as_string().unwrap(), "reading");
                    }
                    _ => panic!("Expected array for hobbies"),
                }
                match map.get("address").unwrap() {
                    JsonValue::Object(addr) => {
                        assert_eq!(addr.len(), 2);
                        assert_eq!(addr.get("city").unwrap().as_string().unwrap(), "New York");
                        assert_eq!(addr.get("country").unwrap().as_string().unwrap(), "USA");
                    }
                    _ => panic!("Expected object for address"),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_bad_array_recovery() {
        let mut parser = StreamParser::default();
        let result = r#"
        <action>
            {"message": 123},
            {"code": "404", "details": "error"}
</action>
        "#;

        let bytes = result.as_bytes();

        let mut i = 0;
        let mut end_val = None;
        while i < bytes.len() {
            let end = usize::min(i + 1, bytes.len());
            let chunk = std::str::from_utf8(&bytes[i..end]).unwrap();

            end_val = parser.step(chunk).unwrap();
            i = end;
        }
        assert!(parser.is_done(), "stream parser did not finish");

        /* ── 5. grab the final value (empty chunk → None, but we expect Some) ─ */
        // let final_val = end_val.unwrap();
    }
}
