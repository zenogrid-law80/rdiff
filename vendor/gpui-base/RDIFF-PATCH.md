Vendored from the crates.io `gpui-base` 0.8.0 package under Apache-2.0.
The original license is in LICENSE-APACHE. GPUI Kit remains pinned at 0.8.0.

rdiff adds `EditorState::set_external_folds(Option<Vec<FoldRange>>, cx)` and a
batched DisplayMap setter. The upstream editor exposes fold candidates but no
public way to programmatically close them. This API lets the diff surface fold
exactly matching lines while retaining the real documents, editing, and history.

While external folds are active, syntax-highlighter fold candidates are ignored.
The existing edit pipeline still shifts folds and opens those intersecting edits.
Passing `None` clears external folds and restores syntax folding. All other source
is the unmodified 0.8.0 package. Remove this patch when an equivalent upstream API
is available.
