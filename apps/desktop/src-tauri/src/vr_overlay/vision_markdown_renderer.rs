use std::collections::BTreeMap;
use std::time::Instant;

use windows_sys::Win32::Graphics::Gdi::{DT_NOPREFIX, DT_SINGLELINE};

use super::text_raster::{fill_rounded_rect, FontSpec, Rect, TextMask};
use super::vision_markdown::{parse, Style};
use super::vision_markdown_layout::{font_for, layout, Layout, Metrics};

struct Fonts(BTreeMap<FontSpec, TextMask>);

impl Metrics for Fonts {
    fn width(&self, text: &str, font: FontSpec) -> i32 {
        self.0[&font]
            .measure_size(text, 32767, DT_SINGLELINE | DT_NOPREFIX)
            .0
    }
    fn height(&self, font: FontSpec) -> i32 {
        self.0[&font].line_height()
    }
    fn ascent(&self, font: FontSpec) -> i32 {
        self.0[&font].ascent()
    }
}

struct Answer {
    layout: Layout,
    fonts: Fonts,
}

struct Entry {
    source: String,
    font_size: u32,
    width: i32,
    height: i32,
    answer: Option<Answer>,
}

#[derive(Default)]
pub(super) struct AnswerCache {
    entry: Option<Entry>,
    #[cfg(test)]
    pub(super) preparations: usize,
}

impl AnswerCache {
    pub fn clear(&mut self) {
        self.entry = None;
    }

    pub fn prepare(&mut self, source: &str, font_size: u32, width: i32, height: i32) -> bool {
        if let Some(entry) = &self.entry {
            if entry.source == source
                && entry.font_size == font_size
                && entry.width == width
                && entry.height == height
            {
                return entry.answer.is_some();
            }
        }
        #[cfg(test)]
        {
            self.preparations += 1;
        }
        let started = Instant::now();
        let answer = prepare(source, font_size, width, height);
        if let Err(reason) = &answer {
            tracing::warn!(reason, "VR answer Markdown fallback");
        }
        tracing::debug!(
            elapsed_ms = started.elapsed().as_millis(),
            "VR answer Markdown layout"
        );
        self.entry = Some(Entry {
            source: source.into(),
            font_size,
            width,
            height,
            answer: answer.ok(),
        });
        self.page_count().is_some()
    }

    pub fn page_count(&self) -> Option<usize> {
        Some(self.entry.as_ref()?.answer.as_ref()?.layout.pages.len())
    }

    pub fn disable(&mut self) {
        if let Some(entry) = &mut self.entry {
            entry.answer = None;
        }
    }

    pub fn draw(
        &mut self,
        pixels: &mut [u8],
        size: u32,
        content: Rect,
        page: usize,
        alpha: u8,
    ) -> Result<(), String> {
        let entry = self.entry.as_mut().ok_or("Markdown layout unavailable")?;
        let font_size = entry.font_size;
        let answer = entry.answer.as_mut().ok_or("Markdown layout unavailable")?;
        let lines = &answer.layout.pages[page.min(answer.layout.pages.len() - 1)];
        for line in lines {
            let top = content.top + line.y;
            let bottom = top + line.height;
            if line.kind == super::vision_markdown::Kind::Code {
                fill_rounded_rect(
                    pixels,
                    size,
                    size,
                    Rect::new(content.left + line.left - 8, top, content.right, bottom),
                    0,
                    [24, 27, 32, alpha],
                );
            }
            for level in 0..line.quote_depth {
                let indent = (answer.fonts.height(FontSpec::body(font_size)) * 135 + 99) / 100;
                let x = content.left + level as i32 * indent;
                fill_rounded_rect(
                    pixels,
                    size,
                    size,
                    Rect::new(x, top, x + 3, bottom),
                    0,
                    [120, 143, 160, alpha],
                );
            }
            if line.kind == super::vision_markdown::Kind::Rule {
                fill_rounded_rect(
                    pixels,
                    size,
                    size,
                    Rect::new(
                        content.left + line.left,
                        top + line.height / 2,
                        content.right,
                        top + line.height / 2 + 2,
                    ),
                    0,
                    [100, 112, 125, alpha],
                );
            }
            for run in &line.runs {
                let mask = answer.fonts.0.get_mut(&run.font).unwrap();
                let local_top = line.y + line.ascent - mask.ascent();
                let rect = Rect::new(
                    run.x,
                    local_top,
                    (run.x + run.width + 2).min(content.right - content.left),
                    local_top + mask.line_height(),
                );
                if run.style.code && line.kind != super::vision_markdown::Kind::Code {
                    fill_rounded_rect(
                        pixels,
                        size,
                        size,
                        Rect::new(
                            content.left + rect.left,
                            top,
                            content.left + rect.right,
                            bottom,
                        ),
                        3,
                        [24, 27, 32, alpha],
                    );
                }
                mask.draw(&run.text, rect, DT_SINGLELINE | DT_NOPREFIX, false)?;
                let color = if run.style.link {
                    [240, 198, 140]
                } else {
                    [255; 3]
                };
                mask.blend_into(pixels, size, rect, content.left, content.top, color);
            }
        }
        Ok(())
    }
}

