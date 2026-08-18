//! Parses `git diff` output into [`Patch`].
//!
//! A pure function from `&str` to `Patch`: no subprocess, no filesystem. Real Git
//! output only ever comes from [`super::patch_reader`], invoked with `--no-color` and
//! `core.quotePath=false` so a user's own configuration cannot change the shape this
//! parser has to understand.

use std::iter::Peekable;
use std::path::PathBuf;
use std::str::Lines;

use domain::error::PatchError;
use domain::patch::{DiffLine, FilePatch, FileStatus, Hunk, LineOrigin, Patch};

const NO_NEWLINE_MARKER: &str = "\\ No newline at end of file";

pub(crate) fn parse_patch(text: &str) -> Result<Patch, PatchError> {
    let mut lines = text.lines().peekable();
    let mut files = Vec::new();
    while let Some(&line) = lines.peek() {
        match line.strip_prefix("diff --git ") {
            Some(header) => {
                lines.next();
                files.push(parse_file(header, &mut lines)?);
            }
            None => return Err(malformed(line)),
        }
    }
    Ok(Patch { files })
}

fn malformed(line: &str) -> PatchError {
    PatchError::Malformed(line.to_string())
}

struct FileHeader {
    old_path: Option<PathBuf>,
    new_path: Option<PathBuf>,
    is_binary: bool,
    similarity: Option<u8>,
    is_rename: bool,
    is_copy: bool,
    is_new_file: bool,
    is_deleted_file: bool,
}

fn parse_file(header: &str, lines: &mut Peekable<Lines<'_>>) -> Result<FilePatch, PatchError> {
    let (fallback_old, fallback_new) = parse_diff_git_header(header)?;
    let mut header_state = FileHeader {
        old_path: Some(fallback_old),
        new_path: Some(fallback_new),
        is_binary: false,
        similarity: None,
        is_rename: false,
        is_copy: false,
        is_new_file: false,
        is_deleted_file: false,
    };
    let mut hunks = Vec::new();

    while let Some(&line) = lines.peek() {
        if line.starts_with("diff --git ") {
            break;
        }
        if let Some(rest) = line.strip_prefix("@@ ") {
            hunks.push(parse_hunk(rest, lines)?);
            continue;
        }
        lines.next();
        if line.starts_with("old mode ") || line.starts_with("new mode ") {
        } else if line.starts_with("new file mode ") {
            header_state.is_new_file = true;
        } else if line.starts_with("deleted file mode ") {
            header_state.is_deleted_file = true;
        } else if let Some(rest) = line.strip_prefix("similarity index ") {
            header_state.similarity = Some(parse_percentage(rest, line)?);
        } else if line.starts_with("dissimilarity index ") {
        } else if let Some(path) = line.strip_prefix("rename from ") {
            header_state.is_rename = true;
            header_state.old_path = Some(PathBuf::from(path));
        } else if let Some(path) = line.strip_prefix("rename to ") {
            header_state.is_rename = true;
            header_state.new_path = Some(PathBuf::from(path));
        } else if let Some(path) = line.strip_prefix("copy from ") {
            header_state.is_copy = true;
            header_state.old_path = Some(PathBuf::from(path));
        } else if let Some(path) = line.strip_prefix("copy to ") {
            header_state.is_copy = true;
            header_state.new_path = Some(PathBuf::from(path));
        } else if line.starts_with("index ") {
        } else if line.starts_with("Binary files ") {
            header_state.is_binary = true;
        } else if let Some(rest) = line.strip_prefix("--- ") {
            header_state.old_path = parse_prefixed_path(rest, "a/");
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            header_state.new_path = parse_prefixed_path(rest, "b/");
        } else {
            return Err(malformed(line));
        }
    }

    if header_state.is_new_file {
        header_state.old_path = None;
    }
    if header_state.is_deleted_file {
        header_state.new_path = None;
    }

    let status = if header_state.is_rename {
        FileStatus::Renamed {
            similarity: header_state.similarity.unwrap_or(100),
        }
    } else if header_state.is_copy {
        FileStatus::Copied {
            similarity: header_state.similarity.unwrap_or(100),
        }
    } else if header_state.is_new_file {
        FileStatus::Added
    } else if header_state.is_deleted_file {
        FileStatus::Deleted
    } else {
        FileStatus::Modified
    };

    Ok(FilePatch {
        old_path: header_state.old_path,
        new_path: header_state.new_path,
        status,
        is_binary: header_state.is_binary,
        hunks,
    })
}

