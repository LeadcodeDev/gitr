use std::path::PathBuf;

/// What happened to one file between two trees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    /// `similarity` is Git's percentage, 0 to 100.
    Renamed {
        similarity: u8,
    },
    Copied {
        similarity: u8,
    },
    TypeChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineOrigin {
    Context,
    Addition,
    Deletion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub origin: LineOrigin,
    /// Line number in the pre-image, absent for an addition.
    pub old_number: Option<u32>,
    /// Line number in the post-image, absent for a deletion.
    pub new_number: Option<u32>,
    /// Without the leading `+`, `-` or space, and without the trailing newline.
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// The text after the `@@ ... @@` marker — Git's guess at the enclosing function.
    pub heading: String,
    pub lines: Vec<DiffLine>,
}

/// One file's changes.
///
/// `hunks` is empty for a binary file or for a pure rename; `is_binary` distinguishes
/// the first case from the second, which a caller cannot infer from emptiness alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilePatch {
    /// Absent for an added file.
    pub old_path: Option<PathBuf>,
    /// Absent for a deleted file.
    pub new_path: Option<PathBuf>,
    pub status: FileStatus,
    pub is_binary: bool,
    pub hunks: Vec<Hunk>,
}

impl FilePatch {
    /// The path to show in a file list: the post-image where there is one.
    pub fn display_path(&self) -> Option<&PathBuf> {
        self.new_path.as_ref().or(self.old_path.as_ref())
    }

    pub fn added_lines(&self) -> usize {
        self.count(LineOrigin::Addition)
    }

    pub fn deleted_lines(&self) -> usize {
        self.count(LineOrigin::Deletion)
    }

    fn count(&self, origin: LineOrigin) -> usize {
        self.hunks
            .iter()
            .flat_map(|hunk| hunk.lines.iter())
            .filter(|line| line.origin == origin)
            .count()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Patch {
    pub files: Vec<FilePatch>,
}

impl Patch {
    pub fn added_lines(&self) -> usize {
        self.files.iter().map(FilePatch::added_lines).sum()
    }

    pub fn deleted_lines(&self) -> usize {
        self.files.iter().map(FilePatch::deleted_lines).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(origin: LineOrigin) -> DiffLine {
        DiffLine {
            origin,
            old_number: None,
            new_number: None,
            content: String::new(),
        }
    }

    fn file_patch(lines: Vec<DiffLine>) -> FilePatch {
        FilePatch {
            old_path: Some(PathBuf::from("a.rs")),
            new_path: Some(PathBuf::from("b.rs")),
            status: FileStatus::Modified,
            is_binary: false,
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 1,
                heading: String::new(),
                lines,
            }],
        }
    }

    #[test]
    fn counts_additions_and_deletions_separately() {
        let patch = file_patch(vec![
            line(LineOrigin::Addition),
            line(LineOrigin::Addition),
            line(LineOrigin::Deletion),
            line(LineOrigin::Context),
        ]);
        assert_eq!(patch.added_lines(), 2);
        assert_eq!(patch.deleted_lines(), 1);
    }

    #[test]
    fn totals_across_files() {
        let patch = Patch {
            files: vec![
                file_patch(vec![line(LineOrigin::Addition)]),
                file_patch(vec![line(LineOrigin::Addition), line(LineOrigin::Deletion)]),
            ],
        };
        assert_eq!(patch.added_lines(), 2);
        assert_eq!(patch.deleted_lines(), 1);
    }

    #[test]
    fn display_path_prefers_the_post_image() {
        let mut patch = file_patch(vec![]);
        assert_eq!(patch.display_path(), Some(&PathBuf::from("b.rs")));
        patch.new_path = None;
        assert_eq!(patch.display_path(), Some(&PathBuf::from("a.rs")));
    }
}
