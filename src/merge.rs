use crate::diff;
use similar::{Algorithm, MergeResolution, TextMerge};
use std::{
    collections::VecDeque,
    fs,
    io::Write,
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePaths {
    pub source: PathBuf,
    pub target: PathBuf,
    pub base: PathBuf,
    pub result: PathBuf,
}

pub struct MergeInput {
    pub source: String,
    pub target: String,
    pub base: String,
    pub bom: bool,
}

impl MergeInput {
    pub fn load(paths: &MergePaths) -> Result<Self, String> {
        let read = |path: &Path, role: &str| {
            diff::read_text_exact(path)
                .map_err(|error| format!("{role} ({}): {error}", path.display()))
        };
        let source = read(&paths.source, "Source %1")?;
        let target = read(&paths.target, "Target %2")?;
        let base = read(&paths.base, "Base %b")?;
        let bom = target.starts_with('\u{feff}');
        let without_bom = |text: String| text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned();
        Ok(Self {
            source: without_bom(source),
            target: without_bom(target),
            base: without_bom(base),
            bom,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Source,
    Target,
    Both,
    Base,
    Manual(String),
}

impl Resolution {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Target => "Target",
            Self::Both => "Both",
            Self::Base => "Base",
            Self::Manual(_) => "Edited",
        }
    }
}

pub struct Conflict {
    pub source: String,
    pub target: String,
    pub base: String,
    pub base_line: usize,
    pub source_range: Range<usize>,
    pub target_range: Range<usize>,
    pub result_range: Range<usize>,
    pub resolution: Option<Resolution>,
}

impl Conflict {
    fn choice_text(&self, choice: &Resolution) -> String {
        match choice {
            Resolution::Source => self.source.clone(),
            Resolution::Target => self.target.clone(),
            Resolution::Base => self.base.clone(),
            Resolution::Manual(text) => text.clone(),
            Resolution::Both => {
                let mut text = self.source.clone();
                if !text.is_empty() && !self.target.is_empty() && !text.ends_with('\n') {
                    text.push_str(
                        if self.source.contains("\r\n") || self.target.contains("\r\n") {
                            "\r\n"
                        } else {
                            "\n"
                        },
                    );
                }
                text.push_str(&self.target);
                text
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    text: String,
    conflicts: Vec<(Range<usize>, Option<Resolution>)>,
}

impl Snapshot {
    fn bytes(&self) -> usize {
        self.text.len()
            + self.conflicts.len() * std::mem::size_of::<(Range<usize>, Option<Resolution>)>()
            + self
                .conflicts
                .iter()
                .map(|(_, choice)| match choice {
                    Some(Resolution::Manual(text)) => text.len(),
                    _ => 0,
                })
                .sum::<usize>()
    }
}

pub struct MergeDocument {
    text: String,
    pub conflicts: Vec<Conflict>,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
}

impl MergeDocument {
    pub fn new(base: &str, source: &str, target: &str) -> Self {
        // Keep exact line endings and EOF, and never ignore whitespace in a merge.
        // Patience anchors reduce work on long files; the UI runs this off-thread.
        let merge = TextMerge::configure()
            .algorithm(Algorithm::Patience)
            .merge_lines(base, source, target);
        let mut text = String::new();
        let mut conflicts = Vec::new();
        let mut source_offset = 0;
        let mut target_offset = 0;
        for region in merge.regions() {
            let source: String = region
                .ours_range()
                .filter_map(|i| merge.ours_line(i))
                .collect();
            let target: String = region
                .theirs_range()
                .filter_map(|i| merge.theirs_line(i))
                .collect();
            let base: String = region
                .base_range()
                .filter_map(|i| merge.base_line(i))
                .collect();
            let automatic = match region.resolution() {
                MergeResolution::Unchanged | MergeResolution::Ours | MergeResolution::Both => {
                    Some(source.clone())
                }
                MergeResolution::Theirs => Some(target.clone()),
                _ => None,
            };
            let source_range = source_offset..source_offset + source.len();
            let target_range = target_offset..target_offset + target.len();
            source_offset = source_range.end;
            target_offset = target_range.end;
            if let Some(value) = automatic {
                text.push_str(&value);
            } else {
                // Unresolved ranges start with the ancestor, without markers.
                // Their resolution state is separate from the editable buffer.
                let start = text.len();
                text.push_str(&base);
                conflicts.push(Conflict {
                    source,
                    target,
                    base,
                    base_line: region.base_range().start + 1,
                    source_range,
                    target_range,
                    result_range: start..text.len(),
                    resolution: None,
                });
            }
        }
        Self {
            text,
            conflicts,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        }
    }

    pub fn remaining(&self) -> usize {
        self.conflicts
            .iter()
            .filter(|conflict| conflict.resolution.is_none())
            .count()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            conflicts: self
                .conflicts
                .iter()
                .map(|c| (c.result_range.clone(), c.resolution.clone()))
                .collect(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        for (conflict, (range, resolution)) in self.conflicts.iter_mut().zip(snapshot.conflicts) {
            conflict.result_range = range;
            conflict.resolution = resolution;
        }
    }

    fn trim_history(&mut self) {
        // Shared budget for typing, choices and both history directions.
        let mut bytes = self
            .undo
            .iter()
            .chain(&self.redo)
            .map(Snapshot::bytes)
            .sum::<usize>();
        while self.undo.len() + self.redo.len() > 128 || bytes > 32 * 1024 * 1024 {
            let oldest = if !self.undo.is_empty() {
                self.undo.pop_front()
            } else {
                self.redo.pop_front()
            };
            let Some(oldest) = oldest else { break };
            bytes = bytes.saturating_sub(oldest.bytes());
        }
    }

    fn checkpoint(&mut self) {
        self.undo.push_back(self.snapshot());
        self.redo.clear();
        self.trim_history();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push_back(self.snapshot());
        self.restore(previous);
        self.trim_history();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop_back() else {
            return false;
        };
        self.undo.push_back(self.snapshot());
        self.restore(next);
        self.trim_history();
        true
    }

    pub fn resolve(&mut self, index: usize, choice: Option<Resolution>) -> bool {
        let Some(conflict) = self.conflicts.get(index) else {
            return false;
        };
        let range = conflict.result_range.clone();
        let choice_text = choice
            .as_ref()
            .map(|choice| conflict.choice_text(choice))
            .unwrap_or_else(|| conflict.base.clone());
        if choice == conflict.resolution && self.text[range.clone()] == choice_text {
            return false;
        }
        self.checkpoint();
        let new_end = range.start + choice_text.len();
        let delta = choice_text.len() as isize - range.len() as isize;
        self.text.replace_range(range.clone(), &choice_text);
        // Index order gives collapsed adjacent ranges a deterministic affinity.
        for conflict in &mut self.conflicts[index + 1..] {
            conflict.result_range = conflict.result_range.start.saturating_add_signed(delta)
                ..conflict.result_range.end.saturating_add_signed(delta);
        }
        self.conflicts[index].result_range = range.start..new_end;
        self.conflicts[index].resolution = choice;
        true
    }

    pub fn mark_resolved(&mut self, index: usize) -> bool {
        let Some(conflict) = self.conflicts.get(index) else {
            return false;
        };
        self.resolve(
            index,
            Some(Resolution::Manual(
                self.text[conflict.result_range.clone()].into(),
            )),
        )
    }

    /// Rebase conflict bookmarks after a user edit. Insertions at a nonempty
    /// range's boundaries belong to surrounding text, so applying that conflict
    /// cannot erase a newly added prefix/suffix. Empty ranges own inserted text.
    pub fn edit(&mut self, text: String) -> bool {
        if text == self.text {
            return false;
        }
        let start = self
            .text
            .chars()
            .zip(text.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        let suffix = self.text[start..]
            .chars()
            .rev()
            .zip(text[start..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        let end = self.text.len() - suffix;
        let inserted = text.len() - suffix - start;
        let delta = inserted as isize - (end - start) as isize;
        let owner = self.conflicts.iter().position(|c| {
            let r = &c.result_range;
            if start == end {
                r.start < start && start < r.end || r.is_empty() && r.start == start
            } else {
                r.start <= start && end <= r.end && !r.is_empty()
            }
        });
        self.checkpoint();
        for (index, conflict) in self.conflicts.iter_mut().enumerate() {
            let r = conflict.result_range.clone();
            if owner == Some(index) {
                conflict.result_range.end = r.end.saturating_add_signed(delta);
                conflict.resolution = None;
            } else if end <= r.start {
                conflict.result_range =
                    r.start.saturating_add_signed(delta)..r.end.saturating_add_signed(delta);
            } else if start < r.end {
                // A replacement crossing a boundary keeps newly typed text
                // outside that conflict. Only surviving fragments are owned;
                // a completely deleted conflict becomes an insertion bookmark.
                let new_start = if r.start < start {
                    r.start
                } else {
                    start + inserted
                };
                let new_end = if r.end > end {
                    r.end.saturating_add_signed(delta)
                } else {
                    start
                };
                conflict.result_range = new_start..new_end.max(new_start);
                conflict.resolution = None;
            }
        }
        self.text = text;
        true
    }
}

/// Write a complete UTF-8 result beside the destination, then atomically replace
/// it. A failed write/rename leaves the existing result intact. No input is written.
pub fn save_result(path: &Path, text: &str, bom: bool) -> Result<(), String> {
    fn write(path: &Path, text: &str, bom: bool) -> std::io::Result<()> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.permissions().readonly() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "The result file is read-only.",
                ));
            }
            temp.as_file().set_permissions(metadata.permissions())?;
        }
        if bom {
            temp.write_all(b"\xef\xbb\xbf")?;
        }
        temp.write_all(text.as_bytes())?;
        temp.flush()?;
        temp.as_file().sync_all()?;
        temp.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
    write(path, text, bom).map_err(|error| format!("Could not save {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_independent_edits_and_identical_changes() {
        let doc = MergeDocument::new(
            "one\ntwo\nthree\n",
            "ONE\ntwo\nthree\n",
            "one\ntwo\nTHREE\n",
        );
        assert_eq!(doc.remaining(), 0);
        assert_eq!(doc.text(), "ONE\ntwo\nTHREE\n");
        let doc = MergeDocument::new("old\n", "same\n", "same\n");
        assert_eq!(doc.remaining(), 0);
        assert_eq!(doc.text(), "same\n");
    }

    #[test]
    fn conflict_resolutions_preserve_surrounding_auto_merge() {
        let mut doc = MergeDocument::new("a\nb\nc\n", "a\nsource\nc\n", "a\ntarget\nc\n");
        assert_eq!(doc.remaining(), 1);
        assert_eq!(doc.text(), "a\nb\nc\n");
        assert_eq!(doc.conflicts[0].base_line, 2);
        for (choice, expected) in [
            (Resolution::Source, "source\n"),
            (Resolution::Target, "target\n"),
            (Resolution::Both, "source\ntarget\n"),
            (Resolution::Base, "b\n"),
            (Resolution::Manual("수정\n".into()), "수정\n"),
            (Resolution::Manual(String::new()), ""),
        ] {
            doc.resolve(0, Some(choice));
            assert_eq!(doc.remaining(), 0);
            assert_eq!(doc.text(), format!("a\n{expected}c\n"));
        }
        doc.resolve(0, None);
        assert_eq!(doc.remaining(), 1);
    }

    #[test]
    fn deletion_vs_edit_and_eof_conflicts_are_explicit() {
        let mut doc = MergeDocument::new("keep\nold\n", "keep\n", "keep\nnew\n");
        assert_eq!(doc.remaining(), 1);
        doc.resolve(0, Some(Resolution::Source));
        assert_eq!(doc.text(), "keep\n");
        let mut doc = MergeDocument::new("old", "소스", "대상");
        assert_eq!(doc.remaining(), 1);
        doc.resolve(0, Some(Resolution::Both));
        assert_eq!(doc.text(), "소스\n대상");
    }

    #[test]
    fn exact_crlf_and_missing_final_newline_survive() {
        let doc = MergeDocument::new("a\r\nb\r\n끝", "A\r\nb\r\n끝", "a\r\nb\r\n마지막");
        assert_eq!(doc.remaining(), 0);
        assert_eq!(doc.text(), "A\r\nb\r\n마지막");
        assert_eq!(MergeDocument::new("", "", "").text(), "");
    }

    #[test]
    fn loading_and_saving_preserve_target_bom_without_touching_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let paths = MergePaths {
            source: dir.path().join("소스 파일.txt"),
            target: dir.path().join("target.txt"),
            base: dir.path().join("base.txt"),
            result: dir.path().join("result file.txt"),
        };
        fs::write(&paths.source, "소스\n").unwrap();
        fs::write(&paths.target, "\u{feff}base\n").unwrap();
        fs::write(&paths.base, "base\n").unwrap();
        fs::write(&paths.result, "previous output").unwrap();
        let input = MergeInput::load(&paths).unwrap();
        assert!(input.bom);
        let doc = MergeDocument::new(&input.base, &input.source, &input.target);
        // Merely loading/resolving never writes the result.
        assert_eq!(
            fs::read_to_string(&paths.result).unwrap(),
            "previous output"
        );
        save_result(&paths.result, doc.text(), input.bom).unwrap();
        assert_eq!(fs::read_to_string(&paths.result).unwrap(), "\u{feff}소스\n");
        assert_eq!(fs::read_to_string(&paths.source).unwrap(), "소스\n");
        assert_eq!(fs::read_to_string(&paths.target).unwrap(), "\u{feff}base\n");
        assert_eq!(fs::read_to_string(&paths.base).unwrap(), "base\n");
    }

    #[test]
    fn live_edits_and_choices_share_undo_redo_and_rebase_later_conflicts() {
        let mut doc = MergeDocument::new(
            "head\na\ncontext\nb\ntail\n",
            "head\nSOURCE A\ncontext\nSOURCE B\ntail\n",
            "head\nTARGET A\ncontext\nTARGET B\ntail\n",
        );
        assert_eq!(doc.conflicts.len(), 2);
        let initial = doc.snapshot();
        doc.edit(format!("설명🙂\r\n{}", doc.text()));
        let edited = doc.snapshot();
        doc.resolve(0, Some(Resolution::Source));
        let applied = doc.snapshot();
        doc.edit(doc.text().replace("tail", "직접 편집한 꼬리"));
        let edited_again = doc.snapshot();
        doc.resolve(1, Some(Resolution::Target));
        let final_state = doc.snapshot();
        assert_eq!(
            doc.text(),
            "설명🙂\r\nhead\nSOURCE A\ncontext\nTARGET B\n직접 편집한 꼬리\n"
        );
        assert_eq!(doc.remaining(), 0);
        for expected in [edited_again, applied, edited, initial] {
            assert!(doc.undo());
            assert!(doc.snapshot() == expected);
        }
        assert!(!doc.undo());
        for _ in 0..4 {
            assert!(doc.redo());
        }
        assert!(doc.snapshot() == final_state);
        doc.undo();
        doc.mark_resolved(1);
        assert!(!doc.can_redo());
    }

    #[test]
    fn editing_resolved_conflict_reopens_it_and_manual_confirmation_is_undoable() {
        let mut doc = MergeDocument::new(
            "keep\r\nold\r\n끝",
            "keep\r\n소스\r\n끝",
            "keep\r\n대상\r\n끝",
        );
        doc.resolve(0, Some(Resolution::Source));
        doc.edit(doc.text().replace("소스", "직접🙂"));
        assert_eq!(doc.remaining(), 1);
        let range = doc.conflicts[0].result_range.clone();
        assert_eq!(&doc.text()[range], "직접🙂\r\n");
        doc.mark_resolved(0);
        assert_eq!(doc.remaining(), 0);
        doc.undo();
        assert_eq!(doc.remaining(), 1);
        doc.undo();
        assert_eq!(doc.conflicts[0].resolution, Some(Resolution::Source));
        assert_eq!(doc.text(), "keep\r\n소스\r\n끝");
        doc.redo();
        doc.resolve(0, Some(Resolution::Target));
        assert_eq!(doc.text(), "keep\r\n대상\r\n끝");
    }

    #[test]
    fn boundary_insertions_survive_choices_and_empty_conflicts_remain_addressable() {
        let mut doc = MergeDocument::new("a\nold\nz\n", "a\nleft\nz\n", "a\nright\nz\n");
        doc.edit("a\nprefix\nold\nz\n".into());
        doc.edit("a\nprefix\nold\nsuffix\nz\n".into());
        doc.resolve(0, Some(Resolution::Source));
        assert_eq!(doc.text(), "a\nprefix\nleft\nsuffix\nz\n");
        doc.resolve(0, None);
        assert_eq!(doc.text(), "a\nprefix\nold\nsuffix\nz\n");
        let mut doc = MergeDocument::new("", "소스", "대상");
        assert_eq!(doc.conflicts.len(), 1);
        doc.edit("수동".into());
        assert_eq!(doc.conflicts[0].result_range, 0.."수동".len());
        doc.mark_resolved(0);
        doc.resolve(0, Some(Resolution::Target));
        assert_eq!(doc.text(), "대상");
    }

    #[test]
    fn edit_across_conflicts_keeps_replacement_outside_collapsed_bookmarks() {
        let mut doc = MergeDocument::new("a\ncontext\nb\n", "A\ncontext\nB\n", "X\ncontext\nY\n");
        assert_eq!(doc.conflicts.len(), 2);
        doc.edit("직접 전체 교체".into());
        for conflict in &doc.conflicts {
            assert!(conflict.result_range.is_empty());
        }
        doc.resolve(1, Some(Resolution::Target));
        doc.resolve(0, Some(Resolution::Source));
        assert_eq!(doc.text(), "직접 전체 교체A\nY\n");
        doc.undo();
        doc.undo();
        doc.undo();
        assert_eq!(doc.text(), "a\ncontext\nb\n");
    }

    #[test]
    fn history_is_bounded_and_noop_edits_do_not_consume_it() {
        let mut doc = MergeDocument::new("", "", "");
        for i in 0..200 {
            doc.edit(format!("{i}"));
        }
        assert_eq!(doc.undo.len(), 128);
        assert!(!doc.edit("199".into()));
        assert_eq!(doc.undo.len(), 128);
    }

    #[test]
    fn failed_save_leaves_existing_result_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("readonly.txt");
        fs::write(&path, "original").unwrap();
        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        assert!(save_result(&path, "replacement", false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        fs::set_permissions(&path, original_permissions).unwrap();
        assert!(save_result(&dir.path().join("missing/result"), "new", false).is_err());
    }
}
