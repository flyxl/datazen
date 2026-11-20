//! T167: `components/ai/AiMessageContent.tsx` port — the message body
//! renderer. React splits the content into alternating text / fenced-code
//! segments (`lib/aiMessageBlocks.ts`), renders code segments through
//! `AiCodeBlock` and the text segments through `marked` (GFM + `breaks`),
//! sanitizes the HTML, links `[M1]`-style memory citations, and mounts the
//! result with `dangerouslySetInnerHTML`.
//!
//! The Rune port keeps the segment split and the `AiCodeBlock` track and
//! replaces the HTML pipeline with a structured renderer:
//!
//! | React source | Rune item |
//! |---|---|
//! | `parseMessageSegments` / `MessageSegment` (`lib/aiMessageBlocks.ts`, T230-S2) | [`MessageSegment`] + [`parse_message_segments`] (ported here; the S2 task reuses it) |
//! | `marked.parse` GFM subset | [`Block`] + [`inline_parse`] (paragraphs, ATX headings, bullet / ordered lists, quotes, rules, bold, italic, inline code, links, citations) |
//! | `sanitizeHtml` | nothing to port: the renderer builds `El`s directly, so there is no script-carrying HTML to sanitize |
//! | `linkMemoryCitations` (`[M1]` → `<a href="#memory-M1">`) | `Link` id `memory-M1` on the citation range, dispatched through `on_cite_memory` |
//! | `AiCodeBlock` code segments | [`AiCodeBlock`] with per-index state wiring ([`AiMessageCodeWiring`]) |
//! | `animate-pulse` while streaming | `LoopSpec::opacity(2.0, 0.0, 0.5, 1.0)`, the same Tailwind pulse keyframes `ChatBubble` and `AiCodeBlock` use |
//!
//! # State ownership
//!
//! The component is a pure render: like `ChatBubble`, it owns no runtime
//! state. `AiCodeBlockState` slots are the caller's — one slot per code
//! segment of the message, addressed by the segment index through
//! [`AiMessageCodeWiring`] (the caller's closures locate the slot in its
//! own `S`). The SQL callbacks are the host's and are forwarded verbatim.
//!
//! # Deliberate boundaries
//!
//! * **Inline markup does not nest.** marked nests (`**a *b* c**`); the
//!   scanner matches one marker level, so an inner marker inside a bold
//!   run stays literal text. Chat messages in practice use one level.
//! * **No strikethrough.** `~~text~~` keeps its text without the strike
//!   (the `Span` API has no strikethrough field); the markers themselves
//!   are consumed as they are in GFM.
//! * **No tables.** GFM pipe tables render as their source text, as they
//!   would if `marked`'s table extension were off.
//! * **No HTML entities.** React decodes `&amp;` in the HTML layer; the
//!   port has no HTML layer, so an entity stays literal.
//! * **Quote blocks are one level.** `>` lines collect into one quoted
//!   block with inline parsing, but no nested block parsing inside it.
//! * **`decoration-dotted underline-offset-2` is an underline.** The
//!   citation / link resting style is an accent underline in one weight
//!   (the `Span` API has one underline, no dot pattern or offset).
//! * **Heading sizes are one step, not prose multiples.** React's
//!   `prose` `h1` is 2.25em of the surrounding `text-xs`; the port maps
//!   `h1`/`h2` to `text_small` / `text_small_medium` and `h3`–`h6` to
//!   `text_xsmall` bold, which keeps chat-sized headings inside the
//!   bubble width.
//! * **A message with no code segments still needs the wiring struct**
//!   (type-level honesty: the caller cannot construct a half-wired
//!   component for a message that does have code).
//! * **Code dialect normalisation is `AiCodeBlock`'s.** The `sqlDialect`
//!   prop seeds each block's dialect select; unknown values fall back to
//!   PostgreSQL there, exactly as the `AiCodeBlockProps` port does.

use std::rc::Rc;

use rune::{col, row, text, El, LoopSpec, Span, Tx};
use rune_kit::Kit;

use super::ai_code_block::AiCodeBlock;
use super::ai_code_block_state::{AiCodeBlockProps, AiCodeBlockState, SqlDialect};
use crate::ui::kit::kit;

