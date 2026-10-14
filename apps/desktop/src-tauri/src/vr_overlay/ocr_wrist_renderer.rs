use windows_sys::Win32::Graphics::Gdi::{
    DT_CENTER, DT_END_ELLIPSIS, DT_EXPANDTABS, DT_NOPREFIX, DT_SINGLELINE,
};

use super::ocr_wrist::{Action, View, ViewMode};
use super::ocr_wrist_layout::Pages;
use super::renderer::Texture;
use super::text_raster::{fill_rounded_rect, Rect, TextMask};
use super::vision_markdown_renderer::AnswerCache;
use super::wrist_labels::WristLabels;

pub(super) const SIZE: u32 = 1024;
const TEXT_FLAGS: u32 = DT_SINGLELINE | DT_NOPREFIX | DT_EXPANDTABS;
const SOURCE_CARD: Rect = Rect::new(48, 164, 976, 488);
const TRANSLATION_CARD: Rect = Rect::new(48, 504, 976, 828);
const FULL_CARD: Rect = Rect::new(48, 164, 976, 828);

pub struct Button {
    pub action: Action,
    pub bounds: [f32; 4],
}

pub struct Rendered {
    pub texture: Texture,
    pub page_count: usize,
    pub buttons: Vec<Button>,
}

pub fn render(
    view: &View,
    font_size: u32,
    background_opacity: f32,
    copy: &WristLabels,
) -> Result<Rendered, String> {
    render_with_answer_cache(
        view,
        font_size,
        background_opacity,
        copy,
        &mut AnswerCache::default(),
    )
}

