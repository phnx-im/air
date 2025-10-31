// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{iter::Peekable, sync::LazyLock};

use flutter_rust_bridge::frb;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;

const MAX_DEPTH: usize = 50;

pub(crate) static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    // The non-protocol part is split into two:
    // * Any number of allowed characters except the last character
    // * The last character must be allowed and not .
    Regex::new(
        "(mailto:|https:|http:)\
        [^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`\\\\]*\
        [^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`\\\\.]",
    )
    .unwrap()
});

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum Error {
    #[error("expected more events")]
    ExpectedMoreEvents,
    #[error("expected specific tag")]
    ExpectedSpecificTag,
    #[error("table content not in table")]
    TableContentNotInTable,
    #[error("list item not in list")]
    ListItemNotInList,
    #[error("metadata blocks not supported")]
    MetadataBlocksNotSupported,
    #[error("footnotes not supported")]
    FootnotesNotSupported,
    #[error("definition lists not supported")]
    DefinitionListsNotSupported,
    #[error("block element inline")]
    BlockElementInline,
    #[error("HTML not in block")]
    HtmlNotInBlock,
    #[error("math not supported")]
    MathNotSupported,
    #[error("depth limit reached")]
    DepthLimitReached,
    #[error("invalid UTF-8")]
    InvalidUtf8,
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct MessageContent {
    pub elements: Vec<RangedBlockElement>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct RangedInlineElement {
    pub start: u32,
    pub end: u32,
    pub element: InlineElement,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct RangedBlockElement {
    pub start: u32,
    pub end: u32,
    pub element: BlockElement,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub struct RangedCodeBlock {
    pub start: u32,
    pub end: u32,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
#[frb(dart_metadata = ("freezed"))]
pub struct RangedEvent<'a> {
    pub start: u32,
    pub end: u32,
    pub event: Event<'a>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub enum BlockElement {
    Paragraph(Vec<RangedInlineElement>),
    Heading(Vec<RangedInlineElement>),
    Quote(Vec<RangedBlockElement>),
    UnorderedList(Vec<Vec<RangedBlockElement>>), // Each item has multiple block elements
    OrderedList(u64, Vec<Vec<RangedBlockElement>>), // Each item has multiple block elements
    Table {
        head: Vec<Vec<RangedBlockElement>>,
        rows: Vec<Vec<Vec<RangedBlockElement>>>,
    },
    HorizontalRule,

    /// If code blocks are indented, each line is a separate String
    CodeBlock(Vec<RangedCodeBlock>),

    Error(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[frb(dart_metadata = ("freezed"))]
pub enum InlineElement {
    Text(String),
    Code(String),
    Link {
        dest_url: String,
        children: Vec<RangedInlineElement>,
    },
    Bold(Vec<RangedInlineElement>),
    Italic(Vec<RangedInlineElement>),
    Strikethrough(Vec<RangedInlineElement>),
    Spoiler(Vec<RangedInlineElement>),
    Image(String),
    TaskListMarker(bool),
    //UserMention(String),
    //RoomMention(String),
    //Video,
    //Audio,
    //Voice,
    //Meeting,
    //File,
}

impl MessageContent {
    pub fn error(message: String) -> Self {
        Self {
            elements: vec![RangedBlockElement {
                start: 0,
                end: u32::try_from(message.chars().count()).unwrap_or(u32::MAX),
                element: BlockElement::Error(message),
            }],
        }
    }

    #[frb(sync)]
    pub fn parse_markdown_raw(string: Vec<u8>) -> Result<Self> {
        Self::try_parse_markdown(&String::from_utf8(string).map_err(|_| Error::InvalidUtf8)?)
    }

    pub fn parse_markdown(string: &str) -> Self {
        Self::try_parse_markdown(string)
            .unwrap_or_else(|e| Self::error(format!("Invalid message: {e}")))
    }

    fn try_parse_markdown(string: &str) -> Result<Self> {
        let parsed = Parser::new_ext(
            string,
            // Do not enable Options::ENABLE_GFM, it activates special blockquotes which are not part of the GFM spec https://github.com/orgs/community/discussions/16925
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
        )
        .into_offset_iter();
        let mut result = Vec::new();
        let mut iter = parsed
            .map(|(event, range)| RangedEvent {
                start: u32::try_from(range.start).unwrap_or(u32::MAX),
                end: u32::try_from(range.end).unwrap_or(u32::MAX),
                event,
            })
            .peekable();

        while iter.peek().is_some() {
            result.push(parse_block_element(&mut iter, 1)?);
        }

        Ok(Self { elements: result })
    }
}

fn parse_block_element<'a, I>(iter: &mut Peekable<I>, depth: usize) -> Result<RangedBlockElement>
where
    I: Iterator<Item = RangedEvent<'a>>,
{
    if depth > MAX_DEPTH {
        return Err(Error::DepthLimitReached);
    }

    let peek = iter.peek().ok_or(Error::ExpectedMoreEvents)?;
    let block = match peek.clone().event {
        Event::Start(Tag::Paragraph) => {
            let start = iter.next().expect("we already peeked");
            let value = BlockElement::Paragraph(parse_inline_elements(iter, depth + 1)?);
            let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

            if end.event != Event::End(TagEnd::Paragraph) {
                return Err(Error::ExpectedSpecificTag);
            }

            RangedBlockElement {
                start: start.start,
                end: end.end,
                element: value,
            }
        }
        Event::Start(Tag::Heading { level, .. }) => {
            let start = iter.next().expect("we already peeked");
            let value = BlockElement::Heading(parse_inline_elements(iter, depth + 1)?);
            let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

            if end.event != Event::End(TagEnd::Heading(level)) {
                return Err(Error::ExpectedSpecificTag);
            }

            RangedBlockElement {
                start: start.start,
                end: end.end,
                element: value,
            }
        }
        Event::Start(Tag::List(number)) => {
            let start = iter.next().expect("we already peeked");
            let value = match number {
                Some(s) => BlockElement::OrderedList(s, parse_list_items(iter, depth + 1)?),
                None => BlockElement::UnorderedList(parse_list_items(iter, depth + 1)?),
            };
            let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

            if end.event != Event::End(TagEnd::List(number.is_some())) {
                return Err(Error::ExpectedSpecificTag);
            }

            RangedBlockElement {
                start: start.start,
                end: end.end,
                element: value,
            }
        }
        Event::Start(Tag::Table(_alignments)) => {
            let start = iter.next().expect("we already peeked");
            let value = parse_table_content(iter, depth + 1)?;
            let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

            if end.event != Event::End(TagEnd::Table) {
                return Err(Error::ExpectedSpecificTag);
            }

            RangedBlockElement {
                start: start.start,
                end: end.end,
                element: value,
            }
        }
        Event::Start(Tag::BlockQuote(_)) => {
            let start = iter.next().expect("we already peeked");
            let mut quote_blocks = Vec::new();
            let end;
            loop {
                let peek = iter.peek().ok_or(Error::ExpectedMoreEvents)?;
                if matches!(peek.event, Event::End(TagEnd::BlockQuote(..))) {
                    end = iter.next().expect("we already peeked");
                    break;
                }
                quote_blocks.push(parse_block_element(iter, depth + 1)?);
            }

            RangedBlockElement {
                start: start.start,
                end: end.end,
                element: BlockElement::Quote(quote_blocks),
            }
        }
        Event::Start(Tag::CodeBlock(_code_block_kind)) => {
            let start = iter.next().expect("we already peeked");
            let mut value = Vec::new();

            while let Event::Text(str) = iter.peek().ok_or(Error::ExpectedMoreEvents)?.clone().event
            {
                let event = iter.next().expect("we already peeked");

                // We need this code, otherwise there is an empty line at the end of code blocks
                let mut str = str.into_string();
                if str.ends_with('\n') {
                    str.truncate(str.len() - 1);
                }

                value.push(RangedCodeBlock {
                    start: event.start,
                    end: event.end,
                    value: str.to_string(),
                });
            }

            // A code block cannot contain any other data
            let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

            if end.event != Event::End(TagEnd::CodeBlock) {
                return Err(Error::ExpectedSpecificTag);
            }

            RangedBlockElement {
                start: start.start,
                end: end.end,
                element: BlockElement::CodeBlock(value),
            }
        }
        Event::Rule => {
            let item = iter.next().expect("we already peeked");
            let value = BlockElement::HorizontalRule;

            RangedBlockElement {
                start: item.start,
                end: item.end,
                element: value,
            }
        }

        // Create implicit paragraph for inline elements
        Event::Start(Tag::Emphasis)
        | Event::Start(Tag::Strong)
        | Event::Start(Tag::Strikethrough)
        | Event::Start(Tag::Link { .. })
        | Event::Start(Tag::Image { .. })
        | Event::Text(_)
        | Event::InlineHtml(_)
        | Event::Code(_)
        | Event::TaskListMarker(_)
        | Event::SoftBreak
        | Event::HardBreak => {
            let inner = parse_inline_elements(iter, depth + 1)?;
            RangedBlockElement {
                start: inner[0].start,
                end: inner[inner.len() - 1].end,
                element: BlockElement::Paragraph(inner),
            }
        }

        // The rest are invalid events
        Event::InlineMath(_) | Event::DisplayMath(_) => {
            return Err(Error::MathNotSupported);
        }

        Event::Start(Tag::HtmlBlock) => {
            let start = iter.next().expect("we already peeked");
            let mut value = Vec::new();

            while let Event::Html(str) | Event::Text(str) =
                iter.peek().ok_or(Error::ExpectedMoreEvents)?.clone().event
            {
                let event = iter.next().expect("we already peeked");
                collect_links(event.start, event.end, &str, &mut value);
            }

            // A code block cannot contain any other data
            let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

            if end.event != Event::End(TagEnd::HtmlBlock) {
                return Err(Error::ExpectedSpecificTag);
            }

            RangedBlockElement {
                start: start.start,
                end: end.end,
                element: BlockElement::Paragraph(value),
            }
        }

        Event::Html(_) => {
            return Err(Error::HtmlNotInBlock);
        }

        Event::Start(Tag::Item) => return Err(Error::ListItemNotInList),

        Event::Start(Tag::FootnoteDefinition(_)) | Event::FootnoteReference(_) => {
            return Err(Error::FootnotesNotSupported);
        }

        Event::Start(Tag::Superscript | Tag::Subscript) => return Err(Error::MathNotSupported),

        Event::Start(Tag::DefinitionList)
        | Event::Start(Tag::DefinitionListTitle)
        | Event::Start(Tag::DefinitionListDefinition) => {
            return Err(Error::DefinitionListsNotSupported);
        }

        Event::Start(Tag::TableHead)
        | Event::Start(Tag::TableRow)
        | Event::Start(Tag::TableCell) => return Err(Error::TableContentNotInTable),

        Event::Start(Tag::MetadataBlock(_)) => {
            return Err(Error::MetadataBlocksNotSupported);
        }

        Event::End(_) => return Err(Error::ExpectedSpecificTag),
    };

    Ok(block)
}

fn parse_inline_elements<'a, I>(
    iter: &mut Peekable<I>,
    depth: usize,
) -> Result<Vec<RangedInlineElement>>
where
    I: Iterator<Item = RangedEvent<'a>>,
{
    if depth > MAX_DEPTH {
        return Err(Error::DepthLimitReached);
    }

    let mut result = Vec::new();
    loop {
        let peek = iter.peek().ok_or(Error::ExpectedMoreEvents)?;
        match peek.clone().event {
            Event::Start(Tag::Emphasis) => {
                let start = iter.next().expect("we already peeked");
                let value = InlineElement::Italic(parse_inline_elements(iter, depth + 1)?);
                let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

                if end.event != Event::End(TagEnd::Emphasis) {
                    return Err(Error::ExpectedSpecificTag);
                }

                result.push(RangedInlineElement {
                    start: start.start,
                    end: end.end,
                    element: value,
                });
            }

            Event::Start(Tag::Strong) => {
                let start = iter.next().expect("we already peeked");
                let value = InlineElement::Bold(parse_inline_elements(iter, depth + 1)?);
                let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

                if end.event != Event::End(TagEnd::Strong) {
                    return Err(Error::ExpectedSpecificTag);
                }

                result.push(RangedInlineElement {
                    start: start.start,
                    end: end.end,
                    element: value,
                });
            }
            Event::Start(Tag::Strikethrough) => {
                let start = iter.next().expect("we already peeked");
                let value = InlineElement::Strikethrough(parse_inline_elements(iter, depth + 1)?);
                let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

                if end.event != Event::End(TagEnd::Strikethrough) {
                    return Err(Error::ExpectedSpecificTag);
                }

                result.push(RangedInlineElement {
                    start: start.start,
                    end: end.end,
                    element: value,
                });
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                let start = iter.next().expect("we already peeked");
                let value = InlineElement::Link {
                    dest_url: dest_url.to_string(),
                    children: parse_inline_elements(iter, depth + 1)?,
                };
                let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

                if end.event != Event::End(TagEnd::Link) {
                    return Err(Error::ExpectedSpecificTag);
                }

                result.push(RangedInlineElement {
                    start: start.start,
                    end: end.end,
                    element: value,
                });
            }

            Event::Start(Tag::Image { dest_url, .. }) => {
                let start = iter.next().expect("we already peeked");
                let value = InlineElement::Image(dest_url.to_string());

                let _description = parse_inline_elements(iter, depth + 1)?;

                let end = iter.next().ok_or(Error::ExpectedMoreEvents)?;

                if end.event != Event::End(TagEnd::Image) {
                    return Err(Error::ExpectedSpecificTag);
                }

                result.push(RangedInlineElement {
                    start: start.start,
                    end: end.end,
                    element: value,
                });
            }

            Event::Text(str) => {
                let value = iter.next().expect("we already peeked");
                collect_links(value.start, value.end, &str, &mut result);
            }

            Event::Code(str) => {
                let value = iter.next().expect("we already peeked");
                result.push(RangedInlineElement {
                    start: value.start,
                    end: value.end,
                    element: InlineElement::Code(str.to_string()),
                });
            }

            Event::SoftBreak | Event::HardBreak => {
                let value = iter.next().expect("we already peeked");
                result.push(RangedInlineElement {
                    start: value.start,
                    end: value.end,
                    element: InlineElement::Text("\n".to_owned()),
                });
            }

            Event::TaskListMarker(bool) => {
                let value = iter.next().expect("we already peeked");
                result.push(RangedInlineElement {
                    start: value.start,
                    end: value.end,
                    element: InlineElement::TaskListMarker(bool),
                });
            }

            // This is the end of the container
            Event::End(_) => return Ok(result),

            // Inline HTML should just show as text
            Event::InlineHtml(str) => {
                let value = iter.next().expect("we already peeked");
                result.push(RangedInlineElement {
                    start: value.start,
                    end: value.end,
                    element: InlineElement::Text(str.to_string()),
                });
            }

            // If a block element starts, this inline element has ended
            Event::Start(Tag::Paragraph)
            | Event::Start(Tag::Heading { .. })
            | Event::Start(Tag::BlockQuote(_))
            | Event::Start(Tag::CodeBlock(_))
            | Event::Start(Tag::List(_))
            | Event::Start(Tag::Table(_))
            | Event::Start(Tag::HtmlBlock)
            | Event::Rule
            | Event::Html(_) => return Ok(result),

            // The rest are invalid events
            Event::Start(Tag::TableHead)
            | Event::Start(Tag::TableRow)
            | Event::Start(Tag::TableCell) => return Err(Error::TableContentNotInTable),

            Event::Start(Tag::MetadataBlock(_)) => {
                return Err(Error::MetadataBlocksNotSupported);
            }

            Event::Start(Tag::Item) => return Err(Error::ListItemNotInList),

            Event::Start(Tag::FootnoteDefinition(_)) | Event::FootnoteReference(_) => {
                return Err(Error::FootnotesNotSupported);
            }

            Event::Start(Tag::Superscript | Tag::Subscript) => return Err(Error::MathNotSupported),

            Event::Start(Tag::DefinitionList)
            | Event::Start(Tag::DefinitionListTitle)
            | Event::Start(Tag::DefinitionListDefinition) => {
                return Err(Error::DefinitionListsNotSupported);
            }

            Event::InlineMath(_) | Event::DisplayMath(_) => {
                return Err(Error::MathNotSupported);
            }
        }
    }
}

fn parse_list_items<'a, I>(
    iter: &mut Peekable<I>,
    depth: usize,
) -> Result<Vec<Vec<RangedBlockElement>>>
where
    I: Iterator<Item = RangedEvent<'a>>,
{
    if depth > MAX_DEPTH {
        return Err(Error::DepthLimitReached);
    }

    let mut items = Vec::new();

    loop {
        let peek = iter.peek().ok_or(Error::ExpectedMoreEvents)?;
        match peek.event {
            Event::Start(Tag::Item) => {
                iter.next().expect("we already peeked");
                let mut item_blocks = Vec::new();
                loop {
                    let peek = iter.peek().ok_or(Error::ExpectedMoreEvents)?;
                    if peek.event == Event::End(TagEnd::Item) {
                        iter.next().expect("we already peeked");
                        break;
                    }
                    item_blocks.push(parse_block_element(iter, depth + 1)?);
                }
                items.push(item_blocks);
            }

            // This is the end of the container
            Event::End(_) => return Ok(items),

            _ => return Err(Error::ExpectedSpecificTag),
        }
    }
}

fn parse_table_content<'a, I>(iter: &mut Peekable<I>, depth: usize) -> Result<BlockElement>
where
    I: Iterator<Item = RangedEvent<'a>>,
{
    if depth > MAX_DEPTH {
        return Err(Error::DepthLimitReached);
    }

    if !matches!(
        iter.next(),
        Some(RangedEvent {
            event: Event::Start(Tag::TableHead),
            ..
        })
    ) {
        return Err(Error::ExpectedSpecificTag);
    }

    let table_head = parse_table_cells(iter, depth + 1)?;

    if !matches!(
        iter.next(),
        Some(RangedEvent {
            event: Event::End(TagEnd::TableHead),
            ..
        })
    ) {
        return Err(Error::ExpectedSpecificTag);
    }

    let mut table_rows = Vec::new();

    loop {
        let peek = iter.peek().ok_or(Error::ExpectedMoreEvents)?;
        match peek.event {
            Event::Start(Tag::TableRow) => {
                iter.next().expect("we already peeked");
                let cells = parse_table_cells(iter, depth + 1)?;
                table_rows.push(cells);
                if !matches!(
                    iter.next(),
                    Some(RangedEvent {
                        event: Event::End(TagEnd::TableRow),
                        ..
                    })
                ) {
                    return Err(Error::ExpectedSpecificTag);
                }
            }

            // This is the end of the container
            Event::End(TagEnd::Table) => break,

            _ => return Err(Error::ExpectedSpecificTag),
        }
    }

    Ok(BlockElement::Table {
        head: table_head,
        rows: table_rows,
    })
}

fn parse_table_cells<'a, I>(
    iter: &mut Peekable<I>,
    depth: usize,
) -> Result<Vec<Vec<RangedBlockElement>>>
where
    I: Iterator<Item = RangedEvent<'a>>,
{
    if depth > MAX_DEPTH {
        return Err(Error::DepthLimitReached);
    }

    let mut cells = Vec::new();

    loop {
        let peek = iter.peek().ok_or(Error::ExpectedMoreEvents)?;
        match peek.event {
            Event::Start(Tag::TableCell) => {
                iter.next().expect("we already peeked");
                let mut cell_blocks = Vec::new();
                loop {
                    let peek = iter.peek().ok_or(Error::ExpectedMoreEvents)?;

                    if peek.event == Event::End(TagEnd::TableCell) {
                        iter.next().expect("we already peeked");
                        break;
                    }
                    cell_blocks.push(parse_block_element(iter, depth + 1)?);
                }
                cells.push(cell_blocks);
            }

            // This is the end of the container
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => return Ok(cells),

            _ => return Err(Error::ExpectedSpecificTag),
        }
    }
}

/// Collects links and surrounding text from a string into `elements`.
///
/// If there are no links, a single element with the entire string is added.
fn collect_links(start: u32, end: u32, str: &str, elements: &mut Vec<RangedInlineElement>) {
    let mut last_end = 0;

    for mat in URL_RE.find_iter(str) {
        // Unmatched part before this match
        if mat.start() > last_end {
            let text = str[last_end..mat.start()].to_string();
            elements.push(RangedInlineElement {
                start: start + last_end as u32,
                end: start + mat.start() as u32,
                element: InlineElement::Text(text),
            });
        }

        // Matched link
        let text = mat.as_str().to_string();
        elements.push(RangedInlineElement {
            start: start + mat.start() as u32,
            end: start + mat.end() as u32,
            element: InlineElement::Link {
                dest_url: text.to_string(),
                children: vec![RangedInlineElement {
                    start: start + mat.start() as u32,
                    end: start + mat.end() as u32,
                    element: InlineElement::Text(text),
                }],
            },
        });

        last_end = mat.end();
    }

    // Trailing unmatched part
    if last_end < str.len() {
        let text = str[last_end..].to_string();
        elements.push(RangedInlineElement {
            start: start + last_end as u32,
            end,
            element: InlineElement::Text(text),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_images() {
        MessageContent::try_parse_markdown(r#"![hey *ho*](url)"#).unwrap();
        MessageContent::try_parse_markdown(r#"![![Bad link](img.jpg)](url)"#).unwrap();
    }

    #[test]
    fn raw_html() {
        MessageContent::try_parse_markdown(
            r#"<div><div><p><s>Oh no! Unclosed html tags!

But it ends after the paragraph"#,
        )
        .unwrap();
    }

    #[test]
    fn indented_code_block() {
        MessageContent::try_parse_markdown(
            r#"
    asdf
    asdf"#,
        )
        .unwrap();
    }

    #[test]
    fn max_depth() {
        // Test max depth using nested quotes
        MessageContent::try_parse_markdown(&">".repeat(MAX_DEPTH)).unwrap();
        assert_eq!(
            MessageContent::try_parse_markdown(&">".repeat(MAX_DEPTH + 1)),
            Err(Error::DepthLimitReached)
        );
    }

    #[test]
    fn text_in_html_block() {
        MessageContent::try_parse_markdown(">a<a>").unwrap();
    }

    #[test]
    fn inline_html() {
        MessageContent::try_parse_markdown("|>\n|-\n<Y>").unwrap();
    }

    fn parse_links(str_: &str) -> Vec<RangedInlineElement> {
        let mut elements = Vec::new();
        collect_links(0, str_.len() as u32, str_, &mut elements);
        elements
    }

    #[track_caller]
    fn is_text(elem: &RangedInlineElement, expected: &str, range: (u32, u32)) {
        match &elem.element {
            InlineElement::Text(t) => {
                assert_eq!(t, expected);
                assert_eq!((elem.start, elem.end), range);
            }
            _ => panic!("Expected Text, got {:?}", elem.element),
        }
    }

    #[track_caller]
    fn is_link(elem: &RangedInlineElement, expected: &str, range: (u32, u32)) {
        match &elem.element {
            InlineElement::Link { dest_url, children } => {
                assert_eq!(dest_url, expected);
                assert_eq!((elem.start, elem.end), range);
                assert_eq!(children.len(), 1);
                if let InlineElement::Text(child_text) = &children[0].element {
                    assert_eq!(child_text, expected);
                } else {
                    panic!("Child of Link should be Text");
                }
            }
            _ => panic!("Expected Link, got {:?}", elem.element),
        }
    }

    #[test]
    fn collect_links_no_links() {
        let elems = parse_links("hello world");
        assert_eq!(elems.len(), 1);
        is_text(&elems[0], "hello world", (0, 11));
    }

    #[test]
    fn collect_links_single() {
        let elems = parse_links("Visit https://example.com now!");
        assert_eq!(elems.len(), 3);

        is_text(&elems[0], "Visit ", (0, 6));
        is_link(&elems[1], "https://example.com", (6, 25));
        is_text(&elems[2], " now!", (25, 30));
    }

    #[test]
    fn collect_links_multiple() {
        let elems = parse_links("A https://a.com and https://b.com.");
        dbg!(&elems);
        assert_eq!(elems.len(), 5);

        is_text(&elems[0], "A ", (0, 2));
        is_link(&elems[1], "https://a.com", (2, 15));
        is_text(&elems[2], " and ", (15, 20));
        is_link(&elems[3], "https://b.com", (20, 33)); // note: regex includes trailing '.' due to \S+
        is_text(&elems[4], ".", (33, 34));
    }

    #[test]
    fn collect_links_at_start() {
        let elems = parse_links("https://example.com start");
        assert_eq!(elems.len(), 2);

        is_link(&elems[0], "https://example.com", (0, 19));
        is_text(&elems[1], " start", (19, 25));
    }

    #[test]
    fn collect_links_at_end() {
        let elems = parse_links("end https://example.com");
        assert_eq!(elems.len(), 2);

        is_text(&elems[0], "end ", (0, 4));
        is_link(&elems[1], "https://example.com", (4, 23));
    }

    #[test]
    fn collect_links_adjacent() {
        let elems = parse_links("https://a.comhttps://b.com");
        assert_eq!(elems.len(), 1);
        is_link(&elems[0], "https://a.comhttps://b.com", (0, 26));
    }

    #[test]
    fn extract_link_empty_string() {
        let elems = parse_links("");
        assert!(elems.is_empty());
    }

    #[test]
    fn collect_links_emoji() {
        let elems = parse_links("Check this 🌐 https://example.com 🚀!");
        assert_eq!(elems.len(), 3);
        is_text(&elems[0], "Check this 🌐 ", (0, 16));
        is_link(&elems[1], "https://example.com", (16, 35));
        is_text(&elems[2], " 🚀!", (35, 41));
    }
}