/// React `MessageSegment` (`lib/aiMessageBlocks.ts`): alternating text and
/// fenced-code segments, in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MessageSegment {
    /// React `{ type: 'text', content }`.
    Text { content: String },
    /// React `{ type: 'code', language, code }`; `language` is already
    /// lower-cased by the parser, as the React regex does.
    Code { language: String, code: String },
}

/// React `parseMessageSegments`: split on fenced code blocks
/// (`FENCE_RE = /```([\w-]*)\s*\n([\s\S]*?)```/g`). An unclosed fence
/// (streaming) matches nothing and stays text, as the regex does. The
/// closing fence may be anywhere in the remaining text (the regex is not
/// line-anchored); the captured code is `trimEnd()`-ed and the language
/// tag lower-cased.
pub(crate) fn parse_message_segments(src: &str) -> Vec<MessageSegment> {
    if src.is_empty() {
        return Vec::new();
    }
    let bytes = src.as_bytes();
    let mut segments: Vec<MessageSegment> = Vec::new();
    let mut text_start = 0usize;
    let mut i = 0usize;
    while i + 3 <= bytes.len() {
        if &bytes[i..i + 3] != b"```" {
            i += 1;
            continue;
        }
        // Language tag: `[\w-]*` (ASCII word chars plus dash), then `\s*\n`
        // to the end of the fence line.
        let mut j = i + 3;
        let lang_start = j;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'-') {
            j += 1;
        }
        let language = src[lang_start..j].to_ascii_lowercase();
        // `\s*\n`: the regex eats same-line whitespace up to the newline.
        while j < bytes.len() && (bytes[j] as char).is_ascii_whitespace() && bytes[j] != b'\n' {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'\n' {
            // No newline after the fence line: the regex does not match
            // here; keep scanning from the next byte.
            i += 1;
            continue;
        }
        j += 1;
        // Non-greedy capture to the next ``` anywhere ahead.
        let code_start = j;
        let mut close = None;
        let mut k = j;
        while k + 3 <= bytes.len() {
            if &bytes[k..k + 3] == b"```" {
                close = Some(k);
                break;
            }
            k += 1;
        }
        let code_end = match close {
            Some(c) => c,
            None => {
                // Unclosed: not a match; keep scanning past this fence.
                i += 1;
                continue;
            }
        };
        let after = code_end + 3;
        if text_start < i {
            segments.push(MessageSegment::Text { content: src[text_start..i].to_owned() });
        }
        let code = src[code_start..code_end].trim_end().to_owned();
        segments.push(MessageSegment::Code { language, code });
        text_start = after;
        i = after;
    }
    if text_start < src.len() {
        segments.push(MessageSegment::Text { content: src[text_start..].to_owned() });
    }
    segments
}

/// One block-level markdown element of a text segment.
#[derive(Clone, Debug)]
enum Block {
    /// A paragraph: consecutive non-blank lines joined with `\n`
    /// (`marked` `breaks: true` renders each as a line).
    Paragraph(String),
    /// An ATX heading `#`..`######`.
    Heading { level: u8, text: String },
    /// Consecutive `-` / `*` / `+` items.
    Bullet(Vec<String>),
    /// Consecutive `N.` items with the first source number.
    Ordered { first: u64, items: Vec<String> },
    /// Consecutive `>` lines, joined with `\n`.
    Quote(String),
    /// `---` / `***` / `___` (three or more, alone on the line).
    Rule,
}

