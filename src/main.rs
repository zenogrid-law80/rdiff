mod assets;
mod cli;
mod diff;
mod folding;
mod i18n;
mod merge;
mod merge_ui;
mod settings;
mod surface;
mod syntax;
mod ui;
mod update;
mod update_ui;

fn main() -> std::process::ExitCode {
    match cli::parse(std::env::args_os().skip(1).collect()) {
        Ok(cli::Command::Update { check }) => match update::run(check) {
            Ok(message) => {
                println!("{message}");
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("Update failed: {error}");
                std::process::ExitCode::from(2)
            }
        },
        Ok(cli::Command::Diff(paths)) => {
            ui::run(paths);
            std::process::ExitCode::SUCCESS
        }
        Ok(cli::Command::Merge(paths)) => std::process::ExitCode::from(merge_ui::run(paths)),
        Ok(cli::Command::Help) => {
            println!("{}", cli::USAGE);
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}\n\n{}", cli::USAGE);
            std::process::ExitCode::from(2)
        }
    }
}

// Exercise the exact vendored overlay implementation in the application test
// suite without requiring the vendor's optional platform test dependencies.
#[cfg(test)]
mod decoration_regression {
    use gpui_kit as gpui;
    include!("../vendor/gpui-base/src/input/base/decoration_composition.rs");
}

#[cfg(test)]
#[allow(dead_code)]
mod tab_rendering_regression {
    use gpui_kit as gpui;
    include!("../vendor/gpui-base/src/input/editor/display_map/inline_line.rs");

    #[test]
    fn expanded_tabs_preserve_utf8_highlight_ranges_and_source_text() {
        use gpui_kit::component::input::TabSize;
        let source = "\t한글\t🙂\t";
        for width in [2, 4] {
            let (display, offsets) = TabSize {
                tab_size: width,
                hard_tabs: false,
            }
            .expand_tabs(source);
            assert_eq!(display, source.replace('\t', &" ".repeat(width)));
            assert_eq!(offsets.len(), source.len() + 1);
            for (start, ch) in source.char_indices() {
                let range = offsets[start]..offsets[start + ch.len_utf8()];
                assert!(display.is_char_boundary(range.start));
                assert!(display.is_char_boundary(range.end));
                assert_eq!(
                    &display[range],
                    if ch == '\t' {
                        " ".repeat(width)
                    } else {
                        ch.to_string()
                    }
                );
            }
            assert_eq!(source, "\t한글\t🙂\t");
        }
    }

    #[test]
    fn caret_and_hit_testing_use_source_bytes_at_each_tab_width() {
        use gpui_kit::component::input::TabSize;
        use std::sync::Arc;

        let source = "a\t한\tb";
        for width in [2, 4] {
            let (display, offsets) = TabSize {
                tab_size: width,
                hard_tabs: false,
            }
            .expand_tabs(source);
            // Deterministic glyph advances exercise the production coordinate map.
            let mut shaped = ShapedLine::default();
            shaped.text = display.clone().into();
            *shaped = Arc::new(gpui::LineLayout {
                width: px(display.chars().count() as f32 * 10.),
                len: display.len(),
                runs: vec![gpui::ShapedRun {
                    font_id: gpui::FontId(0),
                    glyphs: display
                        .char_indices()
                        .enumerate()
                        .map(|(column, (index, _))| gpui::ShapedGlyph {
                            id: gpui::GlyphId(0),
                            position: point(px(column as f32 * 10.), px(0.)),
                            index,
                            is_emoji: false,
                        })
                        .collect(),
                }],
                ..Default::default()
            });
            let line = InputLine::expanded_tabs(source.into(), shaped, offsets);
            assert_eq!(line.text.as_ref(), source);
            assert_eq!(line.len, source.len());
            for index in source
                .char_indices()
                .map(|(index, _)| index)
                .chain([source.len()])
            {
                assert_eq!(line.closest_index_for_x(line.x_for_index(index)), index);
            }
            let start = line.x_for_index(1);
            let end = line.x_for_index(2);
            assert_eq!(end - start, px(width as f32 * 10.));
            assert_eq!(line.index_for_x(start + px(1.)), Some(1));
            assert_eq!(line.index_for_x(end - px(1.)), Some(2));
            assert_eq!(line.index_for_x(px(-1.)), None);
            assert_eq!(line.index_for_x(line.width + px(1.)), None);
        }
    }
}
