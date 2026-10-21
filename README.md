# rdiff

A native text comparison and three-way merge tool built with Rust, [GPUI Kit 0.8.0](https://gpui-kit.com/docs/getting-started/), and similar 3.2.0.

## Running

Building requires Rust stable and the Windows MSVC C++ build tools.

```powershell
cargo run --bin rdiff
cargo run --bin rdiff -- diff examples/before.rs examples/after.rs
cargo run --bin rdiff -- merge examples/merge-source.txt examples/merge-target.txt examples/merge-base.txt target/merge-result.txt
```

Without arguments, rdiff opens a sample comparison. The legacy `rdiff <file1> <file2>` form is also supported. After building, you can run `target/debug/rdiff.exe` directly. Use `rdiff --help` for command usage.

## GitHub self-update

Click **Check updates** at the bottom of either the Diff or three-way merge window to check the latest stable GitHub Release of `zenogrid-law80/rdiff`. If an update is available, click **Install v…**. The button tooltip shows the result or error details. Updating does not restart the application: unsaved edits remain open, and the new version takes effect on the next launch.

```powershell
rdiff self-update --check
rdiff self-update
```

Self-update supports Windows x64 and requires write access to the installation directory. It downloads `rdiff-windows-x86_64.exe` and verifies its size and GitHub API SHA-256 digest before replacing the executable. If replacement fails, it attempts to restore the previous executable and reports any recovery failure. The backup is kept at `.rdiff-update-*/previous.exe` in the installation directory. You can delete that backup directory after closing all windows running the previous version. Network errors, API rate limits, and missing public releases are reported as errors. Downgrades and prereleases are not installed. Authentication for private repositories is not currently supported.

To publish a release, update the version in `Cargo.toml` and push a matching `vX.Y.Z` tag. `.github/workflows/release.yml` tests and builds the application, then publishes the executable with the required asset name to GitHub Releases.

## P4V external tool configuration

Set the executable to the absolute path of `rdiff.exe` and use the following arguments. Keep the quotes to support paths containing spaces.

```text
Diff:   diff "%1" "%2"
Merge:  merge "%1" "%2" "%b" "%r"
```

| Merge argument | Meaning |
| --- | --- |
| `%1` | Source / Theirs: incoming changes |
| `%2` | Target / Yours: your changes |
| `%b` | Base: common ancestor |
| `%r` | Result: output file path |

This order follows [P4V's external merge tool argument conventions](https://www.perforce.com/manuals/v22.2/p4v/Content/P4V/configuring.preferences.merge.html). rdiff stays running until the merge window closes and reports the outcome through its exit code.

## RustRover

1. Open this repository with **File → Open**.
2. Wait for Cargo synchronization, then create a Cargo run configuration for `run --bin rdiff`.
3. Click **Run**. Use `test --bin rdiff` to run the tests.

For a comparison, add `-- diff examples/before.rs examples/after.rs`. For the sample merge with two conflicts, use the merge command shown above; its output path is `target/merge-result.txt`.

## Comparing and editing

- **Open file** opens the original and modified files independently.
- **Side by side** shows a left pane that accepts block merges but not direct typing, and a directly editable right pane. Typing and pasting trigger comparison while preserving the cursor. `Ctrl+E` focuses the right editor; `Ctrl+Enter` compares again.
- Both line-number columns appear in the center as **left number | right number**. Each follows its document's scrolling and folding state, with change connectors between them.
- The central double-chevron button applies its change block from left to right. Left-only text is inserted, right-only text is removed, and replacements use the left text. Other blocks remain unchanged, and comparison refreshes immediately. **Apply >>** applies the selected block.
- **Alt+Down / Alt+Up** selects the next / previous change. **Alt+Right** applies left to right; **Alt+Left** applies right to left. **<< Apply / Apply >>** performs the same operations. Both directions share undo history with direct edits.
- **↶ / ↷**, `Ctrl+Z`, and `Ctrl+Shift+Z` or `Ctrl+Y` undo and redo edits and block merges. History retains up to 128 entries and 32 MiB. A new edit after undo clears redo history.
- **Save left / Save right** saves only that file. **Ctrl+S** saves both modified files. Applying a block changes memory only. Saving preserves each file's original BOM and refuses to overwrite a file changed externally. Each file is saved atomically; a failed save leaves that side marked as modified. Open a file before saving demo edits.
- **Revert right** restores the last loaded or saved right-hand text and can itself be undone. Opening a modified side and swapping files are disabled while changes are unsaved. Closing checks both sides for unsaved changes.
- **Side by side / Unified** switches between the editable split view and a read-only single-column view.
- **Words / Characters / Lines / No highlighting** cycles through difference highlighting modes. Rust, TOML, JSON, and Markdown syntax highlighting is supported.
- **Do not ignore / Trim whitespace / Ignore whitespace** compares all whitespace, ignores whitespace at line edges, or ignores all Unicode whitespace. Source text is preserved.
- The **Sync scroll** icon links panes using actual line correspondences. Turn it off to navigate independently. Shared context remains aligned even when document lengths differ.
- The **Collapse unchanged** icon folds long matching sections in split and three-way views, leaving three context lines around changes. Click a thin wavy divider to expand the corresponding section in all panes. Turn the button off to show all text, or on to collapse again. Three-way folding hides only sections identical in all three panes. Line numbers, saving, and merging use the original documents.
- File headers show line endings as **LF**, **CRLF**, **CR**, **Mixed**, or **No EOL**.
- **Curved / Straight** changes connectors between blocks of different heights. Modifications are blue, insertions green, and deletions gray.
- The outer scroll tracks show the locations and extents of changes. Click a marker to navigate to it; a bright outline marks the selected change. Empty sides of insertions and deletions still have a small marker. Three-way merge provides a track for each pane.
- **↑ / ↓**, `Shift+F7` / `F7` cycles through changes. You can also select a change in the sidebar.
- **Swap** exchanges the original and modified files.
- **Copy patch** copies a unified patch with three context lines. It uses exact source text, including whitespace and line-ending changes, regardless of display comparison settings.
- **Changes** toggles the change list. It is hidden by default to leave more room for editing.
- Use the mouse wheel to scroll vertically. Horizontal wheel or trackpad gestures, or `Shift+wheel`, scroll the pane under the pointer horizontally. Line numbers and central merge buttons stay fixed.

## Comparison behavior and limits

- UTF-8 text files up to 2 MiB per file are supported. A UTF-8 BOM is hidden in the editor and preserved when saving.
- Binary files, UTF-16, and invalid UTF-8 produce an error message.
- Display comparison treats CRLF and LF as equivalent but distinguishes a missing final newline.
- Line comparison runs in the background with a 300 ms computation limit. Complex inputs may produce larger change groups instead of a minimal edit sequence.
- Word or character differences receive stronger highlighting inside changed blocks. UTF-8 boundaries are preserved. Large blocks or exhausted computation budgets fall back to whole-line highlighting.
- Only visible lines are rendered. Folder comparison and Git repository browsing are outside the current scope.

## Three-way merge

- Nonoverlapping changes and identical changes on both sides are merged automatically against the common ancestor. Conflict computation runs in the background.
- Documents appear as **Source | Result | Target**. Source and Target are read-only. The central Result remains directly editable even with unresolved conflicts. Conflicting sections initially contain Base text alongside the automatically merged content.
- Double-chevron buttons beside each conflict apply the corresponding Source or Target block to Result. Connectors follow the current Result positions after editing. Synchronized scrolling, curved or straight connectors, and word or character highlighting are available.
- **Alt+Down / Alt+Up** selects the next / previous conflict. **Alt+Right** applies Source → Result; **Alt+Left** applies Target → Result. Source and Target files are never modified. `F7` / `Shift+F7` also navigates conflicts.
- Selecting a conflict in the **Conflicts** list or with navigation keys scrolls all three editors to it. Amber means unresolved, green means resolved, and a border marks the selection. **Base** shows the selected conflict's common ancestor below the editors.
- **Source >> / << Target / Use both / Use base** replaces only the selected conflict's current Result range. Edits elsewhere remain intact, and these controls continue to work after direct editing. **Use both** places Source before Target. **Reset conflict** restores Base text and marks the conflict unresolved.
- After editing a conflict manually, use **Mark resolved** to accept the current text. Editing inside a resolved conflict reopens it. Edits outside conflict ranges do not change resolution status.
- **Undo / Redo** covers direct edits, conflict applications, manual resolution, and resets in one history. It restores text, conflict positions, and resolution states together. Shortcuts are `Ctrl+Z`, `Ctrl+Shift+Z`, and `Ctrl+Y`. History is limited to 128 entries and 32 MiB; a new edit after undo clears redo history.
- Insertions at conflict boundaries are preserved as surrounding text. A replacement spanning several conflicts stays outside those conflict ranges; fully removed conflicts are tracked as insertion points. Use **Mark resolved** to accept the manual result or undo to restore the original ranges.
- **Save** / `Ctrl+S` writes to `%r`. **Save & Close** / `Ctrl+Enter` saves and exits. Even a successful automatic merge writes nothing until explicitly saved.
- **Close** or the window close button exits. Unsaved changes prompt for save, discard, or cancel; saving is unavailable while conflicts remain unresolved. Exit codes: `0` = all conflicts resolved and current result saved; `1` = canceled or unsaved; `2` = argument, input, or save error. Editing after saving returns the outcome to `1`; undoing to the saved text and resolution state restores `0`.
- Merging distinguishes whitespace, CRLF/LF, and missing final newlines exactly. Automatic merging and source selections preserve original line endings. Result BOM handling follows Target. Unreadable inputs display an error and disable saving.
- Saving completes a temporary file in the result directory before replacement. A failed save preserves the existing result. Only `%r` is written; inputs are snapshots read when the window opens.

## Development and validation

```powershell
cargo fmt --all --check
cargo check --locked --bin rdiff
cargo test --locked --bin rdiff
cargo clippy --locked --bin rdiff -- -D warnings
```

`src/diff.rs` handles line comparison and patch generation; `src/merge.rs` handles three-way merging and result saving; `src/cli.rs` parses command arguments. `src/surface.rs` provides shared editors, change regions, connectors, and synchronized scrolling. `src/ui.rs` and `src/merge_ui.rs` connect these features to their respective screens. Files under `examples/` are comparison and merge inputs, not Cargo executable examples.

`src/folding.rs` computes exact matching sections and folding context. The repository vendors `gpui-base` 0.8.0 to expose an external folding API that preserves source text. Changes are documented in `vendor/gpui-base/RDIFF-PATCH.md`. `src/update.rs` implements release checking, verification, and executable replacement; `src/update_ui.rs` provides the shared update control.

The interface and core comparison and merge behavior are inspired by RustRover diff. This is not a complete replica of JetBrains products. IDE VCS integration, folder and binary comparison, language-aware semantic analysis, and formatting-aware comparison are not supported.

MIT License.