/// Split a text segment's trimmed content into blocks. GFM pipe tables
/// are not recognised (see the module docs) and render as paragraph text.
fn parse_blocks(src: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut bullet: Vec<String> = Vec::new();
    let mut ordered: Option<(u64, Vec<String>)> = None;
    let mut quote: Vec<&str> = Vec::new();

    fn flush_paragraph(blocks: &mut Vec<Block>, paragraph: &mut Vec<&str>) {
        if !paragraph.is_empty() {
            blocks.push(Block::Paragraph(paragraph.join("\n")));
            paragraph.clear();
        }
    }
    fn flush_lists(
        blocks: &mut Vec<Block>,
        bullet: &mut Vec<String>,
        ordered: &mut Option<(u64, Vec<String>)>,
    ) {
        if !bullet.is_empty() {
            blocks.push(Block::Bullet(std::mem::take(bullet)));
        }
        if let Some((first, items)) = ordered.take() {
            blocks.push(Block::Ordered { first, items });
        }
    }
    fn flush_quote(blocks: &mut Vec<Block>, quote: &mut Vec<&str>) {
        if !quote.is_empty() {
            blocks.push(Block::Quote(quote.join("\n")));
            quote.clear();
        }
    }

    for raw_line in src.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_lists(&mut blocks, &mut bullet, &mut ordered);
            flush_quote(&mut blocks, &mut quote);
            continue;
        }
        // Horizontal rule: three or more of one marker, alone on the line.
        if is_rule_line(line) {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_lists(&mut blocks, &mut bullet, &mut ordered);
            flush_quote(&mut blocks, &mut quote);
            blocks.push(Block::Rule);
            continue;
        }
        // ATX heading: 1-6 `#`, then a space or the line end.
        if let Some((level, rest)) = heading_level(line) {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_lists(&mut blocks, &mut bullet, &mut ordered);
            flush_quote(&mut blocks, &mut quote);
            blocks.push(Block::Heading {
                level,
                text: rest.trim().to_owned(),
            });
            continue;
        }
        // Block quote: `>` or `> ` prefix.
        if let Some(rest) = line.strip_prefix('>') {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_lists(&mut blocks, &mut bullet, &mut ordered);
            quote.push(rest.trim_start());
            continue;
        }
        // Bullet item: `- ` / `* ` / `+ `.
        if let Some(rest) = bullet_item(line) {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_quote(&mut blocks, &mut quote);
            // A bullet ends an ordered list and vice versa, as a blank line
            // would not be needed for in GFM.
            if let Some((first, items)) = ordered.take() {
                blocks.push(Block::Ordered { first, items });
            }
            bullet.push(rest.to_owned());
            continue;
        }
        // Ordered item: `N. `.
        if let Some((n, rest)) = ordered_item(line) {
            flush_paragraph(&mut blocks, &mut paragraph);
            flush_quote(&mut blocks, &mut quote);
            if !bullet.is_empty() {
                blocks.push(Block::Bullet(std::mem::take(&mut bullet)));
            }
            match ordered.as_mut() {
                Some((_, items)) => items.push(rest.to_owned()),
                None => {
                    ordered = Some((n, vec![rest.to_owned()]));
                }
            }
            continue;
        }
        // Plain line: a paragraph line, ending any list or quote.
        flush_lists(&mut blocks, &mut bullet, &mut ordered);
        flush_quote(&mut blocks, &mut quote);
        paragraph.push(line);
    }
    flush_paragraph(&mut blocks, &mut paragraph);
    flush_lists(&mut blocks, &mut bullet, &mut ordered);
    flush_quote(&mut blocks, &mut quote);
    blocks
}

/// `---` / `***` / `___`, three or more of the one character, nothing else.
fn is_rule_line(line: &str) -> bool {
    let t = line.trim();
    for marker in ['-', '*', '_'] {
        if t.len() >= 3 && t.chars().all(|c| c == marker) {
            return true;
        }
    }
    false
}

/// ATX heading level: 1–6 `#` followed by a space (or the line end, which
/// React's `marked` also accepts).
fn heading_level(line: &str) -> Option<(u8, &str)> {
    let mut level = 0usize;
    let bytes = line.as_bytes();
    while level < bytes.len() && bytes[level] == b'#' {
        level += 1;
    }
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
        Some((level as u8, rest))
    } else {
        None
    }
}

/// `- ` / `* ` / `+ ` item text (the marker needs the space; a lone
/// `-`/`*`/`+` is not an item; rule lines are checked before this, so
/// `***` never reaches here).
fn bullet_item(line: &str) -> Option<&str> {
    for marker in ['-', '*', '+'] {
        if let Some(after) = line.strip_prefix(marker) {
            if let Some(item) = after.strip_prefix(' ') {
                return Some(item);
            }
        }
    }
    None
}

