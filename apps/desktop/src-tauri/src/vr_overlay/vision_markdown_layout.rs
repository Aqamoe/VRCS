use unicode_segmentation::UnicodeSegmentation;

use super::text_raster::FontSpec;
use super::vision_markdown::{Block, Document, Kind, Span, Style};

pub(super) trait Metrics {
    fn width(&self, text: &str, font: FontSpec) -> i32;
    fn height(&self, font: FontSpec) -> i32;
    fn ascent(&self, font: FontSpec) -> i32;
}

#[derive(Debug)]
pub(super) struct Run {
    pub text: String,
    pub font: FontSpec,
    pub style: Style,
    pub x: i32,
    pub width: i32,
}

#[derive(Debug)]
pub(super) struct Line {
    pub runs: Vec<Run>,
    pub y: i32,
    pub height: i32,
    pub ascent: i32,
    pub left: i32,
    pub quote_depth: usize,
    pub kind: Kind,
}

pub(super) struct Layout {
    pub pages: Vec<Vec<Line>>,
}

pub(super) fn font_for(kind: Kind, style: Style, size: u32) -> FontSpec {
    let mut font = FontSpec::body(size);
    if let Kind::Heading(level) = kind {
        let scale = match level {
            1 => 135,
            2 => 120,
            _ => 110,
        };
        font.size = (font.size * scale + 99) / 100;
        font.weight = 700;
    }
    if style.bold {
        font.weight = 700;
    }
    font.italic = style.italic;
    font.strike = style.strike;
    font.monospace = style.code || kind == Kind::Code;
    font
}

pub(super) fn layout(
    document: &Document,
    size: u32,
    width: i32,
    height: i32,
    metrics: &impl Metrics,
) -> Result<Layout, &'static str> {
    let base = (metrics.height(FontSpec::body(size)) * 135 + 99) / 100;
    let gap = (base * 35 + 99) / 100;
    let mut blocks = Vec::new();
    for block in document {
        blocks.push(wrap(block, size, width, metrics)?);
    }
    let mut pages: Vec<Vec<Line>> = vec![Vec::new()];
    let mut y = 0;
    for (index, lines) in blocks.iter_mut().enumerate() {
        let block_height: i32 = lines.iter().map(|line| line.height).sum();
        let next_height = if matches!(document[index].kind, Kind::Heading(_)) {
            document
                .get(index + 1)
                .map(|block| {
                    std::iter::once(font_for(block.kind, Style::default(), size))
                        .chain(
                            block
                                .spans
                                .iter()
                                .map(|span| font_for(block.kind, span.style, size)),
                        )
                        .map(|font| (metrics.height(font) * 135 + 99) / 100)
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0)
        } else {
            0
        };
        let keep_height = block_height
            + if next_height > 0 {
                gap + next_height
            } else {
                0
            };
        if y > 0 && keep_height <= height && y + gap + keep_height > height {
            pages.push(Vec::new());
            y = 0;
        }
        if y > 0 {
            y += gap;
        }
        for mut line in lines.drain(..) {
            if line.height > height {
                return Err("line exceeds page");
            }
            if y + line.height > height {
                pages.push(Vec::new());
                y = 0;
            }
            line.y = y;
            y += line.height;
            pages.last_mut().unwrap().push(line);
        }
    }
    Ok(Layout { pages })
}

fn wrap(
    block: &Block,
    size: u32,
    width: i32,
    metrics: &impl Metrics,
) -> Result<Vec<Line>, &'static str> {
    let default_font = font_for(block.kind, Style::default(), size);
    let base = (metrics.height(FontSpec::body(size)) * 135 + 99) / 100;
    let left = (block.indent + block.quote_depth) as i32 * base
        + if block.kind == Kind::Code { 8 } else { 0 };
    let prefix_width = metrics.width(&block.prefix, default_font);
    let start = left + prefix_width;
    let right = width - if block.kind == Kind::Code { 8 } else { 0 };
    if start >= right {
        return Err("indent exceeds width");
    }
    let new_line = || Line {
        runs: Vec::new(),
        y: 0,
        height: (metrics.height(default_font) * 135 + 99) / 100,
        ascent: metrics.ascent(default_font),
        left,
        quote_depth: block.quote_depth,
        kind: block.kind,
    };
    let mut lines = Vec::new();
    let mut line = new_line();
    if !block.prefix.is_empty() {
        line.runs.push(Run {
            text: block.prefix.clone(),
            font: default_font,
            style: Style::default(),
            x: left,
            width: prefix_width,
        });
    }
    let mut x = start;
    let spans = block.spans.iter().map(|span| Span {
        text: if block.kind == Kind::Code {
            span.text.replace('\t', "    ")
        } else {
            span.text.clone()
        },
        style: span.style,
    });
    for span in spans {
        let font = font_for(block.kind, span.style, size);
        // Word boundaries preserve CJK text and spaces; oversized words split only at graphemes.
        for token in span.text.split_word_bounds() {
            if token == "\n" || token == "\r\n" {
                lines.push(line);
                line = new_line();
                x = start;
                continue;
            }
            let token_width = metrics.width(token, font);
            if x > start && x + token_width > right {
                lines.push(line);
                line = new_line();
                x = start;
            }
            let previous = line.runs.last().filter(|run| {
                run.font == font && run.style == span.style && run.x + run.width == x
            });
            let added_width = previous
                .map(|run| metrics.width(&(run.text.clone() + token), font) - run.width)
                .unwrap_or(token_width);
            if x + added_width <= right {
                x = append(&mut line, token, font, span.style, x, added_width, metrics);
                continue;
            }
            for grapheme in token.graphemes(true) {
                let previous = line.runs.last().filter(|run| {
                    run.font == font && run.style == span.style && run.x + run.width == x
                });
                let mut extra_width = previous
                    .map(|run| metrics.width(&(run.text.clone() + grapheme), font) - run.width)
                    .unwrap_or_else(|| metrics.width(grapheme, font));
                if x > start && x + extra_width > right {
                    lines.push(line);
                    line = new_line();
                    x = start;
                    extra_width = metrics.width(grapheme, font);
                }
                x = append(
                    &mut line,
                    grapheme,
                    font,
                    span.style,
                    x,
                    extra_width,
                    metrics,
                );
            }
        }
    }
    if !line.runs.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    Ok(lines)
}

