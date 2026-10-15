//! Streaming markdown sanitizer for outbound text.
//!
//! Ports `src/messaging/markdown-filter.ts`: a character-level state machine
//! that strips markdown constructs Weixin does not render, emitting as much
//! filtered text as possible on each [`StreamingMarkdownFilter::feed`] and
//! holding back only what is needed to disambiguate a partial marker.
//!
//! Passed through unchanged:
//! * code fences, inline code, tables, horizontal rules, bold
//! * italic / bold-italic wrapping non-CJK content
//!
//! Stripped:
//! * italic / bold-italic wrapping CJK content
//! * H5 / H6 headings
//! * images — removed entirely
//!
//! ```ignore
//! let mut f = StreamingMarkdownFilter::new();
//! let out = f.feed("**bold** 你好") + &f.flush();
//! ```

/// Which inline construct is currently being accumulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineKind {
    Image,
    Bold3,
    Italic,
    UnderBold3,
    UnderItalic,
}

impl InlineKind {
    fn opening(self) -> &'static str {
        match self {
            InlineKind::Image => "![",
            InlineKind::Bold3 => "***",
            InlineKind::Italic => "*",
            InlineKind::UnderBold3 => "___",
            InlineKind::UnderItalic => "_",
        }
    }

    fn closing(self) -> &'static str {
        match self {
            InlineKind::Image => "](", // handled separately
            InlineKind::Bold3 => "***",
            InlineKind::Italic => "*",
            InlineKind::UnderBold3 => "___",
            InlineKind::UnderItalic => "_",
        }
    }
}

/// The streaming filter.
#[derive(Debug, Default)]
pub struct StreamingMarkdownFilter {
    buf: String,
    fence: bool,
    sol: bool,
    inline: Option<(InlineKind, String)>,
}

impl StreamingMarkdownFilter {
    /// Create a filter at the start of a stream.
    pub fn new() -> Self {
        StreamingMarkdownFilter {
            buf: String::new(),
            fence: false,
            sol: true,
            inline: None,
        }
    }

    /// Feed the next chunk, returning everything already safe to emit.
    pub fn feed(&mut self, delta: &str) -> String {
        self.buf.push_str(delta);
        self.pump(false)
    }

    /// Finish the stream, flushing any buffered content.
    pub fn flush(&mut self) -> String {
        self.pump(true)
    }

    /// Convenience helper: filter a complete string in one call.
    pub fn filter(text: &str) -> String {
        let mut f = StreamingMarkdownFilter::new();
        let mut out = f.feed(text);
        out.push_str(&f.flush());
        out
    }

    fn pump(&mut self, eof: bool) -> String {
        let mut out = String::new();
        while !self.buf.is_empty() {
            let s_len = self.buf.len();
            let s_sol = self.sol;
            let s_fence = self.fence;
            let s_inl = self.inline.is_some();

            if self.fence {
                out.push_str(&self.pump_fence(eof));
            } else if let Some((kind, _)) = self.inline {
                out.push_str(&self.pump_inline(kind));
            } else if self.sol {
                out.push_str(&self.pump_sol(eof));
            } else {
                out.push_str(&self.pump_body(eof));
            }

            if self.buf.len() == s_len
                && self.sol == s_sol
                && self.fence == s_fence
                && self.inline.is_some() == s_inl
            {
                break;
            }
        }

        if eof {
            if let Some((kind, acc)) = self.inline.take() {
                out.push_str(kind.opening());
                out.push_str(&acc);
            }
        }
        out
    }

    /// Inside a code fence: pass content through verbatim.
    fn pump_fence(&mut self, eof: bool) -> String {
        if self.sol {
            if self.buf.len() < 3 && !eof {
                return String::new();
            }
            if self.buf.starts_with("```") {
                if let Some(nl) = self.buf[3..].find('\n') {
                    let end = nl + 4;
                    let line = self.buf[..end].to_string();
                    self.buf.drain(..end);
                    self.fence = false;
                    self.sol = true;
                    return line;
                }
                if eof {
                    let line = std::mem::take(&mut self.buf);
                    self.fence = false;
                    return line;
                }
                return String::new();
            }
            self.sol = false;
        }
        match self.buf.find('\n') {
            Some(nl) => {
                let chunk = self.buf[..=nl].to_string();
                self.buf.drain(..=nl);
                self.sol = true;
                chunk
            }
            None => std::mem::take(&mut self.buf),
        }
    }