/// `N. ` item text (GFM allows up to nine digits).
fn ordered_item(line: &str) -> Option<(u64, &str)> {
    let digits_end = line.find(|c: char| !c.is_ascii_digit()).unwrap_or(line.len());
    if digits_end == 0 || digits_end > 9 {
        return None;
    }
    let after = line.get(digits_end..digits_end + 2)?;
    if after != ". " {
        return None;
    }
    let n = line[..digits_end].parse::<u64>().ok()?;
    let rest = line.get(digits_end + 2..)?;
    Some((n, rest))
}

/// The inline parse result: the cleaned text plus byte-range spans and
/// links over it.
struct Inlined {
    content: String,
    spans: Vec<Span>,
    links: Vec<rune::Link>,
}

/// The scanner context: the kit (for colors) and the allowed citation
/// references (React `memoryReferences`).
struct InlineCtx<'a> {
    k: &'a Kit,
    citations: Option<&'a [String]>,
}

/// React's inline pipeline (marked spans + `sanitizeHtml` + the citation
/// link post-pass), as one left-to-right scan of one block's text.
fn inline_parse(src: &str, ctx: &InlineCtx<'_>) -> Inlined {
    let mut out = String::with_capacity(src.len());
    let mut spans: Vec<Span> = Vec::new();
    let mut links: Vec<rune::Link> = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        // Backslash escape of a marker keeps the escaped character.
        if b == b'\\' && i + 1 < bytes.len() && is_inline_marker(bytes[i + 1]) {
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }
        // Bold: `**` / `__`. The close must sit after non-empty content,
        // so an unclosed run stays literal (marked's behaviour).
        if (b == b'*' && starts_with_at(bytes, i, b"**")) || (b == b'_' && starts_with_at(bytes, i, b"__")) {
            let marker = if b == b'*' { "**" } else { "__" };
            if let Some(end) = find_marker(bytes, i + 2, marker).filter(|&e| e > i + 2) {
                let inner = &src[i + 2..end];
                let start = out.len();
                out.push_str(inner);
                spans.push(Span::new(start, out.len()).bold());
                i = end + 2;
                continue;
            }
        }
        // Italic: `*` / `_`; same non-empty-content rule (the fallback of
        // an unclosed `**` must not eat the second star as an empty
        // italic).
        if b == b'*' || b == b'_' {
            if let Some(end) = find_marker(bytes, i + 1, if b == b'*' { "*" } else { "_" })
                .filter(|&e| e > i + 1)
            {
                let inner = &src[i + 1..end];
                let start = out.len();
                out.push_str(inner);
                spans.push(Span::new(start, out.len()).italic());
                i = end + 1;
                continue;
            }
        }
        // Strikethrough: `~~text~~` keeps the text, no strike style
        // (see the module docs).
        if b == b'~' && starts_with_at(bytes, i, b"~~") {
            if let Some(end) = find_marker(bytes, i + 2, "~~").filter(|&e| e > i + 2) {
                out.push_str(&src[i + 2..end]);
                i = end + 2;
                continue;
            }
        }
        // Inline code: `` `code` ``.
        if b == b'`' {
            if let Some(end) = find_marker(bytes, i + 1, "`").filter(|&e| e > i + 1) {
                let inner = &src[i + 1..end];
                let start = out.len();
                out.push_str(inner);
                spans.push(Span::new(start, out.len()).mono().bg(ctx.k.c.layer_01, 1.0));
                i = end + 1;
                continue;
            }
        }
        // Link or citation: `[` … `]`.
        if b == b'[' {
            // Citation first (React links `[M1]` after the markdown render,
            // so bold inside a citation stays bold; here one range gets both).
            if let Some(reference) = citation_at(src, i) {
                let allowed = ctx.citations.is_some_and(|refs| refs.iter().any(|r| r == &reference));
                if allowed {
                    let start = out.len();
                    let label = format!("[{reference}]");
                    out.push_str(&label);
                    spans.push(
                        Span::new(start, out.len())
                            .color(ctx.k.c.text_accent)
                            .underline(ctx.k.c.text_accent),
                    );
                    links.push(
                        rune::Link::new(start..out.len(), format!("memory-{reference}"))
                            .label(format!("memory citation {reference}")),
                    );
                    i += label.len();
                    continue;
                }
            }
            // Markdown link: `[label](url)`.
            if let Some((label, url)) = markdown_link_at(src, i) {
                let start = out.len();
                out.push_str(&label);
                spans.push(
                    Span::new(start, out.len())
                        .color(ctx.k.c.text_accent)
                        .underline(ctx.k.c.text_accent),
                );
                links.push(rune::Link::new(start..out.len(), url.clone()).label(label.clone()));
                i += label.len() + url.len() + 4;
                continue;
            }
        }
        // Plain character: copy the full UTF-8 sequence.
        let ch_len = utf8_len_at(bytes, i);
        let end = (i + ch_len).min(bytes.len());
        out.push_str(&src[i..end]);
        i = end;
    }
    Inlined { content: out, spans, links }
}