fn parse_diff_git_header(header: &str) -> Result<(PathBuf, PathBuf), PatchError> {
    let rest = header.strip_prefix("a/").ok_or_else(|| malformed(header))?;
    let split_at = rest.find(" b/").ok_or_else(|| malformed(header))?;
    let old = PathBuf::from(&rest[..split_at]);
    let new = PathBuf::from(&rest[split_at + " b/".len()..]);
    Ok((old, new))
}

fn parse_prefixed_path(remainder: &str, prefix: &str) -> Option<PathBuf> {
    if remainder == "/dev/null" {
        None
    } else {
        Some(PathBuf::from(
            remainder.strip_prefix(prefix).unwrap_or(remainder),
        ))
    }
}

fn parse_percentage(value: &str, line: &str) -> Result<u8, PatchError> {
    value
        .strip_suffix('%')
        .ok_or_else(|| malformed(line))?
        .parse::<u8>()
        .map_err(|_| malformed(line))
}

fn parse_hunk(header_rest: &str, lines: &mut Peekable<Lines<'_>>) -> Result<Hunk, PatchError> {
    let full_header = format!("@@ {header_rest}");
    let (old_start, old_lines, new_start, new_lines, heading) =
        parse_hunk_header(header_rest).ok_or_else(|| malformed(&full_header))?;
    lines.next();

    let mut old_cursor = old_start;
    let mut new_cursor = new_start;
    let mut old_remaining = old_lines;
    let mut new_remaining = new_lines;
    let mut body = Vec::new();

    while old_remaining > 0 || new_remaining > 0 {
        let line = lines.next().ok_or_else(|| malformed(&full_header))?;
        if line == NO_NEWLINE_MARKER {
            continue;
        }
        match line.as_bytes().first() {
            Some(b' ') => {
                if old_remaining == 0 || new_remaining == 0 {
                    return Err(malformed(line));
                }
                body.push(DiffLine {
                    origin: LineOrigin::Context,
                    old_number: Some(old_cursor),
                    new_number: Some(new_cursor),
                    content: line[1..].to_string(),
                });
                old_cursor += 1;
                new_cursor += 1;
                old_remaining -= 1;
                new_remaining -= 1;
            }
            Some(b'+') => {
                if new_remaining == 0 {
                    return Err(malformed(line));
                }
                body.push(DiffLine {
                    origin: LineOrigin::Addition,
                    old_number: None,
                    new_number: Some(new_cursor),
                    content: line[1..].to_string(),
                });
                new_cursor += 1;
                new_remaining -= 1;
            }
            Some(b'-') => {
                if old_remaining == 0 {
                    return Err(malformed(line));
                }
                body.push(DiffLine {
                    origin: LineOrigin::Deletion,
                    old_number: Some(old_cursor),
                    new_number: None,
                    content: line[1..].to_string(),
                });
                old_cursor += 1;
                old_remaining -= 1;
            }
            _ => return Err(malformed(line)),
        }
    }

    while matches!(lines.peek(), Some(&next) if next == NO_NEWLINE_MARKER) {
        lines.next();
    }

    Ok(Hunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        heading,
        lines: body,
    })
}

fn parse_hunk_header(rest: &str) -> Option<(u32, u32, u32, u32, String)> {
    let end = rest.find(" @@")?;
    let range_part = &rest[..end];
    let remainder = &rest[end + " @@".len()..];
    let heading = remainder.strip_prefix(' ').unwrap_or(remainder).to_string();

    let mut parts = range_part.split(' ');
    let old_part = parts.next()?;
    let new_part = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let (old_start, old_lines) = parse_range(old_part, '-')?;
    let (new_start, new_lines) = parse_range(new_part, '+')?;
    Some((old_start, old_lines, new_start, new_lines, heading))
}

