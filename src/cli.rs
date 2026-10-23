use crate::merge::MergePaths;
use std::{ffi::OsString, path::PathBuf};

pub const USAGE: &str = "rdiff — file/folder comparison and three-way merge

Usage:
  rdiff                                  Open an empty diff window
  rdiff diff <path1> <path2>               Compare two files or folders
  rdiff folders [<folder1> <folder2>]      Compare folders, or choose them in the window
  rdiff merge <source> <target> <base> <result>
  rdiff <file1> <file2>                    Compare two files (legacy)
  rdiff self-update --check               Check GitHub Releases
  rdiff self-update                       Install the latest stable release

P4V arguments:
  diff \"%1\" \"%2\"
  merge \"%1\" \"%2\" \"%b\" \"%r\"

Merge exit codes: 0 = saved, 1 = canceled/unsaved, 2 = error.
Quote paths containing spaces. Only an explicit Save writes the result file.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Update { check: bool },
    Diff(Vec<PathBuf>),
    Folders(Vec<PathBuf>),
    Merge(MergePaths),
    Help,
}

pub fn parse(args: Vec<OsString>) -> Result<Command, String> {
    let Some(first) = args.first() else {
        return Ok(Command::Diff(Vec::new()));
    };
    if first == "self-update" {
        return match args.as_slice() {
            [_] => Ok(Command::Update { check: false }),
            [_, flag] if flag == "--check" => Ok(Command::Update { check: true }),
            _ => Err("Usage: rdiff self-update [--check]".into()),
        };
    }
    if first == "--help" || first == "-h" {
        return if args.len() == 1 {
            Ok(Command::Help)
        } else {
            Err("--help does not accept file arguments.".into())
        };
    }
    if first == "folders" || first == "folder" {
        return match args.len() {
            1 => Ok(Command::Folders(Vec::new())),
            3 => Ok(Command::Folders(
                args[1..].iter().map(PathBuf::from).collect(),
            )),
            _ => Err("folders requires two folder paths, or no arguments".into()),
        };
    }
    if first == "diff" {
        return if args.len() == 3 {
            Ok(Command::Diff(args[1..].iter().map(PathBuf::from).collect()))
        } else {
            Err("diff requires exactly two paths: diff <path1> <path2>".into())
        };
    }
    if first == "merge" {
        return if args.len() == 5 {
            Ok(Command::Merge(MergePaths {
                source: args[1].clone().into(),
                target: args[2].clone().into(),
                base: args[3].clone().into(),
                result: args[4].clone().into(),
            }))
        } else {
            Err("merge requires four paths in this order: source target base result".into())
        };
    }
    if args.len() == 2 {
        Ok(Command::Diff(args.into_iter().map(PathBuf::from).collect()))
    } else {
        Err("Unrecognized command or incorrect number of file paths.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn update_command_is_explicit() {
        assert_eq!(
            parse(args(&["self-update"])),
            Ok(Command::Update { check: false })
        );
        assert_eq!(
            parse(args(&["self-update", "--check"])),
            Ok(Command::Update { check: true })
        );
        assert!(parse(args(&["self-update", "--force"])).is_err());
        assert!(parse(args(&["self-update", "--check", "extra"])).is_err());
    }

    #[test]
    fn explicit_diff_and_legacy_preserve_paths() {
        let paths = vec![PathBuf::from("C:/내 파일/one.rs"), PathBuf::from("two.rs")];
        assert_eq!(
            parse(args(&["diff", "C:/내 파일/one.rs", "two.rs"])),
            Ok(Command::Diff(paths.clone()))
        );
        assert_eq!(
            parse(args(&["C:/내 파일/one.rs", "two.rs"])),
            Ok(Command::Diff(paths))
        );
        assert_eq!(parse(vec![]), Ok(Command::Diff(vec![])));
    }

    #[test]
    fn merge_maps_perforce_argument_order() {
        assert_eq!(
            parse(args(&[
                "merge",
                "theirs",
                "yours",
                "ancestor",
                "result path"
            ])),
            Ok(Command::Merge(MergePaths {
                source: "theirs".into(),
                target: "yours".into(),
                base: "ancestor".into(),
                result: "result path".into(),
            }))
        );
    }

    #[test]
    fn folder_command_supports_picker_and_preserves_unicode_paths() {
        for command in ["folder", "folders"] {
            assert_eq!(parse(args(&[command])), Ok(Command::Folders(vec![])));
            assert_eq!(
                parse(args(&[command, "C:/원본 폴더", "C:/수정 폴더"])),
                Ok(Command::Folders(vec![
                    "C:/원본 폴더".into(),
                    "C:/수정 폴더".into()
                ]))
            );
            assert!(parse(args(&[command, "a"])).is_err());
            assert!(parse(args(&[command, "a", "b", "c"])).is_err());
        }
    }

    #[test]
    fn rejects_missing_and_extra_arguments() {
        for values in [
            &["diff"][..],
            &["diff", "a"],
            &["diff", "a", "b", "c"],
            &["merge", "a", "b", "c"],
            &["merge", "a", "b", "c", "d", "e"],
        ] {
            assert!(parse(args(values)).is_err(), "{values:?}");
        }
        assert_eq!(parse(args(&["--help"])), Ok(Command::Help));
    }
}