/// Whether `b` is a marker character the backslash escape applies to.
fn is_inline_marker(b: u8) -> bool {
    matches!(b, b'*' | b'_' | b'`' | b'~' | b'[' | b']' | b'\\')
}

/// `bytes[i..]` starts with `needle`.
fn starts_with_at(bytes: &[u8], i: usize, needle: &[u8]) -> bool {
    bytes[i..].starts_with(needle)
}

/// The next occurrence of `marker` at or after `from`, as a byte index.
fn find_marker(bytes: &[u8], from: usize, marker: &str) -> Option<usize> {
    let m = marker.as_bytes();
    let mut k = from;
    while k + m.len() <= bytes.len() {
        if &bytes[k..k + m.len()] == m {
            return Some(k);
        }
        k += 1;
    }
    None
}

/// The UTF-8 sequence length starting at `i` (1 for a continuation byte,
/// defensive; input is valid UTF-8).
fn utf8_len_at(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

/// `[M<digits>]` at `i`: the citation reference (`M1`), when it is closed
/// before any other `[`.
fn citation_at(src: &str, i: usize) -> Option<String> {
    let bytes = src.as_bytes();
    let mut j = i + 1;
    if j >= bytes.len() || bytes[j] != b'M' {
        return None;
    }
    j += 1;
    let digits_start = j;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    if j == digits_start || j >= bytes.len() || bytes[j] != b']' {
        return None;
    }
    Some(src[i + 1..j].to_owned())
}

/// `[label](url)` at `i`: the label and url, when the url is non-empty
/// and whitespace-free (React's `sanitizeHtml` keeps such hrefs only).
fn markdown_link_at(src: &str, i: usize) -> Option<(String, String)> {
    let close = find_marker(src.as_bytes(), i + 1, "]")?;
    if close <= i + 1 {
        return None;
    }
    let label = &src[i + 1..close];
    if label.contains('[') {
        return None;
    }
    let bytes = src.as_bytes();
    let mut j = close + 1;
    if j >= bytes.len() || bytes[j] != b'(' {
        return None;
    }
    j += 1;
    let url_start = j;
    while j < bytes.len() && bytes[j] != b')' {
        j += 1;
    }
    if j >= bytes.len() || j == url_start {
        return None;
    }
    let url = &src[url_start..j];
    if url.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    Some((label.to_owned(), url.to_owned()))
}

/// The per-message `AiCodeBlock` event wiring: the caller's closures
/// route an interaction on the `idx`-th code segment of this message
/// into the caller's own `S`. Rendering reads a slot snapshot the caller
/// passes to [`AiMessageContent::build`] (one slot per rendered block —
/// a shared slot would share fullscreen, dialect and copy feedback; see
/// the `AiCodeBlock` docs).
pub(crate) struct AiMessageCodeWiring<S: 'static> {
    /// Flips the `idx`-th block's fullscreen.
    pub toggle_fullscreen: Rc<dyn Fn(usize, &mut S)>,
    /// Picks the `idx`-th block's dialect.
    pub set_dialect: Rc<dyn Fn(usize, &mut S, SqlDialect)>,
    /// Opens / closes the `idx`-th block's dialect listbox.
    pub toggle_dialect_open: Rc<dyn Fn(usize, &mut S)>,
    /// Closes the `idx`-th block's dialect listbox.
    pub close_dialect: Rc<dyn Fn(usize, &mut S)>,
    /// The `idx`-th block's optimistic copy mark; returns the request id.
    pub copy_pressed: Rc<dyn Fn(usize, &mut S) -> u64>,
}