pub(super) fn render_with_answer_cache(
    view: &View,
    font_size: u32,
    background_opacity: f32,
    copy: &WristLabels,
    answer_cache: &mut AnswerCache,
) -> Result<Rendered, String> {
    let alpha = (background_opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
    fill_rounded_rect(
        &mut pixels,
        SIZE,
        SIZE,
        Rect::new(32, 32, 992, 992),
        24,
        [22, 24, 28, alpha],
    );
    let mut body = TextMask::new(SIZE, SIZE, font_size.max(1) as i32)?;
    let mut labels = TextMask::new(SIZE, SIZE, 24)?;
    let vision = view.mode == ViewMode::Vision;
    let busy = view.busy();
    let error = vision && view.status == "visionError";
    let line_height = (body.line_height() * 135 + 99) / 100;
    let source_rect = if view.source_only {
        if vision {
            Rect::new(48, 164, 976, 868)
        } else {
            FULL_CARD
        }
    } else if vision {
        Rect::new(48, 164, 976, 324)
    } else {
        SOURCE_CARD
    };
    let translation_rect = if vision {
        Rect::new(48, 340, 976, 868)
    } else {
        TRANSLATION_CARD
    };
    let text_rect = |card: Rect| {
        Rect::new(
            card.left + 24,
            card.top + 60,
            card.right - 24,
            card.bottom - 24,
        )
    };
    let summary = vision && !view.source_only;
    let source_content = if summary {
        Rect::new(72, 216, 952, 308)
    } else {
        text_rect(source_rect)
    };
    let source_line_height = if summary {
        (labels.line_height() * 120 + 99) / 100
    } else {
        line_height
    };
    let source = Pages::new(
        &view.source,
        source_content.right - source_content.left,
        source_content.bottom - source_content.top,
        source_line_height,
        |text| {
            if summary {
                labels.measure_size(text, 32767, TEXT_FLAGS).0
            } else {
                body.measure_size(text, 32767, TEXT_FLAGS).0
            }
        },
    );
    let translated_content = text_rect(translation_rect);
    let markdown = vision
        && view.status == "complete"
        && !view.source_only
        && answer_cache.prepare(
            &view.translation,
            font_size,
            translated_content.right - translated_content.left,
            translated_content.bottom - translated_content.top,
        );
    let translation_text = if busy {
        copy.text(if view.status == "listening" {
            "listeningHint"
        } else {
            "requestingHint"
        })
        .to_owned()
    } else if error {
        format!("{}\n\n{}", copy.text("retryHint"), view.translation)
    } else {
        view.translation.clone()
    };
    let translation = Pages::new(
        if markdown { "" } else { &translation_text },
        translated_content.right - translated_content.left,
        translated_content.bottom - translated_content.top,
        line_height,
        |text| body.measure_size(text, 32767, TEXT_FLAGS).0,
    );
    let page_count = if busy {
        1
    } else if summary {
        if markdown {
            answer_cache.page_count().unwrap()
        } else {
            translation.lines.len()
        }
    } else {
        source.lines.len().max(if view.source_only {
            1
        } else {
            translation.lines.len()
        })
    };
    let page = view.page.min(page_count - 1);

    draw_text(
        &mut pixels,
        &mut labels,
        copy.text(&view.title),
        Rect::new(56, 56, 460, 92),
        [255; 3],
        false,
    )?;
    let has_content = view.block_count > 0 || view.mode == ViewMode::Vision;
    if has_content {
        if view.mode == ViewMode::Ocr {
            draw_text(
                &mut pixels,
                &mut labels,
                &format!(
                    "{} {} / {} · {} {} / {}",
                    copy.text("block"),
                    view.block_index + 1,
                    view.block_count,
                    copy.text("page"),
                    page + 1,
                    page_count
                ),
                Rect::new(460, 56, 968, 92),
                [200, 210, 220],
                true,
            )?;
        }
        draw_text(
            &mut pixels,
            &mut labels,
            copy.text(&view.status),
            Rect::new(56, 108, 700, 140),
            if error {
                [255, 178, 168]
            } else {
                [180, 195, 210]
            },
            false,
        )?;
    } else {
        draw_text(
            &mut pixels,
            &mut body,
            copy.text(&view.status),
            FULL_CARD,
            [210, 217, 225],
            true,
        )?;
    }
    draw_text(
        &mut pixels,
        &mut labels,
        &view.detail,
        Rect::new(708, 108, 968, 140),
        [180, 195, 210],
        true,
    )?;

    for (card, content, pages, label, color) in [
        (
            source_rect,
            source_content,
            &source,
            if view.mode == ViewMode::Vision {
                copy.text("question")
            } else {
                copy.text("source")
            },
            [210, 217, 225],
        ),
        (
            translation_rect,
            translated_content,
            &translation,
            copy.text(if error {
                "visionError"
            } else {
                &view.translation_label
            }),
            if error { [255, 195, 186] } else { [255; 3] },
        ),
    ]
    .into_iter()
    .take(if !has_content {
        0
    } else if view.source_only {
        1
    } else {
        2
    }) {
        fill_rounded_rect(
            &mut pixels,
            SIZE,
            SIZE,
            card,
            16,
            if error && card.top == translation_rect.top {
                [50, 34, 38, alpha]
            } else {
                [34, 38, 44, alpha]
            },
        );
        draw_text(
            &mut pixels,
            &mut labels,
            label,
            Rect::new(
                card.left + 24,
                card.top + 16,
                card.right - if vision { 260 } else { 120 },
                card.top + 48,
            ),
            [140, 193, 225],
            false,
        )?;
        if markdown && card.top == translation_rect.top {
            if let Err(error) = answer_cache.draw(&mut pixels, SIZE, content, page, alpha) {
                tracing::warn!(error, "VR answer Markdown draw fallback");
                answer_cache.disable();
                return render_with_answer_cache(
                    view,
                    font_size,
                    background_opacity,
                    copy,
                    answer_cache,
                );
            }
            continue;
        }
        let is_summary = summary && card.top == source_rect.top;
        let (lines, ended) = pages.at(if is_summary { 0 } else { page });
        if ended {
            draw_text(
                &mut pixels,
                &mut labels,
                copy.text("end"),
                Rect::new(
                    card.right - 100,
                    card.top + 16,
                    card.right - 24,
                    card.top + 48,
                ),
                [180, 195, 210],
                true,
            )?;
        }
        for (index, line) in lines.iter().enumerate() {
            let height = if is_summary {
                source_line_height
            } else {
                line_height
            };
            let top = content.top + index as i32 * height;
            let line = if is_summary && index + 1 == lines.len() && pages.lines.len() > 1 {
                format!("{line}…")
            } else {
                line.clone()
            };
            draw_text(
                &mut pixels,
                if is_summary { &mut labels } else { &mut body },
                &line,
                Rect::new(content.left, top, content.right, top + height),
                color,
                false,
            )?;
        }
    }

    let mut buttons = Vec::new();
    let mut controls = if vision {
        vec![
            (
                Action::PreviousPage,
                "previousPage",
                !busy && page > 0,
                Rect::new(48, 892, 224, 976),
            ),
            (
                Action::NextPage,
                "nextPage",
                !busy && page + 1 < page_count,
                Rect::new(400, 892, 576, 976),
            ),
            (
                Action::TogglePin,
                if view.pinned { "unpin" } else { "pin" },
                true,
                Rect::new(596, 892, 780, 976),
            ),
            (
                Action::Close,
                if busy { "cancel" } else { "close" },
                true,
                Rect::new(800, 892, 976, 976),
            ),
        ]
    } else {
        vec![
            (
                Action::PreviousBlock,
                "previousBlock",
                view.block_count > 1 && view.block_index > 0,
                Rect::new(48, 848, 262, 910),
            ),
            (
                Action::NextBlock,
                "nextBlock",
                view.block_count > 1 && view.block_index + 1 < view.block_count,
                Rect::new(278, 848, 492, 910),
            ),
            (
                Action::PreviousPage,
                "previousPage",
                !busy && page > 0,
                Rect::new(508, 848, 722, 910),
            ),
            (
                Action::NextPage,
                "nextPage",
                !busy && page + 1 < page_count,
                Rect::new(738, 848, 976, 910),
            ),
            (
                Action::NextLanguage,
                "language",
                !view.source_only && view.language_count > 1,
                Rect::new(48, 924, 528, 980),
            ),
            (
                Action::TogglePin,
                if view.pinned { "unpin" } else { "pin" },
                true,
                Rect::new(544, 924, 748, 980),
            ),
            (Action::Close, "close", true, Rect::new(764, 924, 976, 980)),
        ]
    };
    if vision && !busy && !view.source.is_empty() {
        controls.push((
            Action::ToggleQuestion,
            if view.source_only {
                "backToAnswer"
            } else {
                "expand"
            },
            true,
            Rect::new(732, 48, 976, 104),
        ));
    }
    for (action, label, enabled, rect) in controls {
        if !has_content && !matches!(action, Action::TogglePin | Action::Close) {
            continue;
        }
        let hovered = enabled && view.hovered == Some(action);
        let color = if hovered {
            [82, 133, 157, alpha]
        } else if action == Action::TogglePin && view.pinned {
            [61, 103, 123, alpha]
        } else if enabled {
            [49, 61, 73, alpha]
        } else {
            [30, 34, 40, alpha]
        };
        fill_rounded_rect(&mut pixels, SIZE, SIZE, rect, 10, color);
        draw_text(
            &mut pixels,
            &mut labels,
            copy.text(label),
            rect,
            if enabled { [255; 3] } else { [96, 108, 120] },
            true,
        )?;
        if enabled {
            buttons.push(Button {
                action,
                bounds: [
                    rect.left as f32 / SIZE as f32,
                    rect.top as f32 / SIZE as f32,
                    rect.right as f32 / SIZE as f32,
                    rect.bottom as f32 / SIZE as f32,
                ],
            });
        }
    }
    if has_content && vision {
        draw_text(
            &mut pixels,
            &mut labels,
            &format!("{} / {}", page + 1, page_count),
            Rect::new(236, 892, 388, 976),
            [200, 210, 220],
            true,
        )?;
    }
    Ok(Rendered {
        texture: Texture {
            pixels,
            width: SIZE,
            height: SIZE,
        },
        page_count,
        buttons,
    })
}

fn draw_text(
    pixels: &mut [u8],
    mask: &mut TextMask,
    text: &str,
    rect: Rect,
    color: [u8; 3],
    center: bool,
) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    mask.draw(
        text,
        rect,
        TEXT_FLAGS
            | if center {
                DT_CENTER | DT_END_ELLIPSIS
            } else {
                DT_END_ELLIPSIS
            },
        center,
    )?;
    mask.blend_into(pixels, SIZE, rect, 0, 0, color);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn localized_labels(resource: &str) -> WristLabels {
        let resource: serde_json::Value = serde_json::from_str(resource).unwrap();
        let mut labels = resource["translation"]["settings"]["vrOcr"]["states"]
            .as_object()
            .unwrap()
            .clone();
        labels.extend(
            resource["translation"]["wristReader"]
                .as_object()
                .unwrap()
                .clone(),
        );
        serde_json::from_value(serde_json::Value::Object(labels)).unwrap()
    }
    fn render(view: &View, font_size: u32, opacity: f32) -> Result<Rendered, String> {
        super::render(view, font_size, opacity, &WristLabels::default())
    }
    fn view() -> View {
        View {
            mode: ViewMode::Ocr,
            source: "日本語の原文 English words 中文原文\n".repeat(30),
            translation: "Translated sentence for comparison.\n".repeat(30),
            translation_label: "English".into(),
            title: "OCR".into(),
            status: "Ready".into(),
            detail: String::new(),
            block_index: 0,
            block_count: 2,
            page: 0,
            pinned: false,
            source_only: false,
            language_count: 2,
            hovered: None,
        }
    }

    #[test]
    fn only_completed_vision_answers_use_markdown() {
        for (mode, status, source_only) in [
            (ViewMode::Ocr, "complete", false),
            (ViewMode::Vision, "listening", false),
            (ViewMode::Vision, "finalizing", false),
            (ViewMode::Vision, "requesting", false),
            (ViewMode::Vision, "visionError", false),
            (ViewMode::Vision, "complete", true),
            (ViewMode::Vision, "complete", false),
        ] {
            let mut view = view();
            view.mode = mode;
            view.status = status.into();
            view.source_only = source_only;
            view.translation = "## Heading\n\n**answer**".into();
            let mut cache = AnswerCache::default();
            super::render_with_answer_cache(&view, 32, 0.9, &WristLabels::default(), &mut cache)
                .unwrap();
            assert_eq!(
                cache.page_count().is_some(),
                mode == ViewMode::Vision && status == "complete" && !source_only
            );
        }
    }

    #[test]
    fn markdown_failure_keeps_original_answer_reachable() {
        let mut view = view();
        view.mode = ViewMode::Vision;
        view.status = "complete".into();
        view.translation = format!("{}\nLAST ANSWER LINE", "original answer\n".repeat(4500));
        let original = view.translation.clone();
        let mut cache = AnswerCache::default();
        let first =
            super::render_with_answer_cache(&view, 32, 0.9, &WristLabels::default(), &mut cache)
                .unwrap();
        assert!(cache.page_count().is_none());
        let mask = TextMask::new(SIZE, SIZE, 32).unwrap();
        let pages = Pages::new(
            &original,
            880,
            444,
            (mask.line_height() * 135 + 99) / 100,
            |text| mask.measure_size(text, 32767, TEXT_FLAGS).0,
        );
        assert_eq!(first.page_count, pages.lines.len());
        assert_eq!(
            pages.lines.last().unwrap().last().unwrap(),
            "LAST ANSWER LINE"
        );
        view.page = first.page_count - 1;
        let last =
            super::render_with_answer_cache(&view, 32, 0.9, &WristLabels::default(), &mut cache)
                .unwrap();
        assert_ne!(first.texture.pixels, last.texture.pixels);
        assert!(!last
            .buttons
            .iter()
            .any(|button| button.action == Action::NextPage));
        assert_eq!(view.translation, original);
    }

    #[test]
    fn markdown_fixture_renders_all_sizes_and_exports_preview() {
        let mut view = view();
        view.mode = ViewMode::Vision;
        view.status = "complete".into();
        view.source = "What does this sign say? / 这张告示说了什么？".into();
        view.title = "visionTitle".into();
        view.translation_label = "answer".into();
        view.translation = include_str!("../../tests/fixtures/vr_vision_markdown.md").into();
        let labels = localized_labels(include_str!("../../../src/i18n/locales/zh-CN.json"));
        let directory = std::env::var("VRCS_WRIST_PREVIEW_DIR").ok();
        for font_size in [18, 32, 72] {
            let mut cache = AnswerCache::default();
            view.page = 0;
            let first =
                super::render_with_answer_cache(&view, font_size, 0.94, &labels, &mut cache)
                    .unwrap();
            assert!(
                cache.page_count().is_some(),
                "font size {font_size} fell back to plain text"
            );
            assert!(first.page_count > 1);
            let pages = first.page_count;
            for page in [0, pages / 2, pages - 1] {
                view.page = page;
                let rendered =
                    super::render_with_answer_cache(&view, font_size, 0.94, &labels, &mut cache)
                        .unwrap();
                assert_eq!(rendered.page_count, pages);
                if let Some(directory) = &directory {
                    std::fs::write(
                        std::path::Path::new(directory)
                            .join(format!("markdown-{font_size}-{page}.png")),
                        super::super::ocr_capture::encode_png(rendered.texture).unwrap(),
                    )
                    .unwrap();
                }
            }
            view.source_only = true;
            let question =
                super::render_with_answer_cache(&view, font_size, 0.94, &labels, &mut cache)
                    .unwrap();
            assert_eq!(question.page_count, 1);
            view.source_only = false;
            view.page = 0;
            view.hovered = Some(Action::Close);
            view.pinned = true;
            view.detail = "10s".into();
            let answer =
                super::render_with_answer_cache(&view, font_size, 0.94, &labels, &mut cache)
                    .unwrap();
            assert_eq!(answer.page_count, pages);
            assert!(cache.page_count().is_some());
            assert_eq!(cache.preparations, 1);
        }
    }
    #[test]
    fn vision_pages_have_no_ocr_block_or_language_controls() {
        let mut view = view();
        view.mode = ViewMode::Vision;
        view.block_count = 0;
        let rendered = render(&view, 32, 0.94).unwrap();
        assert!(rendered.page_count > 1);
        assert!(rendered
            .buttons
            .iter()
            .any(|button| button.action == Action::NextPage));
        assert!(rendered
            .buttons
            .iter()
            .any(|button| button.action == Action::TogglePin));
        assert!(rendered
            .buttons
            .iter()
            .any(|button| button.action == Action::Close));
        assert!(!rendered.buttons.iter().any(|button| matches!(
            button.action,
            Action::PreviousBlock | Action::NextBlock | Action::NextLanguage
        )));
    }

    #[test]
    fn vision_answer_uses_most_of_the_reading_area() {
        let mut view = view();
        view.mode = ViewMode::Vision;
        view.source = "What does this sign say?".into();
        view.translation = "One short answer line.\n".repeat(12);
        let rendered = render(&view, 32, 0.94).unwrap();
        assert!(
            rendered.page_count <= 2,
            "answer needs {} pages",
            rendered.page_count
        );
    }

    #[test]
    fn pending_questions_cannot_page_or_expand_and_summary_stays_on_answer_pages() {
        let mut view = view();
        view.mode = ViewMode::Vision;
        view.status = "requesting".into();
        view.source = "Long question\n".repeat(30);
        view.translation.clear();
        let pending = render(&view, 32, 1.).unwrap();
        assert_eq!(pending.page_count, 1);
        assert!(pending
            .buttons
            .iter()
            .all(|button| matches!(button.action, Action::TogglePin | Action::Close)));
        view.status = "complete".into();
        view.translation = "Long answer\n".repeat(30);
        let first = render(&view, 32, 1.).unwrap();
        view.page = 1;
        let second = render(&view, 32, 1.).unwrap();
        let summary_rows = (164 * SIZE * 4) as usize..(324 * SIZE * 4) as usize;
        assert_eq!(
            first.texture.pixels[summary_rows.clone()],
            second.texture.pixels[summary_rows]
        );
        assert!(first
            .buttons
            .iter()
            .any(|button| button.action == Action::ToggleQuestion));
    }

    #[test]
    fn localized_states_render_and_export_review_preview() {
        let mut ocr = view();
        ocr.title = "ocrTitle".into();
        ocr.status = "complete".into();
        ocr.source =
            "PHOTOGRAPHY AREA\nPlease ask before taking photos.\nKeep the walkway clear.".into();
        ocr.translation = "摄影区域\n拍照前，请先征得对方同意。\n请保持通道畅通。".into();
        ocr.translation_label = "简体中文".into();
        ocr.block_count = 3;
        let mut vision = ocr.clone();
        vision.mode = ViewMode::Vision;
        vision.title = "visionTitle".into();
        vision.translation_label = "answer".into();
        vision.source = "这张告示在提醒什么？我需要注意哪些事？".into();
        vision.translation = "这是一张摄影区域的告示。\n\n1. 拍摄前，请先征得对方同意。\n2. 不要遮挡通道或长时间占位。\n3. 不要公开他人的个人信息。\n\n可以先询问：“现在方便合影吗？”".into();
        vision.detail = "28 秒后隐藏".into();
        let mut listening = vision.clone();
        listening.status = "listening".into();
        listening.source = "我想知道这个告示在提醒什么".into();
        listening.translation.clear();
        listening.detail = "4 / 30 s".into();
        let mut requesting = listening.clone();
        requesting.status = "requesting".into();
        requesting.detail.clear();
        let mut error = vision.clone();
        error.status = "visionError".into();
        error.translation = "Request timed out".into();
        let mut expanded = vision.clone();
        expanded.source_only = true;
        expanded.source =
            "这张告示在提醒什么？\n\n请解释拍照前需要确认的事项，以及为什么要保持通道畅通。".into();
        let mut original = ocr.clone();
        original.source_only = true;
        original.status = "source_visible".into();
        let samples = [
            ocr, vision, listening, requesting, error, expanded, original,
        ];
        for resource in [
            include_str!("../../../src/i18n/locales/en-US.json"),
            include_str!("../../../src/i18n/locales/zh-CN.json"),
            include_str!("../../../src/i18n/locales/zh-Hant.json"),
            include_str!("../../../src/i18n/locales/ja-JP.json"),
        ] {
            let labels = localized_labels(resource);
            let mask = TextMask::new(SIZE, SIZE, 24).unwrap();
            for key in [
                "previousPage",
                "nextPage",
                "pin",
                "unpin",
                "close",
                "cancel",
            ] {
                assert!(
                    mask.measure_size(labels.text(key), 32767, TEXT_FLAGS).0 <= 176,
                    "button label is too wide: {}",
                    labels.text(key)
                );
            }
            for view in &samples {
                let rendered = super::render(view, 32, 0.94, &labels).unwrap();
                assert!(rendered.page_count > 0);
                for button in &rendered.buttons {
                    assert!(
                        button.bounds[0] < button.bounds[2] && button.bounds[1] < button.bounds[3]
                    );
                }
            }
        }
        let Ok(directory) = std::env::var("VRCS_WRIST_PREVIEW_DIR") else {
            return;
        };
        let labels = localized_labels(include_str!("../../../src/i18n/locales/zh-CN.json"));
        let mut sheet = Texture {
            width: SIZE * 2,
            height: SIZE + SIZE / 2,
            pixels: [16, 19, 24, 255].repeat((SIZE * SIZE * 3) as usize),
        };
        for (index, view) in samples.iter().enumerate() {
            let texture = super::render(view, 32, 0.94, &labels).unwrap().texture;
            std::fs::write(
                std::path::Path::new(&directory).join(format!("wrist-{index}.png")),
                super::super::ocr_capture::encode_png(texture.clone()).unwrap(),
            )
            .unwrap();
            if index >= 6 {
                continue;
            }
            let scale = if index < 2 { 1 } else { 2 };
            let size = SIZE / scale;
            let left = if index < 2 {
                index as u32 * SIZE
            } else {
                (index as u32 - 2) * size
            };
            let top = if index < 2 { 0 } else { SIZE };
            for y in 0..size {
                for x in 0..size {
                    let source = ((y * scale * SIZE + x * scale) * 4) as usize;
                    let target = (((top + y) * sheet.width + left + x) * 4) as usize;
                    sheet.pixels[target..target + 4]
                        .copy_from_slice(&texture.pixels[source..source + 4]);
                }
            }
        }
        std::fs::write(
            std::path::Path::new(&directory).join("wrist-review.png"),
            super::super::ocr_capture::encode_png(sheet).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn empty_results_show_status_without_result_cards_or_reading_controls() {
        let view = super::super::ocr_wrist::Reader::default().view(
            super::super::ocr_status::OcrState::NoText,
            false,
            false,
        );
        let rendered = render(&view, 32, 1.).unwrap();
        assert_eq!(rendered.buttons.len(), 2);
        assert!(rendered
            .buttons
            .iter()
            .all(|button| { matches!(button.action, Action::TogglePin | Action::Close) }));
        for (x, y) in [(60, 170), (60, 510), (750, 60), (50, 850)] {
            let offset = (y * SIZE as usize + x) * 4;
            assert_eq!(
                &rendered.texture.pixels[offset..offset + 4],
                &[22, 24, 28, 255]
            );
        }
    }

    #[test]
    fn renders_bilingual_pages_with_enabled_navigation_only() {
        let mut view = view();
        let first = render(&view, 32, 0.94).unwrap();
        assert_eq!((first.texture.width, first.texture.height), (1024, 1024));
        assert!(first.page_count > 1);
        assert!(!first
            .buttons
            .iter()
            .any(|b| b.action == Action::PreviousPage || b.action == Action::PreviousBlock));
        assert!(first.buttons.iter().any(|b| b.action == Action::NextPage));
        if let Ok(path) = std::env::var("VRCS_OCR_PREVIEW_PATH") {
            std::fs::write(
                path,
                super::super::ocr_capture::encode_png(first.texture.clone()).unwrap(),
            )
            .unwrap();
        }
        view.page = first.page_count - 1;
        let last = render(&view, 32, 0.94).unwrap();
        assert_ne!(first.texture.pixels, last.texture.pixels);
        assert!(!last.buttons.iter().any(|b| b.action == Action::NextPage));
    }
    #[test]
    fn original_only_uses_full_card_and_hover_is_visible() {
        let mut view = view();
        let bilingual = render(&view, 32, 0.94).unwrap();
        view.source_only = true;
        let original = render(&view, 32, 0.94).unwrap();
        assert!(original.page_count < bilingual.page_count);
        assert!(!original
            .buttons
            .iter()
            .any(|b| b.action == Action::NextLanguage));
        view.hovered = Some(Action::Close);
        assert_ne!(
            original.texture.pixels,
            render(&view, 32, 0.94).unwrap().texture.pixels
        );
    }
}
