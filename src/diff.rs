use similar::{ChangeTag, DiffTag, TextDiff};
use std::{
    fs::File,
    io::Read,
    ops::Range,
    path::Path,
    time::{Duration, Instant},
};

pub const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub number: usize,
    pub text: String,
    pub terminated: bool,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub left: Option<Line>,
    pub right: Option<Line>,
    pub changed: bool,
    pub left_spans: Vec<Range<usize>>,
    pub right_spans: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub left: Range<usize>,
    pub right: Range<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Comparison {
    pub rows: Vec<Row>,
    pub hunks: Vec<usize>,
    pub added: usize,
    pub removed: usize,
    pub edits: Vec<Edit>,
    left_text: String,
    right_text: String,
}

impl Comparison {
    pub fn inline_ranges(&self, mode: HighlightMode) -> [Vec<Range<usize>>; 2] {
        let mut ranges = [Vec::new(), Vec::new()];
        if matches!(mode, HighlightMode::Lines | HighlightMode::None) {
            return ranges;
        }
        let deadline = Instant::now() + Duration::from_millis(50);
        for edit in &self.edits {
            if edit.left.is_empty() || edit.right.is_empty() {
                continue;
            }
            let left = &self.left_text[edit.left.clone()];
            let right = &self.right_text[edit.right.clone()];
            if left.len() + right.len() > 32_768 || Instant::now() > deadline {
                continue;
            }
            for (pane, spans) in inline_changes(left, right, mode).into_iter().enumerate() {
                let offset = if pane == 0 {
                    edit.left.start
                } else {
                    edit.right.start
                };
                ranges[pane].extend(spans.into_iter().map(|r| r.start + offset..r.end + offset));
            }
        }
        ranges
    }

    /// Apply exactly one displayed change. Refuse stale comparisons rather than
    /// replacing a range in a buffer that has since been edited.
    pub fn apply_right(&self, index: usize, left: &str, right: &str) -> Result<String, String> {
        self.apply(index, left, right, false)
    }

    pub fn apply_left(&self, index: usize, left: &str, right: &str) -> Result<String, String> {
        self.apply(index, left, right, true)
    }

    fn apply(
        &self,
        index: usize,
        left: &str,
        right: &str,
        to_left: bool,
    ) -> Result<String, String> {
        if left != self.left_text || right != self.right_text {
            return Err(
                "The text changed after comparison. Compare again before applying a change.".into(),
            );
        }
        let edit = self
            .edits
            .get(index)
            .ok_or("This change is no longer available.")?;
        let mut result = if to_left { left } else { right }.to_owned();
        if to_left {
            result.replace_range(edit.left.clone(), &right[edit.right.clone()]);
        } else {
            result.replace_range(edit.right.clone(), &left[edit.left.clone()]);
        }
        if result.len() > MAX_BYTES {
            return Err("Applying this change would exceed the 2 MiB text limit.".into());
        }
        Ok(result)
    }
}

