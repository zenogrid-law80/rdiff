use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

const MAX_ENTRIES: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Link,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyDirection {
    LeftToRight,
    RightToLeft,
}

impl CopyDirection {
    pub fn source(self) -> usize {
        match self {
            Self::LeftToRight => 0,
            Self::RightToLeft => 1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LeftToRight => "Copy left → right",
            Self::RightToLeft => "Copy right → left",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

impl FileInfo {
    fn new(metadata: &Metadata) -> Self {
        let kind = if metadata.file_type().is_symlink() {
            EntryKind::Link
        } else if metadata.is_dir() {
            EntryKind::Directory
        } else if metadata.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        };
        Self {
            kind,
            size: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Different,
    LeftOnly,
    RightOnly,
    TypeMismatch,
    Error,
    NotCompared,
    Identical,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Different => "Different",
            Self::LeftOnly => "Left only",
            Self::RightOnly => "Right only",
            Self::TypeMismatch => "File/folder type differs",
            Self::Error => "Could not compare",
            Self::NotCompared => "Subfolder not compared",
            Self::Identical => "Identical",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortColumn {
    #[default]
    Name,
    Folder,
    Status,
    LeftModified,
    RightModified,
    Extension,
}

#[derive(Clone, Debug)]
pub struct FolderEntry {
    pub relative: PathBuf,
    pub left: Option<FileInfo>,
    pub right: Option<FileInfo>,
    pub status: Status,
    pub error: Option<String>,
}

impl FolderEntry {
    pub fn info(&self, side: usize) -> Option<&FileInfo> {
        if side == 0 {
            self.left.as_ref()
        } else {
            self.right.as_ref()
        }
    }

    pub fn can_copy(&self, direction: CopyDirection) -> bool {
        self.status != Status::Error
            && self
                .info(direction.source())
                .is_some_and(|info| info.kind == EntryKind::File)
            && self
                .info(1 - direction.source())
                .is_none_or(|info| info.kind == EntryKind::File)
    }

    pub fn can_open(&self) -> bool {
        self.status != Status::Error
            && self
                .left
                .iter()
                .chain(self.right.iter())
                .all(|info| info.kind == EntryKind::File)
            && (self.left.is_some() || self.right.is_some())
    }

    pub fn name(&self) -> String {
        self.relative
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    pub fn folder(&self) -> String {
        self.relative
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into())
    }

    pub fn extension(&self) -> String {
        if self
            .left
            .iter()
            .chain(self.right.iter())
            .any(|info| info.kind == EntryKind::Directory)
        {
            return String::new();
        }
        self.relative
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
}

pub struct FolderComparison {
    pub roots: [PathBuf; 2],
    pub entries: Vec<FolderEntry>,
}

impl FolderComparison {
    pub fn copy_file(
        &self,
        index: usize,
        direction: CopyDirection,
        cancelled: &AtomicBool,
    ) -> Result<u64, String> {
        check_copy_cancel(cancelled)?;
        let entry = self
            .entries
            .get(index)
            .filter(|entry| entry.can_copy(direction))
            .ok_or("Select a regular file that exists on the source side")?;
        if entry.relative.as_os_str().is_empty()
            || entry
                .relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err("The file path must stay inside the selected folders".into());
        }
        let source_side = direction.source();
        let target_side = 1 - source_side;
        let source_info = entry.info(source_side).unwrap();
        let target_info = entry.info(target_side);
        let source_root =
            fs::canonicalize(&self.roots[source_side]).map_err(|error| error.to_string())?;
        let target_root =
            fs::canonicalize(&self.roots[target_side]).map_err(|error| error.to_string())?;
        let source = copy_path(&source_root, &entry.relative, false)?;
        let target = copy_path(&target_root, &entry.relative, false)?;
        let source_metadata = check_copy_snapshot(&source, Some(source_info))?.unwrap();
        let target_metadata = check_copy_snapshot(&target, target_info)?;
        let canonical_source = fs::canonicalize(&source).map_err(|error| error.to_string())?;
        if fs::canonicalize(&target).ok().as_ref() == Some(&canonical_source) {
            return Err("Source and destination are the same file".into());
        }
        check_writable(target_metadata.as_ref())?;
        let mut input = File::open(&source).map_err(|error| error.to_string())?;
        unchanged(&input, source_info)?;
        // Create only missing ordinary directories. Never follow a directory
        // symlink introduced since scanning to write outside the chosen root.
        let target = copy_path(&target_root, &entry.relative, true)?;
        let parent = target.parent().ok_or("Destination folder is unavailable")?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        let temp_permissions = temp
            .as_file()
            .metadata()
            .map_err(|error| error.to_string())?
            .permissions();
        let mut remaining = source_info.size;
        let mut buffer = [0u8; 64 * 1024];
        while remaining > 0 {
            check_copy_cancel(cancelled)?;
            let count = remaining.min(buffer.len() as u64) as usize;
            input
                .read_exact(&mut buffer[..count])
                .map_err(|error| error.to_string())?;
            temp.write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
            remaining -= count as u64;
        }
        temp.flush().map_err(|error| error.to_string())?;
        temp.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        unchanged(&input, source_info)?;
        check_copy_snapshot(&source, Some(source_info))?;
        if fs::canonicalize(&self.roots[source_side]).ok().as_ref() != Some(&source_root)
            || fs::canonicalize(&self.roots[target_side]).ok().as_ref() != Some(&target_root)
        {
            return Err("A selected folder changed on disk. Compare again before copying.".into());
        }
        copy_path(&source_root, &entry.relative, false)?;
        copy_path(&target_root, &entry.relative, false)?;
        let latest_target = check_copy_snapshot(&target, target_info)?;
        check_writable(latest_target.as_ref())?;
        check_copy_cancel(cancelled)?;
        // Preserve an existing destination's permissions; new files inherit
        // source permissions, including executable bits on Unix.
        let permissions = target_metadata
            .as_ref()
            .unwrap_or(&source_metadata)
            .permissions();
        temp.as_file()
            .set_permissions(permissions)
            .map_err(|error| error.to_string())?;
        let published = if target_info.is_some() {
            temp.persist(&target)
        } else {
            temp.persist_noclobber(&target)
        };
        match published {
            Ok(_) => Ok(source_info.size),
            Err(error) => {
                // A read-only source must not leave an undeletable temporary
                // file behind if Windows refuses the final rename.
                let _ = error.file.as_file().set_permissions(temp_permissions);
                Err(error.error.to_string())
            }
        }
    }

    pub fn visible(&self, identical: bool, column: SortColumn, descending: bool) -> Vec<usize> {
        let mut indices: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| identical || entry.status != Status::Identical)
            .map(|(index, _)| index)
            .collect();
        indices.sort_by(|&a, &b| {
            let (a, b) = (&self.entries[a], &self.entries[b]);
            let order = match column {
                SortColumn::Name => a.relative.file_name().cmp(&b.relative.file_name()),
                SortColumn::Folder => a.relative.parent().cmp(&b.relative.parent()),
                SortColumn::Status => a.status.cmp(&b.status),
                SortColumn::LeftModified => a
                    .left
                    .as_ref()
                    .and_then(|info| info.modified)
                    .cmp(&b.left.as_ref().and_then(|info| info.modified)),
                SortColumn::RightModified => a
                    .right
                    .as_ref()
                    .and_then(|info| info.modified)
                    .cmp(&b.right.as_ref().and_then(|info| info.modified)),
                SortColumn::Extension => a.relative.extension().cmp(&b.relative.extension()),
            }
            .then_with(|| a.relative.cmp(&b.relative));
            if descending { order.reverse() } else { order }
        });
        indices
    }
}

fn check_copy_cancel(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err("File copy canceled".into())
    } else {
        Ok(())
    }
}

fn check_writable(metadata: Option<&Metadata>) -> Result<(), String> {
    if metadata.is_some_and(|metadata| metadata.permissions().readonly()) {
        Err("The destination file is read-only".into())
    } else {
        Ok(())
    }
}

fn check_copy_snapshot(
    path: &Path,
    expected: Option<&FileInfo>,
) -> Result<Option<Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if expected.is_none_or(|info| {
                info.kind != EntryKind::File
                    || !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.len() != info.size
                    || metadata.modified().ok() != info.modified
            }) {
                return Err(format!(
                    "File changed on disk. Compare again before copying: {}",
                    path.display()
                ));
            }
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && expected.is_none() => {
            Ok(None)
        }
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn copy_path(root: &Path, relative: &Path, create: bool) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    if !fs::metadata(root)
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err("A selected root is no longer a folder".into());
    }
    for component in relative.parent().unwrap_or(Path::new("")).components() {
        path.push(component.as_os_str());
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if create {
                    match fs::create_dir(&path) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                            let metadata =
                                fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
                            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                                return Err("Destination parent is not an ordinary folder".into());
                            }
                        }
                        Err(error) => return Err(error.to_string()),
                    }
                }
            }
            _ => {
                return Err(format!(
                    "Parent folder changed or is a symbolic link: {}",
                    path.display()
                ));
            }
        }
    }
    Ok(root.join(relative))
}

