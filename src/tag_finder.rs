//! Incremental tag-scanner:  <Tag> … (raw bytes) … </Tag>

use crate::json_types::JsonError;

#[derive(Debug)]
pub enum TagEvent {
    Open(String),  // <Tag>
    Bytes(String), // payload
    Close(String), // </Tag>
}

#[derive(Default, Debug)]
pub struct TagFinder {
    buf: String,  // carries over up to a whole unfinished tag
    inside: bool, // true ⇢ we’re between <Tag> … </Tag>
}

impl TagFinder {
    pub fn new() -> Self {
        Self::default()
    }
    /// Feed the next text chunk, emitting TagEvents.
    /// `emit` will be called with:
    ///   • TagEvent::Open  { name }
    ///   • TagEvent::Bytes(payload)
    ///   • TagEvent::Close { name }
    pub fn push(
        &mut self,
        chunk: &str,
        mut emit: impl FnMut(TagEvent) -> Result<(), JsonError>,
    ) -> Result<(), JsonError> {
        self.buf.push_str(chunk);

        loop {
            /*──────── look for the next '<' ───────────────────────────*/
            let lt = match self.buf.find('<') {
                Some(i) => i,
                None => break,
            };

            /*──────── everything *before* it is payload ──────────────*/
            if self.inside && lt > 0 {
                let payload = self.buf[..lt].to_owned();
                if !payload.is_empty() {
                    emit(TagEvent::Bytes(payload))?;
                }
            }

            /*──────── look for the matching '>' ───────────────────────*/
            let gt = match self.buf[lt..].find('>') {
                Some(off) => lt + off,
                None => {
                    // tag split across chunks → keep tail for next push()
                    self.buf.drain(..lt); // drop handled bytes
                    return Ok(());
                }
            };

            /*──────── analyse the tag ────────────────────────────────*/
            let tag_body = &self.buf[lt + 1..gt]; // without '<' / '>'
            let is_close = tag_body.starts_with('/');
            let name_part = if is_close { &tag_body[1..] } else { tag_body };
            // strip attributes if present, keep only the tag name
            let name = name_part.split_whitespace().next().unwrap_or("").to_owned();

            if !self.inside && !is_close {
                /* <Tag> : enter streaming mode */
                self.inside = true;
                emit(TagEvent::Open(name))?;
            } else if self.inside && is_close {
                /* </Tag> : exit streaming mode */
                emit(TagEvent::Close(name))?;
                self.inside = false;
            }
            // else: nested <…> or stray </…> – ignore.

            /*──────── consume the tag itself ─────────────────────────*/
            self.buf.drain(..gt + 1);
        }

        /*──────── no '<' left in buffer – handle tail ───────────────*/
        if self.inside && !self.buf.is_empty() {
            emit(TagEvent::Bytes(std::mem::take(&mut self.buf)))?;
        } else {
            // keep only a tiny tail (≤4 chars) to recognise a split tag
            let keep = self.buf.len().min(4);
            let tail = self.buf.split_off(self.buf.len() - keep);
            self.buf = tail;
        }
        Ok(())
    }
}