/// UTF-8 byte ranges relative to each block, suitable for editor decorations.
pub fn inline_changes(left: &str, right: &str, mode: HighlightMode) -> [Vec<Range<usize>>; 2] {
    let mut ranges = [Vec::<Range<usize>>::new(), Vec::<Range<usize>>::new()];
    if matches!(mode, HighlightMode::Lines | HighlightMode::None)
        || left.is_empty()
        || right.is_empty()
        || left.len() + right.len() > 32_768
    {
        return ranges;
    }
    let mut config = TextDiff::configure();
    config.timeout(Duration::from_millis(3));
    let diff = if mode == HighlightMode::Words {
        config.diff_unicode_words(left, right)
    } else {
        config.diff_chars(left, right)
    };
    let mut offsets = [0, 0];
    for change in diff.iter_all_changes() {
        let len = change.value().len();
        let pane = match change.tag() {
            ChangeTag::Delete => 0,
            ChangeTag::Insert => 1,
            ChangeTag::Equal => {
                offsets[0] += len;
                offsets[1] += len;
                continue;
            }
        };
        let start = offsets[pane];
        if let Some(last) = ranges[pane].last_mut().filter(|r| r.end == start) {
            last.end += len;
        } else {
            ranges[pane].push(start..start + len);
        }
        offsets[pane] += len;
    }
    ranges
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum HighlightMode {
    #[default]
    Words,
    Characters,
    Lines,
    None,
}
impl HighlightMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Words => "Words",
            Self::Characters => "Characters",
            Self::Lines => "Lines",
            Self::None => "No highlighting",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Words => Self::Characters,
            Self::Characters => Self::Lines,
            Self::Lines => Self::None,
            Self::None => Self::Words,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Whitespace {
    #[default]
    Exact,
    Trim,
    Ignore,
}
impl Whitespace {
    pub fn label(self) -> &'static str {
        match self {
            Self::Exact => "Do not ignore",
            Self::Trim => "Trim whitespace",
            Self::Ignore => "Ignore whitespace",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Exact => Self::Trim,
            Self::Trim => Self::Ignore,
            Self::Ignore => Self::Exact,
        }
    }
}

fn byte_offsets(text: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for line in text.split_inclusive('\n') {
        offsets.push(offsets.last().unwrap() + line.len());
    }
    offsets
}

fn inline_spans(left: &str, right: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(2))
        .diff_chars(left, right);
    let mut spans = (Vec::<Range<usize>>::new(), Vec::<Range<usize>>::new());
    let (mut old, mut new) = (0, 0);
    for change in diff.iter_all_changes() {
        let size = change.value().len();
        let (ranges, start) = match change.tag() {
            ChangeTag::Delete => (&mut spans.0, old),
            ChangeTag::Insert => (&mut spans.1, new),
            ChangeTag::Equal => {
                old += size;
                new += size;
                continue;
            }
        };
        if let Some(last) = ranges.last_mut().filter(|last| last.end == start) {
            last.end += size;
        } else {
            ranges.push(start..start + size);
        }
        match change.tag() {
            ChangeTag::Delete => old += size,
            ChangeTag::Insert => new += size,
            ChangeTag::Equal => unreachable!(),
        }
    }
    spans
}

fn lines(text: &str) -> Vec<Line> {
    text.split_inclusive('\n')
        .enumerate()
        .map(|(index, raw)| Line {
            number: index + 1,
            text: raw
                .strip_suffix("\r\n")
                .or_else(|| raw.strip_suffix('\n'))
                .unwrap_or(raw)
                .to_owned(),
            terminated: raw.ends_with('\n'),
        })
        .collect()
}

fn key(line: &Line, whitespace: Whitespace) -> String {
    let mut text = match whitespace {
        Whitespace::Ignore => line.text.chars().filter(|c| !c.is_whitespace()).collect(),
        Whitespace::Trim => line.text.trim().to_owned(),
        Whitespace::Exact => line.text.clone(),
    };
    // Preserve the final-newline distinction even in whitespace-ignore mode.
    if line.terminated {
        text.push('\n');
    }
    text
}

pub fn compare(left: &str, right: &str, ignore_whitespace: bool) -> Comparison {
    compare_with_whitespace(
        left,
        right,
        if ignore_whitespace {
            Whitespace::Ignore
        } else {
            Whitespace::Exact
        },
    )
}