    /// At the start of a line: consume line-start constructs.
    fn pump_sol(&mut self, eof: bool) -> String {
        let b = &self.buf;

        if b.starts_with('\n') {
            self.buf.drain(..1);
            return "\n".to_string();
        }

        if b.starts_with('`') {
            if b.len() < 3 && !eof {
                return String::new();
            }
            if b.starts_with("```") {
                if let Some(nl) = b.get(3..).and_then(|s| s.find('\n')) {
                    let end = nl + 4;
                    let line = b[..end].to_string();
                    self.buf.drain(..end);
                    self.fence = true;
                    self.sol = true;
                    return line;
                }
                if eof {
                    let line = std::mem::take(&mut self.buf);
                    return line;
                }
                return String::new();
            }
            self.sol = false;
            return String::new();
        }

        if b.starts_with('>') {
            self.sol = false;
            return String::new();
        }

        if b.starts_with('#') {
            let n = b.chars().take_while(|c| *c == '#').count();
            if n == b.len() && !eof {
                return String::new();
            }
            if (5..=6).contains(&n) && n < b.len() {
                let bytes_after = b[n..].chars().next();
                if bytes_after == Some(' ') {
                    self.buf.drain(..n + 1);
                    self.sol = false;
                    return String::new();
                }
            }
            self.sol = false;
            return String::new();
        }

        if b.starts_with(' ') || b.starts_with('\t') {
            if !eof && b.chars().all(|c| c == ' ' || c == '\t') {
                return String::new();
            }
            self.sol = false;
            return String::new();
        }

        let first = b.chars().next().unwrap_or(' ');
        if matches!(first, '-' | '*' | '_') {
            let j = b
                .chars()
                .position(|c| c != first && c != ' ')
                .unwrap_or(b.len());
            if j == b.len() && !eof {
                return String::new();
            }
            let count = b[..j].chars().filter(|c| *c == first).count();
            let next = b[j..].chars().next();
            if (j == b.len() || next == Some('\n')) && count >= 3 {
                let end = (j + 1).min(b.len());
                let line = b[..end].to_string();
                self.buf.drain(..end);
                self.sol = true;
                return line;
            }
            self.sol = false;
            return String::new();
        }

        self.sol = false;
        String::new()
    }

    /// Scan a line body for inline triggers, emitting safe chars eagerly.
    fn pump_body(&mut self, eof: bool) -> String {
        let chars: Vec<char> = self.buf.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            if c == '\n' {
                let byte = chars[..=i].iter().map(|c| c.len_utf8()).sum();
                let out = self.buf[..byte].to_string();
                self.buf.drain(..byte);
                self.sol = true;
                return out;
            }

            if c == '!' && chars.get(i + 1) == Some(&'[') {
                let byte = chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>();
                let out = self.buf[..byte].to_string();
                self.buf.drain(..byte + 2);
                self.inline = Some((InlineKind::Image, String::new()));
                return out;
            }

            if c == '~' {
                i += 1;
                continue;
            }

            if c == '*' || c == '_' {
                let three = chars.get(i + 1) == Some(&c) && chars.get(i + 2) == Some(&c);
                if three {
                    let byte = chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>();
                    let out = self.buf[..byte].to_string();
                    self.buf.drain(..byte + 3);
                    self.inline = Some((
                        if c == '*' {
                            InlineKind::Bold3
                        } else {
                            InlineKind::UnderBold3
                        },
                        String::new(),
                    ));
                    return out;
                }
                if chars.get(i + 1) == Some(&c) {
                    i += 2;
                    continue;
                }
                if let Some(next) = chars.get(i + 1) {
                    if *next != ' ' && *next != '\n' {
                        let byte = chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>();
                        let out = self.buf[..byte].to_string();
                        self.buf.drain(..byte + 1);
                        self.inline = Some((
                            if c == '*' {
                                InlineKind::Italic
                            } else {
                                InlineKind::UnderItalic
                            },
                            String::new(),
                        ));
                        return out;
                    }
                }
                i += 1;
                continue;
            }

            i += 1;
        }