#[derive(Clone)]
pub struct CompareOptions {
    pub recursive: bool,
    pub filter: String,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            filter: "*.*".into(),
        }
    }
}

#[derive(Default)]
struct Snapshot {
    entries: BTreeMap<PathBuf, Result<FileInfo, String>>,
    blocked: BTreeMap<PathBuf, String>,
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err("Folder comparison canceled".into())
    } else {
        Ok(())
    }
}

fn scan(root: &Path, recursive: bool, cancelled: &AtomicBool) -> Result<Snapshot, String> {
    if !fs::metadata(root)
        .map_err(|error| format!("{}: {error}", root.display()))?
        .is_dir()
    {
        return Err(format!("{}: not a folder", root.display()));
    }
    let mut snapshot = Snapshot::default();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        check_cancel(cancelled)?;
        let entries = match fs::read_dir(root.join(&relative)) {
            Ok(entries) => entries,
            Err(error) if relative.as_os_str().is_empty() => {
                return Err(format!("{}: {error}", root.display()));
            }
            Err(error) => {
                snapshot.blocked.insert(relative, error.to_string());
                continue;
            }
        };
        for entry in entries {
            check_cancel(cancelled)?;
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) if relative.as_os_str().is_empty() => {
                    return Err(format!("{}: {error}", root.display()));
                }
                Err(error) => {
                    snapshot.blocked.insert(relative.clone(), error.to_string());
                    continue;
                }
            };
            let path = relative.join(entry.file_name());
            let info = fs::symlink_metadata(entry.path())
                .map(|metadata| FileInfo::new(&metadata))
                .map_err(|error| error.to_string());
            if recursive
                && info
                    .as_ref()
                    .is_ok_and(|info| info.kind == EntryKind::Directory)
            {
                pending.push(path.clone());
            }
            snapshot.entries.insert(path, info);
            if snapshot.entries.len() > MAX_ENTRIES {
                return Err(format!(
                    "Folder comparison exceeds the {MAX_ENTRIES} item limit"
                ));
            }
        }
    }
    Ok(snapshot)
}