fn append(
    line: &mut Line,
    text: &str,
    font: FontSpec,
    style: Style,
    x: i32,
    width: i32,
    metrics: &impl Metrics,
) -> i32 {
    line.height = line.height.max((metrics.height(font) * 135 + 99) / 100);
    line.ascent = line.ascent.max(metrics.ascent(font));
    if let Some(run) = line
        .runs
        .last_mut()
        .filter(|run| run.font == font && run.style == style && run.x + run.width == x)
    {
        run.text.push_str(text);
        run.width += width;
        run.x + run.width
    } else {
        line.runs.push(Run {
            text: text.into(),
            font,
            style,
            x,
            width,
        });
        x + width
    }
}

#[cfg(test)]
mod tests {
    use super::super::vision_markdown::parse;
    use super::*;

    struct Fixed;
    impl Metrics for Fixed {
        fn width(&self, text: &str, font: FontSpec) -> i32 {
            text.graphemes(true).count() as i32 * font.size
        }
        fn height(&self, font: FontSpec) -> i32 {
            font.size
        }
        fn ascent(&self, font: FontSpec) -> i32 {
            font.size * 3 / 4
        }
    }
    fn visible(layout: &Layout) -> String {
        layout
            .pages
            .iter()
            .flatten()
            .flat_map(|line| &line.runs)
            .map(|run| run.text.as_str())
            .collect()
    }

    #[test]
    fn mixed_styles_share_lines_and_graphemes_survive_page_breaks() {
        let source = "**bold** plain *italic* 中文 e\u{301} 👨‍👩‍👧‍👦";
        let document = parse(source).unwrap();
        let result = layout(&document, 10, 100, 28, &Fixed).unwrap();
        assert!(result.pages.len() > 1);
        assert!(result.pages[0][0].runs.len() > 1);
        assert_eq!(visible(&result), "bold plain italic 中文 e\u{301} 👨‍👩‍👧‍👦");
        assert!(result
            .pages
            .iter()
            .flatten()
            .flat_map(|line| &line.runs)
            .any(|run| run.text.contains("👨‍👩‍👧‍👦")));
        assert!(result
            .pages
            .iter()
            .flatten()
            .all(|line| line.y + line.height <= 28));
    }

    #[test]
    fn code_blank_lines_indentation_and_continuations_survive() {
        let document = parse("> ```\n>   first\n>\n>   longlonglonglong\n> ```\n").unwrap();
        let result = layout(&document, 10, 100, 28, &Fixed).unwrap();
        assert_eq!(visible(&result), "  first  longlonglonglong");
        assert!(result
            .pages
            .iter()
            .flatten()
            .any(|line| line.runs.is_empty()));
        assert!(result
            .pages
            .iter()
            .flatten()
            .all(|line| line.quote_depth == 1 && line.kind == Kind::Code && line.left == 22));
    }

    #[test]
    fn headings_stay_with_content_and_small_blocks_stay_together() {
        let document = parse("one\n\n## title\n\nbody\n\nlast").unwrap();
        let result = layout(&document, 10, 200, 50, &Fixed).unwrap();
        let heading_page = result
            .pages
            .iter()
            .find(|page| {
                page.iter()
                    .any(|line| matches!(line.kind, Kind::Heading(_)))
            })
            .unwrap();
        assert!(heading_page
            .iter()
            .flat_map(|line| &line.runs)
            .any(|run| run.text == "body"));
        assert!(layout(
            &parse(&format!("{}text", "> ".repeat(10))).unwrap(),
            10,
            20,
            50,
            &Fixed
        )
        .is_err());
    }

    #[test]
    fn long_blocks_make_progress_at_all_font_sizes() {
        let text = format!("{} e\u{301} 👨‍👩‍👧‍👦", "wordwithoutspaces".repeat(200));
        let document = parse(&text).unwrap();
        for size in [18, 32, 72] {
            let result = layout(&document, size, 880, 444, &Fixed).unwrap();
            assert_eq!(visible(&result), text);
            assert!(result.pages.len() > 1);
            assert!(result.pages.iter().all(|page| !page.is_empty()));
            for line in result.pages.iter().flatten() {
                assert!(line.y + line.height <= 444);
                assert!(line
                    .runs
                    .iter()
                    .all(|run| run.x >= 0 && run.x + run.width <= 880));
            }
        }
        let oversized = layout(&parse("界界").unwrap(), 32, 1, 44, &Fixed).unwrap();
        assert_eq!(visible(&oversized), "界界");
        assert_eq!(oversized.pages.len(), 2);
    }
}