        // Whole buffer consumed without a trigger. Hold back a trailing
        // marker fragment that might still grow.
        let mut hold = 0usize;
        if !eof {
            if self.buf.ends_with("**") || self.buf.ends_with("__") {
                hold = 2;
            } else if self.buf.ends_with('*') || self.buf.ends_with('_') || self.buf.ends_with('!')
            {
                hold = 1;
            }
        }
        let split = self.buf.len() - hold;
        let out = self.buf[..split].to_string();
        self.buf.drain(..split);
        out
    }

    /// Accumulate inline content until the closing marker appears.
    fn pump_inline(&mut self, kind: InlineKind) -> String {
        let Some((_, acc)) = self.inline.as_mut() else {
            return String::new();
        };
        acc.push_str(&self.buf);
        self.buf.clear();

        match kind {
            InlineKind::Image => {
                let acc = self
                    .inline
                    .as_ref()
                    .map(|(_, a)| a.clone())
                    .unwrap_or_default();
                let Some(cb) = acc.find(']') else {
                    return String::new();
                };
                if cb + 1 >= acc.len() {
                    return String::new();
                }
                if acc.as_bytes().get(cb + 1) != Some(&b'(') {
                    let out = format!("![{}", &acc[..=cb]);
                    let rest = acc[cb + 1..].to_string();
                    self.inline = None;
                    self.buf.push_str(&rest);
                    return out;
                }
                let Some(cp) = acc[cb + 2..].find(')').map(|p| p + cb + 2) else {
                    return String::new();
                };
                let rest = acc[cp + 1..].to_string();
                self.inline = None;
                self.buf.push_str(&rest);
                // The whole image construct is dropped.
                String::new()
            }
            InlineKind::Bold3 | InlineKind::UnderBold3 => {
                let marker = kind.closing();
                let acc = self
                    .inline
                    .as_ref()
                    .map(|(_, a)| a.clone())
                    .unwrap_or_default();
                let Some(idx) = acc.find(marker) else {
                    return String::new();
                };
                let content = acc[..idx].to_string();
                let rest = acc[idx + marker.len()..].to_string();
                self.inline = None;
                self.buf.push_str(&rest);
                if contains_cjk(&content) {
                    content
                } else {
                    format!("{marker}{content}{marker}")
                }
            }
            InlineKind::Italic | InlineKind::UnderItalic => {
                let marker = kind.closing();
                let chars: Vec<char> = self
                    .inline
                    .as_ref()
                    .map(|(_, a)| a.chars().collect())
                    .unwrap_or_default();
                let mut j = 0;
                while j < chars.len() {
                    let c = chars[j];
                    if c == '\n' {
                        let byte: usize = chars[..=j].iter().map(|c| c.len_utf8()).sum();
                        let acc = self
                            .inline
                            .as_ref()
                            .map(|(_, a)| a.clone())
                            .unwrap_or_default();
                        let out = format!("{}{}", marker, &acc[..byte]);
                        let rest = acc[byte..].to_string();
                        self.inline = None;
                        self.sol = true;
                        self.buf.push_str(&rest);
                        return out;
                    }
                    if c == marker.chars().next().unwrap() {
                        if chars.get(j + 1) == Some(&c) {
                            j += 2;
                            continue;
                        }
                        let byte: usize = chars[..j].iter().map(|c| c.len_utf8()).sum();
                        let acc = self
                            .inline
                            .as_ref()
                            .map(|(_, a)| a.clone())
                            .unwrap_or_default();
                        let content: String = acc[..byte].to_string();
                        let rest = acc[byte + 1..].to_string();
                        self.inline = None;
                        self.buf.push_str(&rest);
                        return if contains_cjk(&content) {
                            content
                        } else {
                            format!("{marker}{content}{marker}")
                        };
                    }
                    j += 1;
                }
                String::new()
            }
        }
    }
}

