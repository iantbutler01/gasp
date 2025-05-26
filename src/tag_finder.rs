//! Incremental tag-scanner:  <Tag> … (raw bytes) … </Tag>

use crate::json_types::JsonError;

#[derive(Debug)]
pub enum TagEvent {
    Open(String),  // <Tag>
    Bytes(String), // payload
    Close(String), // </Tag>
}

#[derive(Debug)]
pub struct TagFinder {
    buf: String,                                // carries over up to a whole unfinished tag
    inside: bool,                               // true ⇢ we're between <Tag> … </Tag>
    wanted: std::collections::HashSet<String>, // tags we specifically want to process (empty = all)
    ignored: std::collections::HashSet<String>, // tags to ignore content within
    inside_ignored: bool,                      // true if we're currently inside an ignored tag
    ignored_depth: usize,                      // depth of nested ignored tags
}

impl Default for TagFinder {
    fn default() -> Self {
        Self {
            buf: String::new(),
            inside: false,
            wanted: std::collections::HashSet::new(),
            ignored: std::collections::HashSet::new(),
            inside_ignored: false,
            ignored_depth: 0,
        }
    }
}

impl TagFinder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new TagFinder with specific wanted and ignored tags
    ///
    /// * `wanted` - Tags to specifically process. If empty, all non-ignored tags are processed.
    /// * `ignored` - Tags to completely ignore. These tags and their content will be skipped.
    pub fn new_with_filter(wanted: Vec<String>, ignored: Vec<String>) -> Self {
        Self {
            buf: String::new(),
            inside: false,
            wanted: wanted.into_iter().collect(),
            ignored: ignored.into_iter().collect(),
            inside_ignored: false,
            ignored_depth: 0,
        }
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
            if self.inside && !self.inside_ignored && lt > 0 {
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

            // Check if this tag is ignored
            let is_ignored = self.ignored.contains(&name);

            // Check if this tag is wanted (or if wanted list is empty, all non-ignored tags are wanted)
            let is_wanted = if self.wanted.is_empty() {
                !is_ignored
            } else {
                self.wanted.contains(&name)
            };

            // If we're inside a wanted tag and not in an ignored section,
            // emit the entire tag as content (for nested tags)
            if self.inside && !self.inside_ignored && !is_wanted && !is_ignored {
                // This is a nested tag inside a wanted tag - emit it as content
                let tag_content = self.buf[lt..=gt].to_owned();
                emit(TagEvent::Bytes(tag_content))?;
                self.buf.drain(..gt + 1);
                continue;
            }

            if !is_close {
                /* <Tag> : opening tag */
                if is_ignored {
                    // Start ignoring content
                    self.inside_ignored = true;
                    self.ignored_depth += 1;
                } else if is_wanted && !self.inside_ignored {
                    // Process wanted tags
                    emit(TagEvent::Open(name.clone()))?;
                    // Only set inside=true if we're not already inside another wanted tag
                    if !self.inside {
                        self.inside = true;
                    }
                }
            } else {
                /* </Tag> : closing tag */
                if is_ignored && self.inside_ignored {
                    self.ignored_depth -= 1;
                    if self.ignored_depth == 0 {
                        self.inside_ignored = false;
                    }
                } else if is_wanted && !self.inside_ignored {
                    emit(TagEvent::Close(name.clone()))?;
                    // Check if we're closing the outermost wanted tag
                    // For simplicity, we'll just check if this is the last wanted tag
                    self.inside = false;
                }
            }

            /*──────── consume the tag itself ─────────────────────────*/
            self.buf.drain(..gt + 1);
        }

        /*──────── no '<' left in buffer – handle tail ───────────────*/
        if self.inside && !self.inside_ignored && !self.buf.is_empty() {
            emit(TagEvent::Bytes(std::mem::take(&mut self.buf)))?;
        } else {
            // keep only a tiny tail (≤200 chars) to recognise a split tag
            let keep = self.buf.len().min(200);
            let tail = self.buf.split_off(self.buf.len() - keep);
            self.buf = tail;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_tag_handling() {
        // Test case: Tag name split across chunks
        // For example: "<Report" and "Sub>" as separate chunks

        let mut finder = TagFinder::new();
        let mut events = Vec::new();

        // First chunk contains part of opening tag
        finder
            .push("<Report", |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();

        // Assert no events emitted yet since the tag is incomplete
        assert_eq!(events.len(), 0);

        // Second chunk completes the opening tag
        finder
            .push("Sub>{", |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();

        // Print the actual events for debugging
        println!("Events after second chunk: {:?}", events);

        // The TagFinder should have emitted at least one event (Open)
        assert!(!events.is_empty(), "No events emitted after opening tag");

        // Check that the ReportSub tag was recognized in some form
        let mut found_tag = false;
        for event in &events {
            if let TagEvent::Open(name) = event {
                if name == "ReportSub" {
                    found_tag = true;
                    break;
                }
            }
        }
        assert!(found_tag, "'ReportSub' found");

        // Clear events to test the final chunk
        events.clear();

        // Third chunk has content and closing tag
        finder
            .push(" more content</ReportSub>", |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();

        println!("Events after third chunk: {:?}", events);

        // We should have at least the Close event
        let mut has_close = false;
        let mut has_bytes = false;

        for event in &events {
            match event {
                TagEvent::Close(name) if name == "ReportSub" => has_close = true,
                TagEvent::Bytes(_) => has_bytes = true,
                _ => {}
            }
        }

        assert!(has_close, "No close event for ReportSub tag");
        assert!(has_bytes, "No bytes event for content");
    }

    #[test]
    fn test_extreme_tag_splitting() {
        // Test case where tag opening bracket, name, and closing bracket are all in separate chunks

        let mut finder = TagFinder::new();
        let mut events = Vec::new();

        // First chunk just has opening bracket
        finder
            .push("<", |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();

        println!("Events after first chunk: {:?}", events);

        // Second chunk has tag name
        finder
            .push("ReportSub", |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();

        println!("Events after second chunk: {:?}", events);

        // Third chunk completes the opening tag
        finder
            .push(">content", |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();

        println!("Events after third chunk: {:?}", events);

        // Check for any Open event
        let mut has_open_report = false;
        for event in &events {
            if let TagEvent::Open(name) = event {
                if name == "ReportSub" {
                    has_open_report = true;
                    break;
                }
            }
        }

        assert!(has_open_report, "No open event for ReportSub tag found");

        // Now test closing tag in chunks
        events.clear();

        finder
            .push("</Report", |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();

        finder
            .push("Sub>", |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();

        println!("Events after closing tag: {:?}", events);

        // Check for a Close event
        let mut has_close = false;
        for event in &events {
            if let TagEvent::Close(name) = event {
                if name == "ReportSub" {
                    has_close = true;
                    break;
                }
            }
        }

        assert!(has_close, "No close event for ReportSub tag found");
    }

    #[test]
    fn test_tag_filtering() {
        // This test verifies that:
        // 1. Content inside ignored tags is completely skipped
        // 2. Only content inside wanted tags is processed

        // Create test input with a mix of wanted, ignored, and other tags
        // <OuterTag>
        //    Some outer text
        //    <IgnoredTag>This content should be completely ignored</IgnoredTag>
        //    More outer text
        //    <WantedTag>This content should be processed</WantedTag>
        //    <UnwantedTag>This content should be skipped because not in wanted list</UnwantedTag>
        // </OuterTag>

        let input = r#"<OuterTag>Some outer text<IgnoredTag>This content should be completely ignored</IgnoredTag>More outer text<WantedTag>This content should be processed</WantedTag><UnwantedTag>This content should be skipped</UnwantedTag></OuterTag>"#;

        // Create a finder that specifically wants "WantedTag" and "OuterTag",
        // and ignores "IgnoredTag"
        let wanted = vec!["WantedTag".to_string(), "OuterTag".to_string()];
        let ignored = vec!["IgnoredTag".to_string()];

        let mut finder = TagFinder::new_with_filter(wanted, ignored);
        let mut events = Vec::new();

        // Process the input
        finder
            .push(input, |event| {
                println!("Event: {:?}", event);
                events.push(event);
                Ok(())
            })
            .unwrap();

        // Analyze the events
        let mut open_tags = Vec::new();
        let mut content_chunks = Vec::new();
        let mut close_tags = Vec::new();

        for event in &events {
            match event {
                TagEvent::Open(name) => open_tags.push(name.clone()),
                TagEvent::Bytes(content) => content_chunks.push(content.clone()),
                TagEvent::Close(name) => close_tags.push(name.clone()),
            }
        }

        // Debug output
        println!("Open tags: {:?}", open_tags);
        println!("Content chunks: {:?}", content_chunks);
        println!("Close tags: {:?}", close_tags);

        // Verify that only wanted tags were processed
        assert!(
            open_tags.contains(&"OuterTag".to_string()),
            "OuterTag not opened"
        );
        assert!(
            open_tags.contains(&"WantedTag".to_string()),
            "WantedTag not opened"
        );
        assert!(
            !open_tags.contains(&"IgnoredTag".to_string()),
            "IgnoredTag should be ignored"
        );
        assert!(
            !open_tags.contains(&"UnwantedTag".to_string()),
            "UnwantedTag should be ignored"
        );

        // Verify content
        let all_content = content_chunks.join("");
        assert!(
            all_content.contains("Some outer text"),
            "Missing outer text"
        );
        assert!(
            all_content.contains("More outer text"),
            "Missing text after ignored tag"
        );
        assert!(
            all_content.contains("This content should be processed"),
            "Missing wanted content"
        );
        assert!(
            !all_content.contains("This content should be completely ignored"),
            "Ignored content should not be present"
        );
        assert!(
            !all_content.contains("This content should be skipped"),
            "Unwanted content should not be present"
        );
    }

    #[test]
    fn test_nested_tags_inside_ignored() {
        // This test specifically verifies that tags nested inside ignored tags are also ignored

        // Create test input with nested tags inside an ignored tag
        let input = r#"<OuterTag>Start of content <IgnoredTag><AnotherTag>This should be ignored</AnotherTag></IgnoredTag> End of content</OuterTag>"#;

        // Create a finder that processes "OuterTag" and ignores "IgnoredTag"
        let wanted = vec!["OuterTag".to_string()];
        let ignored = vec!["IgnoredTag".to_string()];

        let mut finder = TagFinder::new_with_filter(wanted, ignored);
        let mut events = Vec::new();

        // Process the input
        finder
            .push(input, |event| {
                println!("Event: {:?}", event);
                events.push(event);
                Ok(())
            })
            .unwrap();

        // Analyze the events
        let mut open_tags = Vec::new();
        let mut content = String::new();
        let mut close_tags = Vec::new();

        for event in &events {
            match event {
                TagEvent::Open(name) => open_tags.push(name.clone()),
                TagEvent::Bytes(text) => content.push_str(text),
                TagEvent::Close(name) => close_tags.push(name.clone()),
            }
        }

        // Debug output
        println!("Open tags: {:?}", open_tags);
        println!("Content: {:?}", content);
        println!("Close tags: {:?}", close_tags);

        // Only the OuterTag should be opened and closed
        assert_eq!(open_tags.len(), 1, "Only one tag should be opened");
        assert_eq!(
            open_tags[0], "OuterTag",
            "OuterTag should be the only opened tag"
        );

        // The AnotherTag should be completely ignored since it's inside the IgnoredTag
        assert!(
            !open_tags.contains(&"AnotherTag".to_string()),
            "AnotherTag should be ignored"
        );

        // The content should only contain the parts outside of the ignored tag
        assert!(
            content.contains("Start of content"),
            "Missing starting content"
        );
        assert!(content.contains("End of content"), "Missing ending content");
        assert!(
            !content.contains("This should be ignored"),
            "Ignored content should not be present"
        );

        // Close tags should match open tags
        assert_eq!(close_tags.len(), 1, "Only one tag should be closed");
        assert_eq!(
            close_tags[0], "OuterTag",
            "OuterTag should be the only closed tag"
        );
    }

    #[test]
    fn test_wanted_tag_with_nested_content() {
        // This test verifies that content in nested tags within a wanted tag is processed correctly

        // Create test input with nested structure inside a wanted tag
        let input = r#"<WantedTag>Outer content <NestedTag>Inner content</NestedTag> More outer content</WantedTag><UnwantedTag>Skip this</UnwantedTag>"#;

        // Create a finder that only wants "WantedTag"
        let wanted = vec!["WantedTag".to_string()];
        let ignored = vec![];

        let mut finder = TagFinder::new_with_filter(wanted, ignored);
        let mut events = Vec::new();

        // Process the input
        finder
            .push(input, |event| {
                println!("Event: {:?}", event);
                events.push(event);
                Ok(())
            })
            .unwrap();

        // Analyze the events
        let mut open_tags = Vec::new();
        let mut content_chunks = Vec::new();
        let mut close_tags = Vec::new();

        for event in &events {
            match event {
                TagEvent::Open(name) => open_tags.push(name.clone()),
                TagEvent::Bytes(content) => content_chunks.push(content.clone()),
                TagEvent::Close(name) => close_tags.push(name.clone()),
            }
        }

        // Debug output
        println!("Open tags: {:?}", open_tags);
        println!("Content chunks: {:?}", content_chunks);
        println!("Close tags: {:?}", close_tags);

        // Verify that only the wanted tag was processed
        assert_eq!(open_tags.len(), 1, "Only one tag should be opened");
        assert_eq!(
            open_tags[0], "WantedTag",
            "WantedTag should be the only opened tag"
        );

        // Verify the UnwantedTag was ignored
        assert!(
            !open_tags.contains(&"UnwantedTag".to_string()),
            "UnwantedTag should be ignored"
        );

        // Verify all content inside the wanted tag was captured, including nested tags
        let all_content = content_chunks.join("");
        assert!(
            all_content.contains("Outer content"),
            "Missing outer content"
        );
        assert!(
            all_content.contains("<NestedTag>"),
            "Missing nested tag opening"
        );
        assert!(
            all_content.contains("Inner content"),
            "Missing inner content"
        );
        assert!(
            all_content.contains("</NestedTag>"),
            "Missing nested tag closing"
        );
        assert!(
            all_content.contains("More outer content"),
            "Missing content after nested tag"
        );

        // Verify unwanted content was not included
        assert!(
            !all_content.contains("Skip this"),
            "Unwanted content should not be present"
        );

        // Verify close tags
        assert_eq!(close_tags.len(), 1, "Only one tag should be closed");
        assert_eq!(
            close_tags[0], "WantedTag",
            "WantedTag should be the only closed tag"
        );
    }
}
