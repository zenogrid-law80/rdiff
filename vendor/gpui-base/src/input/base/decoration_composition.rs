use gpui::HighlightStyle;
#[cfg(test)]
use gpui::{FontStyle, FontWeight, blue, red};
use std::ops::Range;

pub(super) fn compose_decorations(
    mut styles: Vec<(Range<usize>, HighlightStyle)>,
    decorations: impl IntoIterator<Item = (Range<usize>, HighlightStyle)>,
    visible_byte_range: Range<usize>,
) -> Option<Vec<(Range<usize>, HighlightStyle)>> {
    let mut visible_decorations = decorations
        .into_iter()
        .filter_map(|(range, style)| {
            let range =
                range.start.max(visible_byte_range.start)..range.end.min(visible_byte_range.end);
            (!range.is_empty()).then_some((range, style))
        })
        .peekable();

    if visible_decorations.peek().is_none() {
        return (!styles.is_empty()).then_some(styles);
    }
    if styles.is_empty() {
        styles.push((visible_byte_range.clone(), HighlightStyle::default()));
    }

    // Decorations are application-authored overrides and must win over syntax
    // and semantic highlighting when both set the same style property.
    // Preserve layer order; GPUI's active-style HashSet has no precedence.
    let layers: Vec<_> = styles.into_iter().chain(visible_decorations).collect();
    let mut endpoints = Vec::with_capacity(layers.len() * 2);
    for (id, (range, _)) in layers.iter().enumerate() {
        if !range.is_empty() {
            endpoints.push((range.start, id, true));
            endpoints.push((range.end, id, false));
        }
    }
    endpoints.sort_unstable();
    let mut active = std::collections::BTreeSet::new();
    let mut previous = 0;
    let mut result = Vec::new();
    for (offset, id, start) in endpoints {
        if previous < offset && !active.is_empty() {
            let style = active
                .iter()
                .fold(HighlightStyle::default(), |style, &id: &usize| {
                    style.highlight(layers[id].1)
                });
            result.push((previous..offset, style));
        }
        if start {
            active.insert(id);
        } else {
            active.remove(&id);
        }
        previous = offset;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::{FontStyle, FontWeight, HighlightStyle, blue, compose_decorations, red};

    #[test]
    fn neutral_decoration_wins_over_every_syntax_run() {
        let syntax = (0..64)
            .map(|i| {
                (
                    i * 2..i * 2 + 2,
                    HighlightStyle {
                        color: Some(blue()),
                        font_style: Some(FontStyle::Italic),
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let neutral = HighlightStyle {
            color: Some(red()),
            font_style: Some(FontStyle::Normal),
            font_weight: Some(FontWeight::NORMAL),
            ..Default::default()
        };
        let styles = compose_decorations(syntax, [(2..126, neutral)], 0..128).unwrap();
        for (range, style) in styles {
            if range.start >= 2 && range.end <= 126 {
                assert_eq!(style.color, neutral.color);
                assert_eq!(style.font_style, neutral.font_style);
                assert_eq!(style.font_weight, neutral.font_weight);
            } else {
                assert_eq!(style.color, Some(blue()));
            }
        }
    }

    #[test]
    fn clips_overlays_and_keeps_syntax_gaps() {
        let syntax = vec![(
            0..12,
            HighlightStyle {
                color: Some(blue()),
                ..Default::default()
            },
        )];
        let fill = HighlightStyle {
            background_color: Some(red()),
            ..Default::default()
        };
        let styles = compose_decorations(syntax, [(0..3, fill), (8..16, fill)], 2..10).unwrap();
        assert_eq!(
            styles.iter().map(|(r, _)| r.clone()).collect::<Vec<_>>(),
            vec![0..2, 2..3, 3..8, 8..10, 10..12]
        );
        assert!(styles.iter().all(|(_, s)| s.color == Some(blue())));
        assert_eq!(styles[2].1.background_color, None);
        assert_eq!(styles[3].1.background_color, Some(red()));
    }
}