pub fn compare_with_whitespace(left: &str, right: &str, whitespace: Whitespace) -> Comparison {
    let left_offsets = byte_offsets(left);
    let right_offsets = byte_offsets(right);
    let mut result = Comparison {
        left_text: left.into(),
        right_text: right.into(),
        ..Default::default()
    };
    let left = lines(left);
    let right = lines(right);
    let old: Vec<_> = left.iter().map(|line| key(line, whitespace)).collect();
    let new: Vec<_> = right.iter().map(|line| key(line, whitespace)).collect();
    let old_refs: Vec<_> = old.iter().map(String::as_str).collect();
    let new_refs: Vec<_> = new.iter().map(String::as_str).collect();
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(300))
        .diff_slices(&old_refs, &new_refs);
    let inline_deadline = Instant::now() + Duration::from_millis(50);
    for op in diff.ops() {
        let changed = op.tag() != DiffTag::Equal;
        if changed {
            if result.rows.last().is_none_or(|row| !row.changed) {
                result.hunks.push(result.rows.len());
                result.edits.push(Edit {
                    left: left_offsets[op.old_range().start]..left_offsets[op.old_range().end],
                    right: right_offsets[op.new_range().start]..right_offsets[op.new_range().end],
                });
            } else if let Some(edit) = result.edits.last_mut() {
                edit.left.end = left_offsets[op.old_range().end];
                edit.right.end = right_offsets[op.new_range().end];
            }
            result.removed += op.old_range().len();
            result.added += op.new_range().len();
        }
        let old = op.old_range();
        let new = op.new_range();
        for offset in 0..old.len().max(new.len()) {
            let left = (offset < old.len()).then(|| left[old.start + offset].clone());
            let right = (offset < new.len()).then(|| right[new.start + offset].clone());
            let (left_spans, right_spans) = match (&left, &right) {
                (Some(left), Some(right)) if changed && left.text != right.text => {
                    if left.text.len() + right.text.len() <= 16_384
                        && Instant::now() < inline_deadline
                    {
                        inline_spans(&left.text, &right.text)
                    } else {
                        (
                            std::iter::once(0..left.text.len()).collect(),
                            std::iter::once(0..right.text.len()).collect(),
                        )
                    }
                }
                _ => (Vec::new(), Vec::new()),
            };
            result.rows.push(Row {
                left,
                right,
                left_spans,
                right_spans,
                changed,
            });
        }
    }
    result
}

pub fn unified_patch(left: &str, right: &str, old_name: &str, new_name: &str) -> String {
    TextDiff::configure()
        .timeout(Duration::from_millis(300))
        .diff_lines(left, right)
        .unified_diff()
        .context_radius(3)
        .header(
            &old_name.replace(['\r', '\n'], " "),
            &new_name.replace(['\r', '\n'], " "),
        )
        .to_string()
}

pub fn read_text(path: &Path) -> Result<(String, bool), String> {
    let text = read_text_exact(path)?;
    let bom = text.starts_with('\u{feff}');
    Ok((
        text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned(),
        bom,
    ))
}

pub fn save_file(path: &Path, expected: &str, text: &str, bom: bool) -> Result<(), String> {
    if text.len() + if bom { 3 } else { 0 } > MAX_BYTES {
        return Err("The file would exceed the 2 MiB limit. Save was stopped.".into());
    }
    let original = read_text_exact(path)?;
    let expected = if bom {
        format!("\u{feff}{expected}")
    } else {
        expected.into()
    };
    if original != expected {
        return Err(
            "The file changed on disk. Save was stopped; reopen the file before applying changes."
                .into(),
        );
    }
    crate::merge::save_result(path, text, bom)
}