impl<S: 'static> Clone for AiMessageCodeWiring<S> {
    fn clone(&self) -> Self {
        Self {
            toggle_fullscreen: Rc::clone(&self.toggle_fullscreen),
            set_dialect: Rc::clone(&self.set_dialect),
            toggle_dialect_open: Rc::clone(&self.toggle_dialect_open),
            close_dialect: Rc::clone(&self.close_dialect),
            copy_pressed: Rc::clone(&self.copy_pressed),
        }
    }
}

/// React `AiMessageContentProps`. The SQL callbacks and `sqlDialect` are
/// forwarded to every `AiCodeBlock`; `on_cite_memory` and `on_open_link`
/// are this port's dispatch for the citation / link `Link`s (React's
/// browser handles `href` navigation; here the host decides).
pub(crate) struct AiMessageContent<S: 'static> {
    content: String,
    wiring: AiMessageCodeWiring<S>,
    sql_dialect: Option<SqlDialect>,
    is_streaming: bool,
    memory_references: Option<Vec<String>>,
    on_insert_sql: Option<Rc<dyn Fn(&mut S, String)>>,
    on_run_code: Option<Rc<dyn Fn(&mut S, String, String)>>,
    on_new_query: Option<Rc<dyn Fn(&mut S, String)>>,
    on_copy_code: Option<Rc<dyn Fn(&mut S, String, u64)>>,
    on_cite_memory: Option<Rc<dyn Fn(&mut S, String)>>,
    on_open_link: Option<Rc<dyn Fn(&mut S, String)>>,
    key: String,
}