fn parse_range(part: &str, sign: char) -> Option<(u32, u32)> {
    let stripped = part.strip_prefix(sign)?;
    let (start_str, lines_str) = match stripped.split_once(',') {
        Some((start, lines)) => (start, lines),
        None => (stripped, "1"),
    };
    let start = start_str.parse::<u32>().ok()?;
    let lines = lines_str.parse::<u32>().ok()?;
    Some((start, lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_diff_yields_an_empty_patch() {
        let patch = parse_patch("").unwrap();
        assert_eq!(patch, Patch::default());
    }

    #[test]
    fn single_file_modification_tracks_line_numbers_exactly() {
        let text = "\
diff --git a/f.txt b/f.txt
index a29bdeb..c0d0fb4 100644
--- a/f.txt
+++ b/f.txt
@@ -1 +1,2 @@
 line1
+line2
";
        let patch = parse_patch(text).unwrap();
        assert_eq!(patch.files.len(), 1);
        let file = &patch.files[0];
        assert_eq!(file.old_path, Some(PathBuf::from("f.txt")));
        assert_eq!(file.new_path, Some(PathBuf::from("f.txt")));
        assert_eq!(file.status, FileStatus::Modified);
        assert!(!file.is_binary);
        assert_eq!(file.hunks.len(), 1);
        let hunk = &file.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_lines, 1);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_lines, 2);
        assert_eq!(
            hunk.lines,
            vec![
                DiffLine {
                    origin: LineOrigin::Context,
                    old_number: Some(1),
                    new_number: Some(1),
                    content: "line1".to_string(),
                },
                DiffLine {
                    origin: LineOrigin::Addition,
                    old_number: None,
                    new_number: Some(2),
                    content: "line2".to_string(),
                },
            ]
        );
    }

    #[test]
    fn multiple_hunks_in_one_file_each_track_their_own_line_numbers() {
        let text = "\
diff --git a/f.txt b/f.txt
index 19339a3..85af29a 100644
--- a/f.txt
+++ b/f.txt
@@ -1,5 +1,5 @@
 line1
-line2
+line2-CHANGED
 line3
 line4
 line5
@@ -25,6 +25,6 @@ line24
 line25
 line26
 line27
-line28
+line28-CHANGED
 line29
 line30
";
        let patch = parse_patch(text).unwrap();
        let file = &patch.files[0];
        assert_eq!(file.hunks.len(), 2);
        assert_eq!(file.hunks[0].old_start, 1);
        assert_eq!(file.hunks[0].heading, "");
        assert_eq!(file.hunks[1].old_start, 25);
        assert_eq!(file.hunks[1].heading, "line24");
        assert_eq!(
            file.hunks[1].lines[3],
            DiffLine {
                origin: LineOrigin::Deletion,
                old_number: Some(28),
                new_number: None,
                content: "line28".to_string(),
            }
        );
        assert_eq!(
            file.hunks[1].lines[4],
            DiffLine {
                origin: LineOrigin::Addition,
                old_number: None,
                new_number: Some(28),
                content: "line28-CHANGED".to_string(),
            }
        );
    }

    #[test]
    fn multiple_files_in_one_patch_are_each_parsed() {
        let text = "\
diff --git a/f.txt b/f.txt
index a29bdeb..c0d0fb4 100644
--- a/f.txt
+++ b/f.txt
@@ -1 +1,2 @@
 line1
+line2
diff --git a/g.txt b/g.txt
index e69de29..b66ba06 100644
--- a/g.txt
+++ b/g.txt
@@ -0,0 +1 @@
+hello
";
        let patch = parse_patch(text).unwrap();
        assert_eq!(patch.files.len(), 2);
        assert_eq!(patch.files[0].display_path(), Some(&PathBuf::from("f.txt")));
        assert_eq!(patch.files[1].display_path(), Some(&PathBuf::from("g.txt")));
    }

    #[test]
    fn added_file_has_no_old_path() {
        let text = "\
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..b66ba06
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+new content
";
        let patch = parse_patch(text).unwrap();
        let file = &patch.files[0];
        assert_eq!(file.status, FileStatus::Added);
        assert_eq!(file.old_path, None);
        assert_eq!(file.new_path, Some(PathBuf::from("new.txt")));
    }

    #[test]
    fn deleted_file_has_no_new_path() {
        let text = "\
diff --git a/h.txt b/h.txt
deleted file mode 100755
index e903805..0000000
--- a/h.txt
+++ /dev/null
@@ -1,4 +0,0 @@
-l1
-l2
-l3-changed
-l4-new
";
        let patch = parse_patch(text).unwrap();
        let file = &patch.files[0];
        assert_eq!(file.status, FileStatus::Deleted);
        assert_eq!(file.old_path, Some(PathBuf::from("h.txt")));
        assert_eq!(file.new_path, None);
        assert_eq!(file.hunks[0].lines.len(), 4);
    }

    #[test]
    fn rename_without_content_change_has_no_hunks() {
        let text = "\
diff --git a/f.txt b/g.txt
similarity index 100%
rename from f.txt
rename to g.txt
";
        let patch = parse_patch(text).unwrap();
        let file = &patch.files[0];
        assert_eq!(file.status, FileStatus::Renamed { similarity: 100 });
        assert_eq!(file.old_path, Some(PathBuf::from("f.txt")));
        assert_eq!(file.new_path, Some(PathBuf::from("g.txt")));
        assert!(file.hunks.is_empty());
        assert!(!file.is_binary);
    }

    #[test]
    fn rename_with_content_change_carries_similarity_and_hunks() {
        let text = "\
diff --git a/f.txt b/g.txt
similarity index 40%
rename from f.txt
rename to g.txt
index b8cb000..1b2654c 100644
--- a/f.txt
+++ b/g.txt
@@ -1,5 +1,6 @@
 l1
 l2
-l3
+l3-changed
 l4
 l5
+l6-new
";
        let patch = parse_patch(text).unwrap();
        let file = &patch.files[0];
        assert_eq!(file.status, FileStatus::Renamed { similarity: 40 });
        assert_eq!(file.hunks.len(), 1);
        assert_eq!(file.hunks[0].lines.len(), 7);
    }

    #[test]
    fn binary_file_is_marked_binary_with_no_hunks() {
        let text = "\
diff --git a/bin.dat b/bin.dat
new file mode 100644
index 0000000..0f49c4a
Binary files /dev/null and b/bin.dat differ
";
        let patch = parse_patch(text).unwrap();
        let file = &patch.files[0];
        assert!(file.is_binary);
        assert!(file.hunks.is_empty());
        assert_eq!(file.status, FileStatus::Added);
        assert_eq!(file.old_path, None);
        assert_eq!(file.new_path, Some(PathBuf::from("bin.dat")));
    }

    #[test]
    fn binary_modification_is_distinguishable_from_a_pure_rename_by_is_binary() {
        let binary_text = "\
diff --git a/bin.dat b/bin.dat
index 9fc36a0..4e2acb6 100644
Binary files a/bin.dat and b/bin.dat differ
";
        let rename_text = "\
diff --git a/f.txt b/g.txt
similarity index 100%
rename from f.txt
rename to g.txt
";
        let binary_patch = parse_patch(binary_text).unwrap();
        let rename_patch = parse_patch(rename_text).unwrap();
        assert!(binary_patch.files[0].hunks.is_empty());
        assert!(rename_patch.files[0].hunks.is_empty());
        assert!(binary_patch.files[0].is_binary);
        assert!(!rename_patch.files[0].is_binary);
    }

    #[test]
    fn no_trailing_newline_marker_does_not_produce_a_spurious_line() {
        let text = "\
diff --git a/f.txt b/f.txt
index f0f2307..eb9b40d 100644
--- a/f.txt
+++ b/f.txt
@@ -1,3 +1,3 @@
 l1
 l2
-l3
+l3-changed
\\ No newline at end of file
";
        let patch = parse_patch(text).unwrap();
        let hunk = &patch.files[0].hunks[0];
        assert_eq!(hunk.lines.len(), 4);
        assert_eq!(hunk.lines.last().unwrap().content, "l3-changed");
    }

    #[test]
    fn no_trailing_newline_can_appear_on_both_old_and_new_sides() {
        let text = "\
diff --git a/f.txt b/f.txt
index aaa..bbb 100644
--- a/f.txt
+++ b/f.txt
@@ -1 +1 @@
-old-no-newline
\\ No newline at end of file
+new-no-newline
\\ No newline at end of file
";
        let patch = parse_patch(text).unwrap();
        let hunk = &patch.files[0].hunks[0];
        assert_eq!(hunk.lines.len(), 2);
        assert_eq!(hunk.lines[0].content, "old-no-newline");
        assert_eq!(hunk.lines[1].content, "new-no-newline");
    }

    #[test]
    fn mode_change_alone_has_no_hunks_and_stays_modified() {
        let text = "\
diff --git a/f.txt b/f.txt
old mode 100644
new mode 100755
";
        let patch = parse_patch(text).unwrap();
        let file = &patch.files[0];
        assert_eq!(file.status, FileStatus::Modified);
        assert!(file.hunks.is_empty());
        assert!(!file.is_binary);
    }

    #[test]
    fn hunk_header_carries_the_function_heading() {
        let text = "\
diff --git a/f.c b/f.c
index bc5cce4..3fc03bd 100644
--- a/f.c
+++ b/f.c
@@ -5,5 +5,5 @@ int foo() {
\x20
 int bar() {
     int y = 2;
-    return y;
+    return y + 1;
 }
";
        let patch = parse_patch(text).unwrap();
        assert_eq!(patch.files[0].hunks[0].heading, "int foo() {");
    }

    #[test]
    fn merge_commit_first_parent_diff_parses_like_any_other_diff() {
        let text = "\
diff --git a/b.txt b/b.txt
new file mode 100644
index 0000000..df0fa74
--- /dev/null
+++ b/b.txt
@@ -0,0 +1 @@
+sidefile
";
        let patch = parse_patch(text).unwrap();
        assert_eq!(patch.files.len(), 1);
        assert_eq!(patch.files[0].status, FileStatus::Added);
        assert_eq!(patch.files[0].new_path, Some(PathBuf::from("b.txt")));
    }

    #[test]
    fn malformed_hunk_header_is_reported_not_panicked() {
        let text = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ garbage @@
 line
";
        let error = parse_patch(text).unwrap_err();
        assert!(matches!(error, PatchError::Malformed(_)));
    }

    #[test]
    fn hunk_claiming_fewer_lines_than_it_contains_is_malformed() {
        let text = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,1 +1,1 @@
 line1
 line2
";
        let error = parse_patch(text).unwrap_err();
        assert!(matches!(error, PatchError::Malformed(_)));
    }

    #[test]
    fn hunk_overrunning_the_new_side_count_is_malformed() {
        let text = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,1 +1,1 @@
+extra1
+extra2
";
        let error = parse_patch(text).unwrap_err();
        assert!(matches!(error, PatchError::Malformed(_)));
    }

    #[test]
    fn unrecognized_top_level_input_is_malformed() {
        let error = parse_patch("this is not a diff at all\n").unwrap_err();
        assert!(matches!(error, PatchError::Malformed(_)));
    }

    #[test]
    fn unrecognized_file_header_line_is_malformed() {
        let text = "\
diff --git a/f.txt b/f.txt
this is garbage
";
        let error = parse_patch(text).unwrap_err();
        assert!(matches!(error, PatchError::Malformed(_)));
    }
}