pub fn read_text_exact(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > MAX_BYTES as u64 {
        return Err("File is too large. The limit is 2 MiB per file.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("File is too large. The limit is 2 MiB per file.".into());
    }
    if bytes.contains(&0) {
        return Err("Binary files and UTF-16 are not supported. Choose UTF-8 text.".into());
    }
    String::from_utf8(bytes).map_err(|_| "This file is not valid UTF-8 text.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_only_selected_change_and_preserves_other_right_content() {
        let left = "a\nleft\nkeep\nlast\n";
        let right = "a\nright\nkeep\nLAST\n";
        let comparison = compare(left, right, false);
        assert_eq!(
            comparison.apply_right(0, left, right).unwrap(),
            "a\nleft\nkeep\nLAST\n"
        );
        assert_eq!(
            comparison.apply_right(1, left, right).unwrap(),
            "a\nright\nkeep\nlast\n"
        );
        assert!(comparison.apply_right(2, left, right).is_err());
        assert!(
            comparison
                .apply_right(0, left, "edited since comparing")
                .is_err()
        );
    }

    #[test]
    fn applies_insert_delete_empty_and_eof_changes() {
        for (left, right) in [
            ("a\nb\nc\n", "a\nc\n"),
            ("a\nc\n", "a\nb\nc\n"),
            ("", "all removed"),
            ("all added", ""),
            ("a\n", "a"),
            ("a", "a\n"),
        ] {
            let comparison = compare(left, right, false);
            assert_eq!(comparison.hunks.len(), 1);
            assert_eq!(comparison.apply_right(0, left, right).unwrap(), left);
            assert_eq!(comparison.apply_left(0, left, right).unwrap(), right);
        }
    }

    #[test]
    fn left_merge_preserves_other_blocks_and_rejects_stale_text() {
        let left = "머리\r\n원본\r\nkeep\r\nleft\r\n끝";
        let right = "머리\n수정\nkeep\nright\n끝";
        let comparison = compare(left, right, false);
        assert_eq!(
            comparison.apply_left(0, left, right).unwrap(),
            "머리\r\n수정\nkeep\r\nleft\r\n끝"
        );
        assert_eq!(
            comparison.apply_left(1, left, right).unwrap(),
            "머리\r\n원본\r\nkeep\r\nright\n끝"
        );
        assert!(comparison.apply_left(0, "changed", right).is_err());
        assert!(comparison.apply_left(0, left, "changed").is_err());
        assert!(comparison.apply_left(2, left, right).is_err());
    }

    #[test]
    fn hunk_copy_preserves_exact_unicode_and_line_endings() {
        let left = "머리\r\n원본\r\n끝";
        let right = "머리\n수정\n끝";
        assert_eq!(
            compare(left, right, false)
                .apply_right(0, left, right)
                .unwrap(),
            "머리\n원본\r\n끝"
        );
        let left = " a\nleft\nz\n";
        let right = "a\nright\nz\n";
        assert_eq!(
            compare(left, right, true)
                .apply_right(0, left, right)
                .unwrap(),
            "a\nleft\nz\n"
        );
    }

    #[test]
    fn inline_spans_use_utf8_boundaries_and_leave_equal_text_clear() {
        let row = compare("한글 old 끝\n", "한글 new 끝\n", false)
            .rows
            .remove(0);
        let old = &row.left.unwrap().text;
        let new = &row.right.unwrap().text;
        assert_eq!(
            row.left_spans
                .iter()
                .map(|r| &old[r.clone()])
                .collect::<String>(),
            "old"
        );
        assert_eq!(
            row.right_spans
                .iter()
                .map(|r| &new[r.clone()])
                .collect::<String>(),
            "new"
        );
        assert_eq!(inline_spans("same", "same"), (vec![], vec![]));
        let (old, new) = inline_spans("안녕", "안녕하세요");
        assert!(old.is_empty());
        assert_eq!(&"안녕하세요"[new[0].clone()], "하세요");
    }

    #[test]
    fn right_save_preserves_bom_and_refuses_external_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("right.txt");
        std::fs::write(&path, "\u{feff}old\r\n").unwrap();
        save_file(&path, "old\r\n", "new\r\n", true).unwrap();
        assert_eq!(read_text_exact(&path).unwrap(), "\u{feff}new\r\n");
        assert!(save_file(&path, "new\r\n", &"x".repeat(MAX_BYTES), true).is_err());
        assert_eq!(read_text_exact(&path).unwrap(), "\u{feff}new\r\n");
        std::fs::write(&path, "external edit").unwrap();
        assert!(save_file(&path, "new\r\n", "overwrite", true).is_err());
        assert_eq!(read_text_exact(&path).unwrap(), "external edit");
    }

    #[test]
    fn aligns_replacements_and_insertions() {
        let diff = compare("one\ntwo\nthree\n", "one\nTWO\nextra\nthree\n", false);
        assert_eq!((diff.added, diff.removed), (2, 1));
        assert_eq!(diff.hunks, [1]);
        assert_eq!(diff.rows[1].left.as_ref().unwrap().text, "two");
        assert_eq!(diff.rows[1].right.as_ref().unwrap().text, "TWO");
        assert!(diff.rows[2].left.is_none());
        assert_eq!(diff.rows[3].right.as_ref().unwrap().number, 4);
    }

    #[test]
    fn empty_and_identical_inputs() {
        assert!(compare("", "", false).rows.is_empty());
        assert!(compare("same\n", "same\n", false).hunks.is_empty());
        assert_eq!(compare("", "\n", false).added, 1);
        assert_eq!(compare("gone", "", false).removed, 1);
    }

    #[test]
    fn whitespace_mode_keeps_original_display_text() {
        let diff = compare("  hello   world\n", "hello world\n", true);
        assert!(diff.hunks.is_empty());
        assert_eq!(diff.rows[0].left.as_ref().unwrap().text, "  hello   world");
        assert!(!compare(" a\n", "a\n", false).hunks.is_empty());
    }

    #[test]
    fn trimming_ignores_only_line_edges() {
        assert!(
            compare_with_whitespace("  a b \n", "a b\n", Whitespace::Trim)
                .edits
                .is_empty()
        );
        assert_eq!(
            compare_with_whitespace("a  b\n", "a b\n", Whitespace::Trim)
                .edits
                .len(),
            1
        );
        assert!(
            compare_with_whitespace("a\u{2003}b\n", "ab\n", Whitespace::Ignore)
                .edits
                .is_empty()
        );
        assert_eq!(
            compare_with_whitespace("a\n", "a", Whitespace::Trim)
                .edits
                .len(),
            1
        );
    }

    #[test]
    fn inline_highlights_keep_token_and_unicode_boundaries() {
        let left = "// 공통\ncall(old_name);\n";
        let right = "// 공통\ncall(new_name);\n";
        let compared = compare(left, right, false);
        let spans = compared.inline_ranges(HighlightMode::Words);
        assert_eq!(
            spans[0]
                .iter()
                .map(|r| &left[r.clone()])
                .collect::<String>(),
            "old_name"
        );
        assert_eq!(
            spans[1]
                .iter()
                .map(|r| &right[r.clone()])
                .collect::<String>(),
            "new_name"
        );
        let left = "안녕🙂 세계";
        let right = "안녕🙃 세상";
        for mode in [HighlightMode::Words, HighlightMode::Characters] {
            for (text, spans) in [left, right]
                .into_iter()
                .zip(inline_changes(left, right, mode))
            {
                assert!(!spans.is_empty());
                for range in spans {
                    assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
                }
            }
        }
        assert!(
            compared
                .inline_ranges(HighlightMode::None)
                .iter()
                .all(Vec::is_empty)
        );
    }

    #[test]
    fn normalizes_crlf_but_detects_missing_final_newline() {
        assert!(compare("hello\r\n", "hello\n", false).hunks.is_empty());
        assert_eq!(compare("hello\n", "hello", false).hunks, [0]);
        assert_eq!(compare("hello\n", "hello", true).hunks, [0]);
        assert!(unified_patch("a\n", "a", "old", "new").contains("No newline at end of file"));
    }

    #[test]
    fn unicode_and_separate_change_groups() {
        let diff = compare("한글\nkeep\n끝\n", "한국어\nkeep\n마지막\n", false);
        assert_eq!(diff.hunks, [0, 2]);
        assert_eq!(diff.rows[2].right.as_ref().unwrap().text, "마지막");
    }

    #[test]
    fn patch_preserves_exact_text_and_sanitizes_headers() {
        let patch = unified_patch(" a\n", "a\n", "old\nfile", "new\rfile");
        assert!(patch.starts_with("--- old file\n+++ new file\n"));
        assert!(patch.contains("- a\n+a\n"));
    }
}