impl<S: 'static> AiMessageContent<S> {
    /// React `<AiMessageContent content={...} />` with the code-block
    /// state wiring the caller owns.
    pub(crate) fn new(content: impl Into<String>, wiring: AiMessageCodeWiring<S>) -> Self {
        Self {
            content: content.into(),
            wiring,
            sql_dialect: None,
            is_streaming: false,
            memory_references: None,
            on_insert_sql: None,
            on_run_code: None,
            on_new_query: None,
            on_copy_code: None,
            on_cite_memory: None,
            on_open_link: None,
            key: "ai-message-content".to_string(),
        }
    }

    /// React `sqlDialect?: string` (one of the seven values; an unknown
    /// value falls back to PostgreSQL, as `AiCodeBlockState::new` does).
    pub(crate) fn sql_dialect(mut self, sql_dialect: impl AsRef<str>) -> Self {
        self.sql_dialect = SqlDialect::from_value(sql_dialect.as_ref());
        self
    }

    /// React `isStreaming?: boolean`.
    pub(crate) fn is_streaming(mut self, is_streaming: bool) -> Self {
        self.is_streaming = is_streaming;
        self
    }

    /// React `memoryReferences?: string[]`.
    pub(crate) fn memory_references(mut self, references: Vec<String>) -> Self {
        self.memory_references = Some(references);
        self
    }

    /// React `onInsertSql?: (sql) => void`.
    pub(crate) fn on_insert_sql(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_insert_sql = Some(Rc::new(f));
        self
    }

    /// React `onRunCode?: (code, language) => void`.
    pub(crate) fn on_run_code(
        mut self,
        f: impl Fn(&mut S, String, String) + 'static,
    ) -> Self {
        self.on_run_code = Some(Rc::new(f));
        self
    }

    /// React `onNewQuery?: (code) => void`.
    pub(crate) fn on_new_query(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_new_query = Some(Rc::new(f));
        self
    }

    /// The host's clipboard write starter for a code segment's copy
    /// button (`AiCodeBlock`'s `on_copy`).
    pub(crate) fn on_copy_code(
        mut self,
        f: impl Fn(&mut S, String, u64) + 'static,
    ) -> Self {
        self.on_copy_code = Some(Rc::new(f));
        self
    }

    /// A `[M1]` citation press: React's `<a href="#memory-M1">` click,
    /// which the bubble routes to `ChatBubbleState::open_memory_citation`.
    pub(crate) fn on_cite_memory(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_cite_memory = Some(Rc::new(f));
        self
    }

    /// A markdown link press: React's browser navigates the `href`; the
    /// host opens it.
    pub(crate) fn on_open_link(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_open_link = Some(Rc::new(f));
        self
    }

    /// Override the root key prefix.
    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// Builds the content: the text segments' blocks, the code segments'
    /// `AiCodeBlock`s, and the streaming pulse on the root (React's
    /// `animate-pulse` on the single wrapper `div`). `slots` is the
    /// caller-owned snapshot of this message's code-block state slots
    /// (one per code segment, seeded to the segment count); a missing
    /// slot renders with the default state, which only a lagging seed
    /// can produce.
    pub(crate) fn build(self, slots: &[AiCodeBlockState], dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let segments = parse_message_segments(&self.content);
        let mut root = col().key(self.key.clone()).min_w(0.0).w_full();
        if self.is_streaming {
            root = root.looping(LoopSpec::opacity(2.0, 0.0, 0.5, 1.0));
        }
        if segments.is_empty() {
            return root;
        }
        let mut code_index = 0usize;
        for (seg_n, segment) in segments.into_iter().enumerate() {
            match segment {
                MessageSegment::Text { content } => {
                    let trimmed = content.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    // React's per-segment wrapper `div.ai-markdown`; the
                    // segment index keeps sibling containers unique.
                    let key = format!("{}-text-{}", self.key, seg_n);
                    let mut text_col = col().key(key).min_w(0.0).w_full().gap(4.0);
                    for (block_n, block) in parse_blocks(trimmed).into_iter().enumerate() {
                        text_col = text_col.child(self.block_el(&k, block, block_n));
                    }
                    root = root.child(text_col);
                }
                MessageSegment::Code { language, code } => {
                    let idx = code_index;
                    code_index += 1;
                    let props = AiCodeBlockProps {
                        language,
                        code,
                        sql_dialect: self.sql_dialect.unwrap_or_default(),
                        show_actions: true,
                        is_streaming: self.is_streaming,
                    };
                    let toggle_fs = self.wiring.toggle_fullscreen.clone();
                    let set_dialect = self.wiring.set_dialect.clone();
                    let toggle_open = self.wiring.toggle_dialect_open.clone();
                    let close_dialect = self.wiring.close_dialect.clone();
                    let copy_pressed = self.wiring.copy_pressed.clone();
                    let mut block = AiCodeBlock::<S>::new(
                        props,
                        |_s: &S| AiCodeBlockState::default(),
                        move |s: &mut S| toggle_fs(idx, s),
                        move |s: &mut S, dialect| set_dialect(idx, s, dialect),
                        move |s: &mut S| toggle_open(idx, s),
                        move |s: &mut S| close_dialect(idx, s),
                        move |s: &mut S| copy_pressed(idx, s),
                    )
                    .key(format!("{}-code-{}", self.key, idx));
                    if let Some(on_insert_sql) = self.on_insert_sql.clone() {
                        block = block.on_insert_sql(move |s: &mut S, code: String| {
                            on_insert_sql(s, code);
                        });
                    }
                    if let Some(on_run_code) = self.on_run_code.clone() {
                        block = block.on_run_code(move |s: &mut S, code: String, language: String| {
                            on_run_code(s, code, language);
                        });
                    }
                    if let Some(on_new_query) = self.on_new_query.clone() {
                        block = block.on_new_query(move |s: &mut S, code: String| {
                            on_new_query(s, code);
                        });
                    }
                    if let Some(on_copy) = self.on_copy_code.clone() {
                        block = block.on_copy(move |s: &mut S, code: String, request: u64| {
                            on_copy(s, code, request);
                        });
                    }
                    let slot = slots.get(idx).cloned().unwrap_or_default();
                    root = root.child(block.build_with_state(&slot, dark, layer));
                }
            }
        }
        root
    }

    /// One block element: paragraph, heading, list, quote or rule.
    /// `n` is the block's ordinal within its text segment — every key in
    /// the tree carries it, so siblings never collide.
    fn block_el(&self, k: &Kit, block: Block, n: usize) -> El {
        let ctx = InlineCtx {
            k,
            citations: self.memory_references.as_deref(),
        };
        match block {
            Block::Paragraph(src) => {
                let el = self.text_el(k, inline_parse(&src, &ctx));
                el.key(format!("{}-p-{n}", self.key))
            }
            Block::Heading { level, text: src } => {
                let inlined = inline_parse(&src, &ctx);
                let font = match level {
                    1 => k.ty.text_small,
                    2 => k.ty.text_small_medium,
                    _ => k.ty.text_xsmall,
                };
                // React `prose` headings are bold (see the module docs for
                // the size divergence).
                let start = 0usize;
                let end = inlined.content.len();
                let mut spans = inlined.spans;
                spans.push(Span::new(start, end).bold());
                self.text_el(
                    k,
                    Inlined { content: inlined.content, spans, links: inlined.links },
                )
                .font(font)
                .key(format!("{}-h{level}-{n}", self.key))
            }
            Block::Bullet(items) => {
                let mut list = col()
                    .key(format!("{}-bullet-{n}", self.key))
                    .min_w(0.0)
                    .w_full()
                    .gap(2.0);
                for (item_n, item) in items.into_iter().enumerate() {
                    list = list.child(self.list_row(k, "•".to_owned(), item, &ctx, n, item_n));
                }
                list
            }
            Block::Ordered { first, items } => {
                let mut list = col()
                    .key(format!("{}-ordered-{n}", self.key))
                    .min_w(0.0)
                    .w_full()
                    .gap(2.0);
                for (item_n, item) in items.into_iter().enumerate() {
                    // GFM: the numbers after the first increment, as the
                    // browser's <ol start> rendering does.
                    let number = first + item_n as u64;
                    list = list.child(self.list_row(
                        k,
                        format!("{number}."),
                        item,
                        &ctx,
                        n,
                        item_n,
                    ));
                }
                list
            }
            Block::Quote(src) => {
                // No single-side border in Rune; the block indents and the
                // text mutes, as `ChatBubble`'s reasoning section does.
                let inlined = inline_parse(&src, &ctx);
                let quoted = self
                    .text_el(k, inlined)
                    .color(k.c.text_muted)
                    .key(format!("{}-q-{n}", self.key));
                col()
                    .key(format!("{}-quote-{n}", self.key))
                    .min_w(0.0)
                    .w_full()
                    .pl(12.0)
                    .child(quoted)
            }
            Block::Rule => col()
                .key(format!("{}-rule-{n}", self.key))
                .w_full()
                .h(1.0)
                .fill(k.c.border_base),
        }
    }

    /// One list row: the muted marker plus the inline-parsed item text.
    fn list_row(
        &self,
        k: &Kit,
        marker: String,
        item: String,
        ctx: &InlineCtx<'_>,
        block_n: usize,
        item_n: usize,
    ) -> El {
        let label = self.text_el(k, inline_parse(&item, ctx));
        row()
            .key(format!("{}-li-{block_n}-{item_n}", self.key))
            .min_w(0.0)
            .w_full()
            .gap(6.0)
            .child(
                text(marker)
                    .key(format!("{}-li-marker-{block_n}-{item_n}", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(label.key(format!("{}-li-label-{block_n}-{item_n}", self.key)))
    }

    /// One inline-parsed text element with its spans, links and the
    /// citation / link dispatch.
    fn text_el(&self, k: &Kit, inlined: Inlined) -> El {
        let mut el = text(inlined.content)
            .font(k.ty.text_xsmall)
            .color(k.c.text_base)
            .wrap();
        if !inlined.spans.is_empty() {
            el = el.spans(inlined.spans);
        }
        for link in inlined.links {
            el = el.link(link);
        }
        if self.on_cite_memory.is_some() || self.on_open_link.is_some() {
            let on_cite = self.on_cite_memory.clone();
            let on_open = self.on_open_link.clone();
            el = el.on_link(move |s: &mut S, id: &str, _tx: &mut Tx| {
                if let Some(reference) = id.strip_prefix("memory-") {
                    if let Some(on_cite) = on_cite.clone() {
                        on_cite(s, reference.to_owned());
                    }
                } else if let Some(on_open) = on_open.clone() {
                    on_open(s, id.to_owned());
                }
            });
        }
        el
    }
}