fn unchanged(file: &File, info: &FileInfo) -> Result<(), String> {
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file()
        || metadata.len() != info.size
        || metadata.modified().ok() != info.modified
    {
        return Err("File changed during comparison. Compare again.".into());
    }
    Ok(())
}

fn equal_files(
    left: &Path,
    right: &Path,
    a: &FileInfo,
    b: &FileInfo,
    cancelled: &AtomicBool,
) -> Result<bool, String> {
    let mut left = File::open(left).map_err(|error| error.to_string())?;
    let mut right = File::open(right).map_err(|error| error.to_string())?;
    unchanged(&left, a)?;
    unchanged(&right, b)?;
    let mut equal = a.size == b.size;
    if equal {
        let (mut left_bytes, mut right_bytes) = ([0u8; 64 * 1024], [0u8; 64 * 1024]);
        let mut remaining = a.size;
        while remaining > 0 {
            check_cancel(cancelled)?;
            let count = remaining.min(left_bytes.len() as u64) as usize;
            left.read_exact(&mut left_bytes[..count])
                .map_err(|error| error.to_string())?;
            right
                .read_exact(&mut right_bytes[..count])
                .map_err(|error| error.to_string())?;
            if left_bytes[..count] != right_bytes[..count] {
                equal = false;
                break;
            }
            remaining -= count as u64;
        }
    }
    unchanged(&left, a)?;
    unchanged(&right, b)?;
    Ok(equal)
}

