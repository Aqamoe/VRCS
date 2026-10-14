#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Span {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Kind {
    #[default]
    Paragraph,
    Heading(u8),
    Code,
    Rule,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Block {
    pub kind: Kind,
    pub indent: usize,
    pub quote_depth: usize,
    pub prefix: String,
    pub spans: Vec<Span>,
}

pub(super) type Document = Vec<Block>;

pub(super) fn parse(source: &str) -> Result<Document, &'static str> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    if source.len() > 65536 {
        return Err("source budget");
    }
    let mut document = Vec::new();
    let mut block = Block::default();
    let mut style = Style::default();
    let mut styles = Vec::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut quote_depth = 0;
    let mut depth = 0;
    let mut units = 0;
    let mut table = false;
    let mut head = false;
    let mut headers: Vec<String> = Vec::new();
    let mut cells: Vec<Vec<Span>> = Vec::new();
    let mut cell: Vec<Span> = Vec::new();
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(source, options) {
        let inline_code = matches!(&event, Event::Code(_));
        match event {
            Event::Start(tag) => {
                depth += 1;
                if depth > 32 {
                    return Err("nesting budget");
                }
                match tag {
                    Tag::Paragraph => {
                        flush(&mut document, &mut block);
                        block.indent = lists.len().saturating_sub(1);
                        block.quote_depth = quote_depth;
                    }
                    Tag::Heading { level, .. } => {
                        flush(&mut document, &mut block);
                        block.kind = Kind::Heading(level as u8);
                        block.indent = lists.len().saturating_sub(1);
                        block.quote_depth = quote_depth;
                    }
                    Tag::CodeBlock(_) => {
                        flush(&mut document, &mut block);
                        block.kind = Kind::Code;
                        block.indent = lists.len().saturating_sub(1);
                        block.quote_depth = quote_depth;
                    }
                    Tag::BlockQuote(_) => {
                        flush(&mut document, &mut block);
                        quote_depth += 1;
                    }
                    Tag::List(start) => {
                        flush(&mut document, &mut block);
                        lists.push(start);
                    }
                    Tag::Item => {
                        flush(&mut document, &mut block);
                        block.indent = lists.len().saturating_sub(1);
                        block.quote_depth = quote_depth;
                        block.prefix = match lists.last_mut() {
                            Some(Some(number)) => {
                                let prefix = format!("{number}. ");
                                *number += 1;
                                prefix
                            }
                            _ => "- ".into(),
                        };
                    }
                    Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } => {
                        styles.push(style);
                        match tag {
                            Tag::Emphasis => style.italic = true,
                            Tag::Strong => style.bold = true,
                            Tag::Strikethrough => style.strike = true,
                            _ => style.link = true,
                        }
                    }
                    Tag::Table(_) => {
                        flush(&mut document, &mut block);
                        table = true;
                        headers.clear();
                    }
                    Tag::TableHead => {
                        head = true;
                        cells.clear();
                    }
                    Tag::TableRow => cells.clear(),
                    Tag::TableCell => cell.clear(),
                    _ => {}
                }
            }
            Event::End(tag) => {
                depth -= 1;
                match tag {
                    TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::Item => {
                        flush(&mut document, &mut block)
                    }
                    TagEnd::BlockQuote(_) => {
                        flush(&mut document, &mut block);
                        quote_depth -= 1;
                    }
                    TagEnd::List(_) => {
                        flush(&mut document, &mut block);
                        lists.pop();
                    }
                    TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                        style = styles.pop().unwrap_or_default()
                    }
                    TagEnd::TableCell => cells.push(std::mem::take(&mut cell)),
                    TagEnd::TableHead => {
                        headers = cells
                            .iter()
                            .map(|spans| spans.iter().map(|span| span.text.as_str()).collect())
                            .collect();
                        head = false;
                    }
                    TagEnd::TableRow if !head => {
                        for (index, spans) in cells.drain(..).enumerate() {
                            let label = headers
                                .get(index)
                                .filter(|label| !label.is_empty())
                                .cloned()
                                .unwrap_or_else(|| (index + 1).to_string());
                            let mut spans_with_label = vec![Span {
                                text: format!("{label}: "),
                                style: Style::default(),
                            }];
                            spans_with_label.extend(spans);
                            document.push(Block {
                                spans: spans_with_label,
                                indent: lists.len().saturating_sub(1),
                                quote_depth,
                                ..Block::default()
                            });
                            units += 2;
                        }
                    }
                    TagEnd::Table => {
                        // A header-only table still has visible cells.
                        for spans in cells.drain(..) {
                            document.push(Block {
                                spans,
                                indent: lists.len().saturating_sub(1),
                                quote_depth,
                                ..Block::default()
                            });
                        }
                        table = false;
                    }
                    _ => {}
                }
            }
            Event::Text(text) | Event::Code(text) => {
                let mut span_style = style;
                // Code events and code blocks bypass Markdown interpretation.
                span_style.code = block.kind == Kind::Code || inline_code;
                let target = if table { &mut cell } else { &mut block.spans };
                target.push(Span {
                    text: text.into_string(),
                    style: span_style,
                });
                units += 1;
            }
            Event::SoftBreak | Event::HardBreak => {
                let text = if matches!(event, Event::HardBreak) {
                    "\n"
                } else {
                    " "
                };
                let target = if table { &mut cell } else { &mut block.spans };
                target.push(Span {
                    text: text.into(),
                    style,
                });
                units += 1;
            }
            Event::TaskListMarker(checked) => {
                block.prefix = if checked { "[x] " } else { "[ ] " }.into()
            }
            Event::Rule => {
                flush(&mut document, &mut block);
                document.push(Block {
                    kind: Kind::Rule,
                    indent: lists.len().saturating_sub(1),
                    quote_depth,
                    ..Block::default()
                });
            }
            _ => {}
        }
        if units + document.len() > 8192 {
            return Err("structure budget");
        }
    }
    flush(&mut document, &mut block);
    Ok(document)
}

