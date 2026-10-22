Vendored from the crates.io `gpui-base` 0.8.0 package under Apache-2.0.
The original license is in LICENSE-APACHE. GPUI Kit remains pinned at 0.8.0.

rdiff adds `EditorState::set_external_folds(Option<Vec<FoldRange>>, cx)` and a
batched DisplayMap setter. The upstream editor exposes fold candidates but no
public way to programmatically close them. This API lets the diff surface fold
exactly matching lines while retaining the real documents, editing, and history.

While external folds are active, syntax-highlighter fold candidates are ignored.
The existing edit pipeline still shifts folds and opens those intersecting edits.
Passing `None` clears external folds and restores syntax folding.

Text decorations are composed in deterministic layer order. GPUI's upstream
`combine_highlights` iterates active styles through a hash set, which lets syntax
colors win over application overrides unpredictably. The ordered composition
keeps unchanged text neutral while retaining syntax colors inside diff blocks.
Remove these patches when equivalent upstream behavior is available.