fn prepare(source: &str, font_size: u32, width: i32, height: i32) -> Result<Answer, String> {
    let document = parse(source)?;
    let mut fonts = Fonts(BTreeMap::new());
    let specs =
        std::iter::once(FontSpec::body(font_size)).chain(document.iter().flat_map(|block| {
            std::iter::once(font_for(block.kind, Style::default(), font_size)).chain(
                block
                    .spans
                    .iter()
                    .map(|span| font_for(block.kind, span.style, font_size)),
            )
        }));
    for spec in specs {
        if !fonts.0.contains_key(&spec) {
            if fonts.0.len() >= 64 {
                return Err("font budget".into());
            }
            fonts.0.insert(
                spec,
                TextMask::new_with_style(width as u32, height as u32, spec)?,
            );
        }
    }
    let layout = layout(&document, font_size, width, height, &fonts)?;
    Ok(Answer { layout, fonts })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_reuses_layout_and_remembers_fallback_until_inputs_change() {
        let mut cache = AnswerCache::default();
        assert!(cache.prepare("**answer**", 32, 880, 444));
        assert!(cache.prepare("**answer**", 32, 880, 444));
        assert_eq!(cache.preparations, 1);
        assert!(cache.prepare("**answer**", 40, 880, 444));
        assert_eq!(cache.preparations, 2);
        cache.disable();
        assert!(!cache.prepare("**answer**", 40, 880, 444));
        cache.clear();
        assert!(cache.prepare("**answer**", 40, 880, 444));
        let oversized = "a".repeat(65537);
        assert!(!cache.prepare(&oversized, 32, 880, 444));
        assert!(!cache.prepare(&oversized, 32, 880, 444));
        assert_eq!(cache.preparations, 4);
        assert!(cache.prepare("new answer", 32, 880, 444));
        assert!(cache.prepare("new answer", 32, 800, 400));
        assert_eq!(cache.preparations, 6);
    }

    #[test]
    fn windows_fonts_measure_empty_text_and_mixed_style_baselines() {
        let answer = prepare(
            "# **Title**\n\nNormal **bold** *italic* ~~strike~~ `code` 中文",
            72,
            880,
            444,
        )
        .unwrap();
        for (spec, mask) in &answer.fonts.0 {
            assert_eq!(
                mask.measure_size("", 880, DT_SINGLELINE | DT_NOPREFIX),
                (0, 0)
            );
            assert!(mask.line_height() >= spec.size);
            assert!(mask.ascent() <= mask.line_height());
        }
        for line in answer.layout.pages.iter().flatten() {
            for run in &line.runs {
                assert_eq!(run.width, answer.fonts.width(&run.text, run.font));
                assert!(line.ascent >= answer.fonts.ascent(run.font));
                assert!(
                    line.y + line.ascent - answer.fonts.ascent(run.font)
                        + answer.fonts.height(run.font)
                        <= 444
                );
            }
        }
    }

    #[test]
    fn reports_cold_and_warm_layout_times_without_reparsing() {
        let fixture = include_str!("../../tests/fixtures/vr_vision_markdown.md");
        for target_bytes in [8192, 60000] {
            let source = fixture.repeat(target_bytes / fixture.len());
            let mut cache = AnswerCache::default();
            let started = Instant::now();
            assert!(cache.prepare(&source, 32, 880, 444));
            let cold = started.elapsed();
            let started = Instant::now();
            for _ in 0..20 {
                assert!(cache.prepare(&source, 32, 880, 444));
            }
            eprintln!(
                "Markdown {} bytes: cold {:?}; warm {:?} per lookup; {} pages",
                source.len(),
                cold,
                started.elapsed() / 20,
                cache.page_count().unwrap()
            );
            assert_eq!(cache.preparations, 1);
        }
    }
}