fn flush(document: &mut Document, block: &mut Block) {
    if !block.spans.is_empty() || block.kind == Kind::Code {
        document.push(std::mem::take(block));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(document: &[Block]) -> String {
        document
            .iter()
            .flat_map(|block| &block.spans)
            .map(|span| span.text.as_str())
            .collect()
    }

    #[test]
    fn parses_nested_styles_and_literal_code() {
        let document = parse("## Title\n\n**bold *both* ~~gone~~** and `*literal*`\n\n```html\n  <b>**code**</b>\n\nend\n```\n").unwrap();
        assert_eq!(document[0].kind, Kind::Heading(2));
        assert!(document[1]
            .spans
            .iter()
            .any(|span| span.text == "both" && span.style.bold && span.style.italic));
        assert!(document[1]
            .spans
            .iter()
            .any(|span| span.text == "gone" && span.style.bold && span.style.strike));
        assert!(document[1]
            .spans
            .iter()
            .any(|span| span.text == "*literal*" && span.style.code));
        assert_eq!(document[2].kind, Kind::Code);
        assert_eq!(text(&document[2..]), "  <b>**code**</b>\n\nend\n");
    }

    #[test]
    fn lists_and_tables_keep_all_visible_values() {
        let document = parse("3. first\n   - nested\n4. last\n\n- [x] done\n- [ ] pending\n\n| Name | Value |\n| --- | --- |\n| A | **one** |\n| B | two |\n").unwrap();
        assert_eq!(document[0].prefix, "3. ");
        assert!(document
            .iter()
            .any(|block| block.indent == 1 && text(std::slice::from_ref(block)) == "nested"));
        assert!(document.iter().any(|block| block.prefix == "4. "));
        assert!(document.iter().any(|block| block.prefix == "[x] "));
        assert!(document.iter().any(|block| block.prefix == "[ ] "));
        let visible = text(&document);
        assert!(visible.contains("Name: A") && visible.contains("Value: one"));
        assert!(visible.contains("Name: B") && visible.contains("Value: two"));
        assert_eq!(
            document
                .iter()
                .flat_map(|block| &block.spans)
                .filter(|span| span.text == "one")
                .count(),
            1
        );
    }

    #[test]
    fn html_and_images_keep_only_visible_text() {
        let document = parse("[label](https://example.com) ![Remote image](https://example.com/a.png)\n\n<script>alert(1)</script>\n\nvisible <b>word</b> `<>`\n").unwrap();
        assert_eq!(text(&document), "label Remote imagevisible word <>");
        assert!(document[0]
            .spans
            .iter()
            .any(|span| span.text == "label" && span.style.link));
        assert!(!text(&document).contains("https://") && !text(&document).contains("alert"));
    }

    #[test]
    fn header_only_tables_and_empty_images_preserve_visible_content() {
        let document =
            parse("| **Name** | Value |\n| --- | --- |\n\n![](https://example.com/image.png)")
                .unwrap();
        assert_eq!(text(&document), "NameValue");
        assert!(document[0].spans[0].style.bold);
    }

    #[test]
    fn malformed_input_and_limits_do_not_lose_the_answer() {
        assert!(text(&parse("unfinished **bold\n\n```\ncode").unwrap()).contains("code"));
        assert!(parse(&"a".repeat(65537)).is_err());
        assert!(parse(&format!("{}deep", "> ".repeat(33))).is_err());
        assert!(parse(&"x\n\n".repeat(5000)).is_err());
    }
}
