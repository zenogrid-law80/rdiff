//! Matching line spans shared by all panes. These are display ranges only;
//! no placeholder text is ever inserted into a document.
use similar::{DiffTag, TextDiff};
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    ops::Range,
    time::Duration,
};

const CONTEXT: usize = 3;

#[derive(Clone, Debug)]
pub struct EqualFold {
    pub key: u64,
    /// Hidden lines, excluding the three context lines on either side. The
    /// first line is retained by the editor as a clickable placeholder row.
    pub ranges: Vec<Range<usize>>,
}

fn equal_runs(left: &str, right: &str) -> Vec<[Range<usize>; 2]> {
    TextDiff::configure()
        .timeout(Duration::from_millis(100))
        .diff_lines(left, right)
        .ops()
        .iter()
        .filter(|op| op.tag() == DiffTag::Equal)
        .map(|op| [op.old_range(), op.new_range()])
        .collect()
}

pub fn equal_folds(texts: &[&str]) -> Vec<EqualFold> {
    assert!(matches!(texts.len(), 2 | 3));
    let left = equal_runs(texts[0], texts[1]);
    let common: Vec<Vec<Range<usize>>> = if texts.len() == 2 {
        left.into_iter().map(Vec::from).collect()
    } else {
        // Intersect in Result coordinates. Automatically merged changes that
        // differ from either outer pane must stay visible as well as conflicts.
        let right = equal_runs(texts[1], texts[2]);
        let (mut a, mut b) = (0, 0);
        let mut common = Vec::new();
        while a < left.len() && b < right.len() {
            let start = left[a][1].start.max(right[b][0].start);
            let end = left[a][1].end.min(right[b][0].end);
            if start < end {
                let source = left[a][0].start + start - left[a][1].start;
                let target = right[b][1].start + start - right[b][0].start;
                common.push(vec![
                    source..source + end - start,
                    start..end,
                    target..target + end - start,
                ]);
            }
            if left[a][1].end <= right[b][0].end {
                a += 1;
            } else {
                b += 1;
            }
        }
        common
    };
    // split_inclusive preserves newline differences, including CRLF and EOF.
    let lines: Vec<_> = texts[1].split_inclusive('\n').collect();
    let mut occurrences = HashMap::<u64, usize>::new();
    common
        .into_iter()
        .filter_map(|ranges| {
            if ranges[1].len() < CONTEXT * 2 + 3 {
                return None;
            }
            let ranges: Vec<_> = ranges
                .into_iter()
                .map(|r| r.start + CONTEXT..r.end - CONTEXT)
                .collect();
            let mut hash = DefaultHasher::new();
            lines[ranges[1].clone()].hash(&mut hash);
            // Repeated identical blocks need distinct controls/expanded state.
            let occurrence = occurrences.entry(hash.finish()).or_default();
            occurrence.hash(&mut hash);
            *occurrence += 1;
            Some(EqualFold {
                key: hash.finish(),
                ranges,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::equal_folds;

    fn lines(count: usize) -> String {
        (0..count).map(|n| format!("한글 {n}\n")).collect()
    }

    #[test]
    fn retains_context_at_edges_and_both_sides_of_changes() {
        let left = lines(40);
        let right = left.replace("한글 20\n", "modified\n");
        let folds = equal_folds(&[&left, &right]);
        assert_eq!(folds.len(), 2);
        assert_eq!(folds[0].ranges, vec![3..17, 3..17]);
        assert_eq!(folds[1].ranges, vec![24..37, 24..37]);
    }

    #[test]
    fn insertions_keep_corresponding_original_line_numbers() {
        let left = lines(40);
        let right = left.replace("한글 20\n", "added\n한글 20\n");
        let folds = equal_folds(&[&left, &right]);
        assert_eq!(folds[1].ranges, vec![23..37, 24..38]);
        let reverse = equal_folds(&[&right, &left]);
        assert_eq!(reverse[1].ranges, vec![24..38, 23..37]);
    }

    #[test]
    fn three_way_only_folds_lines_equal_in_every_document() {
        let base = lines(60);
        let source = base.replace("한글 20\n", "source\n");
        let target = base.replace("한글 40\n", "target\n");
        let result = source.replace("한글 40\n", "target\n");
        let folds = equal_folds(&[&source, &result, &target]);
        let expected = [3..17, 24..37, 44..57];
        assert_eq!(folds.len(), expected.len());
        for (fold, range) in folds.iter().zip(expected) {
            assert_eq!(fold.ranges, vec![range; 3]);
        }
    }

    #[test]
    fn short_empty_newline_and_whitespace_differences_remain_visible() {
        assert!(equal_folds(&["", ""]).is_empty());
        let short = lines(8);
        assert!(equal_folds(&[&short, &short]).is_empty());
        let left = lines(20);
        let crlf = left.replace('\n', "\r\n");
        assert!(equal_folds(&[&left, &crlf]).is_empty());
        let spaced = left.replace("한글", " 한글");
        assert!(equal_folds(&[&left, &spaced]).is_empty());
        let no_newline = left.trim_end_matches('\n');
        assert_eq!(
            equal_folds(&[&left, no_newline])[0].ranges,
            vec![3..16, 3..16]
        );
    }

    #[test]
    fn expanded_block_identity_survives_changes_before_it() {
        let original = lines(40);
        let changed = original.replace("한글 20\n", "changed\n");
        let before = equal_folds(&[&original, &changed]);
        let shifted = format!("inserted\n{changed}");
        let after = equal_folds(&[&original, &shifted]);
        assert_eq!(before[1].key, after[1].key);
    }

    #[test]
    fn repeated_equal_blocks_have_independent_expansion_keys() {
        let repeated = "same\n".repeat(20);
        let left = format!("{repeated}left\n{repeated}");
        let right = format!("{repeated}right\n{repeated}");
        let folds = equal_folds(&[&left, &right]);
        assert_eq!(folds.len(), 2);
        assert_ne!(folds[0].key, folds[1].key);
    }
}