/// `true` when the text contains a CJK / Hangul / compatibility ideograph.
pub fn contains_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        let cp = c as u32;
        (0x2E80..=0x9FFF).contains(&cp)
            || (0xAC00..=0xD7AF).contains(&cp)
            || (0xF900..=0xFAFF).contains(&cp)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_filter(input: &str, expected: &str) {
        assert_eq!(
            StreamingMarkdownFilter::filter(input),
            expected,
            "input: {input:?}"
        );
    }

    fn assert_unchanged(input: &str) {
        expect_filter(input, input);
    }

    // ---- Plain text -------------------------------------------------------

    #[test]
    fn plain_text_unchanged() {
        assert_unchanged("hello world");
        assert_unchanged("你好世界");
        assert_unchanged("a\nb");
    }

    // ---- Bold (preserved) -------------------------------------------------

    #[test]
    fn bold_is_preserved() {
        // `**` is plain bold and always passes through, CJK or not.
        expect_filter("**bold**", "**bold**");
        expect_filter("**你好**", "**你好**");
        expect_filter("__bold__", "__bold__");
    }

    // ---- Italic -----------------------------------------------------------

    #[test]
    fn italic_latin_preserved() {
        assert_unchanged("*hi*");
        assert_unchanged("_hi_");
    }

    #[test]
    fn italic_around_cjk_is_stripped() {
        // `*你好*` has CJK content, so the markers are dropped.
        expect_filter("*你好*", "你好");
        expect_filter("_你好_", "你好");
        // Sanity: the same wrapper around ASCII survives.
        expect_filter("*abc*", "*abc*");
    }

    #[test]
    fn italic_around_mixed_cjk_is_stripped() {
        expect_filter("*你说hello*", "你说hello");
        expect_filter("*hello你好*", "hello你好");
        expect_filter("*item 1*", "*item 1*");
    }

    // ---- Bold-italic ------------------------------------------------------

    #[test]
    fn bold3_around_cjk_is_stripped() {
        expect_filter("***你好***", "你好");
        expect_filter("___你好___", "你好");
    }

    #[test]
    fn bold3_around_latin_is_preserved() {
        assert_unchanged("***hi***");
        assert_unchanged("___hi___");
    }

    // ---- Images (removed) -------------------------------------------------

    #[test]
    fn images_are_removed_entirely() {
        expect_filter("![alt](https://x/y.png)", "");
        expect_filter("before ![a](u) after", "before  after");
        expect_filter("![a](u1) middle ![b](u2) end", " middle  end");
    }

    #[test]
    fn image_without_url_is_kept() {
        assert_unchanged("![alt] rest");
    }

    // ---- Blockquotes (passed through) ------------------------------------

    #[test]
    fn blockquotes_pass_through() {
        assert_unchanged("> quoted text");
        assert_unchanged(">quoted");
        assert_unchanged("> line1\n> line2");
        assert_unchanged("> **bold** in quote");
    }

    // ---- Headings ---------------------------------------------------------

    #[test]
    fn h1_to_h4_are_preserved() {
        assert_unchanged("# Title");
        assert_unchanged("## Subtitle");
        assert_unchanged("### Section");
        assert_unchanged("#### Subsection");
    }

    #[test]
    fn h5_and_h6_markers_are_stripped() {
        expect_filter("##### Small Heading", "Small Heading");
        expect_filter("###### Tiny Heading", "Tiny Heading");
        expect_filter("##### Title\nbody text", "Title\nbody text");
    }

    // ---- Horizontal rules (passed through) --------------------------------

    #[test]
    fn horizontal_rules_pass_through() {
        assert_unchanged("before\n---\nafter");
        assert_unchanged("before\n***\nafter");
        assert_unchanged("before\n___\nafter");
        assert_unchanged("before\n- - -\nafter");
        assert_unchanged("text\n---");
        assert_unchanged("text\n--\nnext");
    }

    // ---- Tables (passed through) -----------------------------------------

    #[test]
    fn tables_pass_through() {
        let input = "| Header1 | Header2 |\n|---------|---------||\n| Cell1 | Cell2 |";
        assert_unchanged(input);
    }

    // ---- Code -------------------------------------------------------------

    #[test]
    fn code_fence_passes_through_verbatim() {
        let input = "```rust\nlet **x** = 1;\nlet 你好 = 2;\n```\n";
        assert_unchanged(input);
    }

    #[test]
    fn inline_code_passes_through() {
        assert_unchanged("`a*b`");
        assert_unchanged("`你好`");
    }

    // ---- Strikethrough (passed through) ----------------------------------

    #[test]
    fn strikethrough_passes_through() {
        assert_unchanged("~~deleted~~");
        assert_unchanged("keep ~~this~~ too");
        expect_filter("~~unclosed", "~~unclosed");
    }

    // ---- Streaming == one-shot -------------------------------------------

    #[test]
    fn streaming_matches_one_shot_char_by_char() {
        let inputs = [
            "**你好** and *hi*",
            "text ![img](u) more",
            "```\ncode **bold**\n```\n",
            "#### kept",
            "##### stripped",
            "> **bold** in quote",
            "a\n---\nb",
            "***你好***",
            "~~gone~~",
            "",
        ];
        for input in inputs {
            let one_shot = StreamingMarkdownFilter::filter(input);
            let mut f = StreamingMarkdownFilter::new();
            let mut streamed = String::new();
            for ch in input.chars() {
                streamed.push_str(&f.feed(&ch.to_string()));
            }
            streamed.push_str(&f.flush());
            assert_eq!(one_shot, streamed, "mismatch for {input:?}");
        }
    }

    #[test]
    fn streaming_matches_one_shot_in_two_halves() {
        let inputs = [
            "**你好** and *hi*",
            "before ![a](u) after",
            "##### Title\nbody",
        ];
        for input in inputs {
            let one_shot = StreamingMarkdownFilter::filter(input);
            let chars: Vec<char> = input.chars().collect();
            for split in 0..=chars.len() {
                let mut f = StreamingMarkdownFilter::new();
                let first: String = chars[..split].iter().collect();
                let second: String = chars[split..].iter().collect();
                let mut streamed = f.feed(&first);
                streamed.push_str(&f.feed(&second));
                streamed.push_str(&f.flush());
                assert_eq!(
                    one_shot, streamed,
                    "mismatch for {input:?} at split {split}"
                );
            }
        }
    }

    // ---- Helpers ----------------------------------------------------------

    #[test]
    fn contains_cjk_detects_all_ranges() {
        assert!(contains_cjk("你"));
        assert!(contains_cjk("한"));
        assert!(contains_cjk("\u{F900}"));
        assert!(contains_cjk("abc你def"));
        assert!(!contains_cjk("abc 123"));
        assert!(!contains_cjk("*_~"));
        assert!(!contains_cjk(""));
    }
}