// Simple filename wildcards, as in WinMerge: semicolon-separated patterns and
// !pattern exclusions. *.* also matches files without an extension.
fn matches_filter(name: &str, filter: &str) -> bool {
    let name: Vec<_> = name.to_lowercase().chars().collect();
    let patterns: Vec<_> = filter
        .split(';')
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .collect();
    let mut included = !patterns.iter().any(|pattern| !pattern.starts_with('!'));
    for pattern in patterns {
        let excluded = pattern.starts_with('!');
        let pattern = pattern.strip_prefix('!').unwrap_or(pattern);
        let matched = pattern == "*.*"
            || wildcard(&name, &pattern.to_lowercase().chars().collect::<Vec<_>>());
        if excluded && matched {
            return false;
        }
        included |= !excluded && matched;
    }
    included
}

fn wildcard(text: &[char], pattern: &[char]) -> bool {
    let (mut t, mut p, mut star, mut retry) = (0, 0, None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            retry = t;
        } else if let Some(index) = star {
            retry += 1;
            t = retry;
            p = index + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

pub fn compare(
    roots: [PathBuf; 2],
    options: CompareOptions,
    cancelled: &AtomicBool,
) -> Result<FolderComparison, String> {
    let left = scan(&roots[0], options.recursive, cancelled)?;
    let right = scan(&roots[1], options.recursive, cancelled)?;
    compare_snapshots(roots, options, cancelled, left, right)
}

fn compare_snapshots(
    roots: [PathBuf; 2],
    options: CompareOptions,
    cancelled: &AtomicBool,
    left: Snapshot,
    right: Snapshot,
) -> Result<FolderComparison, String> {
    let names: BTreeSet<_> = left
        .entries
        .keys()
        .chain(right.entries.keys())
        .cloned()
        .collect();
    let mut entries = Vec::new();
    for relative in names {
        check_cancel(cancelled)?;
        let (a, b) = (left.entries.get(&relative), right.entries.get(&relative));
        let left_info = a.and_then(|node| node.as_ref().ok()).cloned();
        let right_info = b.and_then(|node| node.as_ref().ok()).cloned();
        let is_dir = left_info
            .iter()
            .chain(right_info.iter())
            .any(|info| info.kind == EntryKind::Directory);
        if !is_dir
            && !matches_filter(
                &relative.file_name().unwrap_or_default().to_string_lossy(),
                &options.filter,
            )
        {
            continue;
        }
        let blocked =
            [(&left, "Left"), (&right, "Right")]
                .into_iter()
                .find_map(|(snapshot, side)| {
                    relative.ancestors().find_map(|path| {
                        snapshot
                            .blocked
                            .get(path)
                            .map(|error| format!("{side}: {error}"))
                    })
                });
        let result = if let Some(error) = blocked {
            Err(error)
        } else {
            match (a, b) {
                (Some(Err(error)), _) => Err(format!("Left: {error}")),
                (_, Some(Err(error))) => Err(format!("Right: {error}")),
                (Some(Ok(a)), Some(Ok(b))) if a.kind != b.kind => Ok(Status::TypeMismatch),
                (Some(Ok(a)), Some(Ok(b))) => match a.kind {
                    EntryKind::File => equal_files(
                        &roots[0].join(&relative),
                        &roots[1].join(&relative),
                        a,
                        b,
                        cancelled,
                    )
                    .map(|equal| {
                        if equal {
                            Status::Identical
                        } else {
                            Status::Different
                        }
                    }),
                    EntryKind::Link => fs::read_link(roots[0].join(&relative))
                        .and_then(|a| fs::read_link(roots[1].join(&relative)).map(|b| a == b))
                        .map(|equal| {
                            if equal {
                                Status::Identical
                            } else {
                                Status::Different
                            }
                        })
                        .map_err(|error| error.to_string()),
                    EntryKind::Directory => Ok(if options.recursive {
                        Status::Identical
                    } else {
                        Status::NotCompared
                    }),
                    EntryKind::Other => Err("Unsupported file type".into()),
                },
                (Some(_), None) => Ok(Status::LeftOnly),
                (None, Some(_)) => Ok(Status::RightOnly),
                (None, None) => unreachable!(),
            }
        };
        let (status, error) = match result {
            Ok(status) => (status, None),
            Err(error) => (Status::Error, Some(error)),
        };
        entries.push(FolderEntry {
            relative,
            left: left_info,
            right: right_info,
            status,
            error,
        });
    }
    check_cancel(cancelled)?;
    // A folder's status summarizes its included descendants, rather than
    // claiming identical just because it exists on both sides.
    let indices: BTreeMap<_, _> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| (entry.relative.clone(), i))
        .collect();
    for index in (0..entries.len()).rev() {
        let status = entries[index].status;
        if status == Status::Identical {
            continue;
        }
        if let Some(&parent) = entries[index]
            .relative
            .parent()
            .and_then(|path| indices.get(path))
        {
            let entry = &mut entries[parent];
            if entry
                .left
                .as_ref()
                .is_some_and(|info| info.kind == EntryKind::Directory)
                && entry
                    .right
                    .as_ref()
                    .is_some_and(|info| info.kind == EntryKind::Directory)
            {
                if status == Status::Error {
                    entry.status = Status::Error;
                    entry
                        .error
                        .get_or_insert_with(|| "A child item could not be compared".into());
                } else if entry.status != Status::Error {
                    entry.status = Status::Different;
                }
            }
        }
    }
    Ok(FolderComparison { roots, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folders() -> (tempfile::TempDir, [PathBuf; 2]) {
        let dir = tempfile::tempdir().unwrap();
        let roots = [dir.path().join("원본 폴더"), dir.path().join("수정 폴더")];
        for root in &roots {
            fs::create_dir(root).unwrap();
        }
        (dir, roots)
    }

    fn result(roots: [PathBuf; 2], options: CompareOptions) -> FolderComparison {
        compare(roots, options, &AtomicBool::new(false)).unwrap()
    }

    #[test]
    fn detects_exact_content_additions_deletions_and_binary_changes() {
        let (_dir, roots) = folders();
        for root in &roots {
            fs::write(root.join("같음.txt"), "동일\r\n").unwrap();
        }
        fs::write(roots[0].join("changed.bin"), [0, 1, 2]).unwrap();
        fs::write(roots[1].join("changed.bin"), [0, 1, 3]).unwrap();
        fs::write(roots[0].join("left.txt"), "left").unwrap();
        fs::write(roots[1].join("right.txt"), "right").unwrap();
        fs::write(roots[0].join("eol.txt"), "a\r\n").unwrap();
        fs::write(roots[1].join("eol.txt"), "a\n").unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        let statuses: BTreeMap<_, _> = comparison
            .entries
            .iter()
            .map(|entry| (entry.name(), entry.status))
            .collect();
        assert_eq!(statuses["같음.txt"], Status::Identical);
        assert_eq!(statuses["changed.bin"], Status::Different);
        assert_eq!(statuses["left.txt"], Status::LeftOnly);
        assert_eq!(statuses["right.txt"], Status::RightOnly);
        assert_eq!(statuses["eol.txt"], Status::Different);
        assert_eq!(fs::read(roots[0].join("changed.bin")).unwrap(), [0, 1, 2]);
        assert!(comparison.entries.iter().all(FolderEntry::can_open));
    }

    #[test]
    fn recurses_and_reports_empty_folders_and_file_folder_conflicts() {
        let (_dir, roots) = folders();
        for root in &roots {
            fs::create_dir(root.join("nested")).unwrap();
        }
        fs::write(roots[0].join("nested/change.h"), "old").unwrap();
        fs::write(roots[1].join("nested/change.h"), "new").unwrap();
        fs::create_dir(roots[0].join("empty")).unwrap();
        fs::write(roots[0].join("collision"), "file").unwrap();
        fs::create_dir(roots[1].join("collision")).unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        let nested = comparison
            .entries
            .iter()
            .find(|entry| entry.relative == Path::new("nested"))
            .unwrap();
        assert_eq!(nested.status, Status::Different);
        let collision = comparison
            .entries
            .iter()
            .find(|entry| entry.name() == "collision")
            .unwrap();
        assert_eq!(collision.status, Status::TypeMismatch);
        assert!(!collision.can_open());
        assert_eq!(
            comparison
                .entries
                .iter()
                .find(|entry| entry.name() == "empty")
                .unwrap()
                .status,
            Status::LeftOnly
        );
        let shallow = result(
            roots,
            CompareOptions {
                recursive: false,
                ..Default::default()
            },
        );
        assert!(
            !shallow
                .entries
                .iter()
                .any(|entry| entry.name() == "change.h")
        );
        assert_eq!(
            shallow
                .entries
                .iter()
                .find(|entry| entry.name() == "nested")
                .unwrap()
                .status,
            Status::NotCompared
        );
    }

    #[test]
    fn filename_filters_support_multiple_patterns_unicode_and_exclusions() {
        for name in ["a.h", "B.CPP", "헤더.h"] {
            assert!(matches_filter(name, "*.h;*.cpp"));
        }
        assert!(!matches_filter("a.txt", "*.h;*.cpp"));
        assert!(matches_filter("LICENSE", "*.*"));
        assert!(matches_filter("헤더.h", "??.h"));
        assert!(!matches_filter("cache.tmp", "*.*;!*.tmp"));
        assert!(matches_filter("source.rs", "!*.tmp"));
        assert!(matches_filter("a", ""));
        let (_dir, roots) = folders();
        for root in &roots {
            fs::write(root.join("keep.h"), "same").unwrap();
        }
        fs::write(roots[0].join("hidden.txt"), "left only").unwrap();
        let comparison = result(
            roots,
            CompareOptions {
                filter: "*.h".into(),
                ..Default::default()
            },
        );
        assert_eq!(comparison.entries.len(), 1);
        assert_eq!(comparison.entries[0].name(), "keep.h");
    }

    #[test]
    fn compares_large_files_by_content_and_supports_sorting_and_filtering() {
        let (_dir, roots) = folders();
        let mut bytes = vec![42; crate::diff::MAX_BYTES + 17];
        for root in &roots {
            fs::write(root.join("large.dat"), &bytes).unwrap();
        }
        assert_eq!(
            result(roots.clone(), CompareOptions::default()).entries[0].status,
            Status::Identical
        );
        *bytes.last_mut().unwrap() = 43;
        fs::write(roots[1].join("large.dat"), &bytes).unwrap();
        for root in &roots {
            fs::write(root.join("a.txt"), "same").unwrap();
        }
        let comparison = result(roots, CompareOptions::default());
        assert_eq!(comparison.visible(false, SortColumn::Name, false).len(), 1);
        let sorted = comparison.visible(true, SortColumn::Name, true);
        assert_eq!(comparison.entries[sorted[0]].name(), "large.dat");
        assert_eq!(comparison.entries[sorted[1]].name(), "a.txt");
    }

    #[test]
    fn rejects_invalid_roots_and_honors_cancellation() {
        let (_dir, roots) = folders();
        assert!(
            compare(
                roots.clone(),
                CompareOptions::default(),
                &AtomicBool::new(true)
            )
            .is_err()
        );
        assert!(
            compare(
                [roots[0].clone(), roots[1].join("missing")],
                CompareOptions::default(),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        fs::write(roots[1].join("file"), "file").unwrap();
        assert!(
            compare(
                [roots[0].clone(), roots[1].join("file")],
                CompareOptions::default(),
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }

    #[test]
    fn unreadable_subtree_is_an_error_instead_of_missing_files() {
        let (_dir, roots) = folders();
        for root in &roots {
            fs::create_dir(root.join("blocked")).unwrap();
        }
        fs::write(roots[1].join("blocked/right.txt"), "right").unwrap();
        let cancelled = AtomicBool::new(false);
        let mut left = scan(&roots[0], true, &cancelled).unwrap();
        left.blocked
            .insert("blocked".into(), "Permission denied".into());
        let right = scan(&roots[1], true, &cancelled).unwrap();
        let comparison =
            compare_snapshots(roots, CompareOptions::default(), &cancelled, left, right).unwrap();
        assert_eq!(comparison.entries.len(), 2);
        assert!(
            comparison
                .entries
                .iter()
                .all(|entry| entry.status == Status::Error && !entry.can_open())
        );
        assert!(
            comparison.entries.iter().all(|entry| entry
                .error
                .as_ref()
                .unwrap()
                .contains("Permission denied"))
        );
    }

    #[test]
    fn detects_files_modified_since_directory_scan() {
        let (_dir, roots) = folders();
        for root in &roots {
            fs::write(root.join("file.txt"), "old").unwrap();
        }
        let cancelled = AtomicBool::new(false);
        let left = scan(&roots[0], true, &cancelled).unwrap();
        let right = scan(&roots[1], true, &cancelled).unwrap();
        fs::write(roots[0].join("file.txt"), "new contents").unwrap();
        let comparison =
            compare_snapshots(roots, CompareOptions::default(), &cancelled, left, right).unwrap();
        assert_eq!(comparison.entries[0].status, Status::Error);
        assert!(
            comparison.entries[0]
                .error
                .as_ref()
                .unwrap()
                .contains("File changed during comparison")
        );
    }

    fn file_index(comparison: &FolderComparison, relative: &str) -> usize {
        comparison
            .entries
            .iter()
            .position(|entry| entry.relative == Path::new(relative))
            .unwrap()
    }

    #[test]
    fn copies_selected_binary_file_in_both_directions_without_touching_other_files() {
        let (_dir, roots) = folders();
        fs::write(
            roots[0].join("data.bin"),
            [0xef, 0xbb, 0xbf, 0, 13, 10, 255],
        )
        .unwrap();
        fs::write(roots[1].join("data.bin"), [0, 1, 2]).unwrap();
        fs::write(roots[1].join("untouched.txt"), "keep").unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        let index = file_index(&comparison, "data.bin");
        assert!(comparison.entries[index].can_copy(CopyDirection::LeftToRight));
        assert!(comparison.entries[index].can_copy(CopyDirection::RightToLeft));
        assert_eq!(
            comparison
                .copy_file(index, CopyDirection::LeftToRight, &AtomicBool::new(false))
                .unwrap(),
            7
        );
        let bytes = fs::read(roots[0].join("data.bin")).unwrap();
        assert_eq!(fs::read(roots[1].join("data.bin")).unwrap(), bytes);
        let comparison = result(roots.clone(), CompareOptions::default());
        assert_eq!(
            comparison.entries[file_index(&comparison, "data.bin")].status,
            Status::Identical
        );
        fs::write(roots[1].join("data.bin"), "수정\r\n").unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        comparison
            .copy_file(
                file_index(&comparison, "data.bin"),
                CopyDirection::RightToLeft,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            fs::read(roots[0].join("data.bin")).unwrap(),
            "수정\r\n".as_bytes()
        );
        assert_eq!(
            fs::read_to_string(roots[1].join("untouched.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn copies_one_sided_large_and_empty_files_and_creates_missing_parents() {
        let (_dir, roots) = folders();
        fs::create_dir_all(roots[0].join("하위/폴더")).unwrap();
        let bytes = vec![171; crate::diff::MAX_BYTES + 65_537];
        fs::write(roots[0].join("하위/폴더/파일.dat"), &bytes).unwrap();
        fs::write(roots[1].join("empty.txt"), "").unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        let index = file_index(&comparison, "하위/폴더/파일.dat");
        assert!(!comparison.entries[index].can_copy(CopyDirection::RightToLeft));
        assert!(
            comparison
                .copy_file(index, CopyDirection::RightToLeft, &AtomicBool::new(false))
                .is_err()
        );
        assert_eq!(
            comparison
                .copy_file(index, CopyDirection::LeftToRight, &AtomicBool::new(false))
                .unwrap(),
            bytes.len() as u64
        );
        assert_eq!(
            fs::read(roots[1].join("하위/폴더/파일.dat")).unwrap(),
            bytes
        );
        comparison
            .copy_file(
                file_index(&comparison, "empty.txt"),
                CopyDirection::RightToLeft,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(fs::read(roots[0].join("empty.txt")).unwrap(), b"");
    }

    #[test]
    fn copy_refuses_stale_source_destination_and_unexpected_new_destination() {
        for side in 0..2 {
            let (_dir, roots) = folders();
            for root in &roots {
                fs::write(root.join("file.txt"), "old").unwrap();
            }
            let comparison = result(roots.clone(), CompareOptions::default());
            fs::write(roots[side].join("file.txt"), "changed externally").unwrap();
            let before = fs::read(roots[1].join("file.txt")).unwrap();
            assert!(
                comparison
                    .copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(false))
                    .is_err()
            );
            assert_eq!(fs::read(roots[1].join("file.txt")).unwrap(), before);
        }
        let (_dir, roots) = folders();
        fs::write(roots[0].join("file.txt"), "source").unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        fs::write(roots[1].join("file.txt"), "created externally").unwrap();
        assert!(
            comparison
                .copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(false))
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(roots[1].join("file.txt")).unwrap(),
            "created externally"
        );
    }

    #[test]
    fn readonly_or_canceled_copy_preserves_destination_and_leaves_no_temporary_files() {
        let (_dir, roots) = folders();
        fs::write(roots[0].join("file.txt"), "source").unwrap();
        let target = roots[1].join("file.txt");
        fs::write(&target, "target").unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        let permissions = fs::metadata(&target).unwrap().permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&target, readonly).unwrap();
        let copied = comparison.copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(false));
        fs::set_permissions(&target, permissions).unwrap();
        assert!(copied.unwrap_err().contains("read-only"));
        assert!(
            comparison
                .copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(true))
                .is_err()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "target");
        assert_eq!(fs::read_dir(&roots[1]).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn failed_atomic_replacement_keeps_original_and_cleans_temporary_file() {
        use std::os::windows::fs::OpenOptionsExt;
        let (_dir, roots) = folders();
        let source = roots[0].join("file.txt");
        let target = roots[1].join("file.txt");
        fs::write(&source, "new source bytes").unwrap();
        fs::write(&target, "original destination").unwrap();
        let permissions = fs::metadata(&source).unwrap().permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&source, readonly).unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        // Let readers open the destination while another application prevents
        // replacement by withholding FILE_SHARE_DELETE.
        let _lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&target)
            .unwrap();
        let copied = comparison.copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(false));
        fs::set_permissions(&source, permissions).unwrap();
        assert!(copied.is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "original destination");
        assert_eq!(fs::read_dir(&roots[1]).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn copy_preserves_source_execute_bits_for_new_files_and_existing_target_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, roots) = folders();
        let source = roots[0].join("script.sh");
        let target = roots[1].join("script.sh");
        fs::write(&source, "#!/bin/sh\necho hello\n").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        comparison
            .copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        let comparison = result(roots, CompareOptions::default());
        comparison
            .copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn copy_rejects_type_conflicts_shared_paths_and_escaping_relative_paths() {
        let (_dir, roots) = folders();
        fs::write(roots[0].join("collision"), "source").unwrap();
        fs::create_dir(roots[1].join("collision")).unwrap();
        let comparison = result(roots.clone(), CompareOptions::default());
        assert!(!comparison.entries[0].can_copy(CopyDirection::LeftToRight));
        assert!(
            comparison
                .copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(false))
                .is_err()
        );
        assert!(roots[1].join("collision").is_dir());
        let comparison = result(
            [roots[0].clone(), roots[0].clone()],
            CompareOptions::default(),
        );
        assert!(
            comparison
                .copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(false))
                .unwrap_err()
                .contains("same file")
        );
        let mut comparison = result(roots, CompareOptions::default());
        comparison.entries[0].right = None;
        comparison.entries[0].relative = "../escape.txt".into();
        assert!(
            comparison
                .copy_file(0, CopyDirection::LeftToRight, &AtomicBool::new(false))
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_refuses_symbolic_link_substitution_on_either_side() {
        use std::os::unix::fs::symlink;
        for side in 0..2 {
            let (dir, roots) = folders();
            for root in &roots {
                fs::create_dir(root.join("nested")).unwrap();
                fs::write(root.join("nested/file.txt"), "old").unwrap();
            }
            let comparison = result(roots.clone(), CompareOptions::default());
            let outside = dir.path().join("outside");
            fs::create_dir(&outside).unwrap();
            fs::write(outside.join("file.txt"), "outside unchanged").unwrap();
            fs::rename(roots[side].join("nested"), roots[side].join("original")).unwrap();
            symlink(&outside, roots[side].join("nested")).unwrap();
            assert!(
                comparison
                    .copy_file(
                        file_index(&comparison, "nested/file.txt"),
                        CopyDirection::LeftToRight,
                        &AtomicBool::new(false)
                    )
                    .is_err()
            );
            assert_eq!(
                fs::read_to_string(outside.join("file.txt")).unwrap(),
                "outside unchanged"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_compare_targets_without_following_loops_or_dangling_links() {
        use std::os::unix::fs::symlink;
        let (_dir, roots) = folders();
        for root in &roots {
            symlink(".", root.join("loop")).unwrap();
            symlink("missing", root.join("dangling")).unwrap();
        }
        symlink("a", roots[0].join("different")).unwrap();
        symlink("b", roots[1].join("different")).unwrap();
        let comparison = result(roots, CompareOptions::default());
        assert_eq!(comparison.entries.len(), 3);
        assert_eq!(
            comparison
                .entries
                .iter()
                .find(|entry| entry.name() == "different")
                .unwrap()
                .status,
            Status::Different
        );
        assert!(comparison.entries.iter().all(|entry| !entry.can_open()));
    }
}
