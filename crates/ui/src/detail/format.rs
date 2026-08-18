//! Pure formatting logic for the commit detail panel.
//!
//! Nothing here touches gpui's `App` or `Window`: every function is a plain transformation
//! from domain types to strings, so it is testable without a running window.

use std::fmt::Write as _;
use std::ops::Range;
use std::path::PathBuf;

use domain::{FilePatch, FileStatus, Hunk, LineOrigin, ObjectId, Patch, Timestamp};

/// The UTF-8 byte range, into the text [`unified_diff_text_with_line_ranges`] produces,
/// of every added and every deleted line — everything else (file headers, hunk headers,
/// context lines) is left undecorated.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DiffLineRanges {
    pub additions: Vec<Range<usize>>,
    pub deletions: Vec<Range<usize>>,
}

/// Hexadecimal characters kept when an identifier is shown abbreviated, matching Git's
/// own default abbreviation length.
pub const ABBREV_LEN: usize = 7;

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub fn abbreviate(id: ObjectId) -> String {
    id.to_hex_prefix(ABBREV_LEN)
}

/// Days since the Unix epoch to a proleptic-Gregorian `(year, month, day)`.
///
/// Howard Hinnant's `civil_from_days` algorithm:
/// <https://howardhinnant.github.io/date_algorithms.html>.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

/// Renders `timestamp` the way `git log` does: in the author's own recorded UTC offset,
/// never converted to the viewer's local time.
pub fn format_timestamp(timestamp: Timestamp) -> String {
    let adjusted = timestamp.seconds + i64::from(timestamp.offset_minutes) * 60;
    let days = adjusted.div_euclid(86_400);
    let time_of_day = adjusted.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3_600;
    let minute = (time_of_day % 3_600) / 60;
    let second = time_of_day % 60;
    let month_name = MONTH_NAMES[(month - 1) as usize];
    format!("{day} {month_name} {year} at {hour:02}:{minute:02}:{second:02}")
}

/// Backslash-escapes every ASCII punctuation character, CommonMark's own escapable set.
///
/// A commit subject, identifier or author line is plain text, not Markdown source, but
/// the detail panel renders it through [`gpui_component::text::markdown`] to get
/// selectable text. Without this, a subject like `fix_bug` or `[WIP] release` would pick
/// up italics or link syntax it never asked for; escaping every ASCII punctuation
/// character, regardless of position, is what CommonMark guarantees always renders as a
/// literal character.
pub fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_punctuation() {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

/// A hunk's `@@ -old,len +new,len @@ heading` marker line.
pub fn hunk_heading(hunk: &Hunk) -> String {
    let marker = format!(
        "@@ -{},{} +{},{} @@",
        hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
    );
    if hunk.heading.is_empty() {
        marker
    } else {
        format!("{marker} {}", hunk.heading)
    }
}

fn git_path(path: Option<&PathBuf>) -> Option<String> {
    path.map(|path| path.display().to_string())
}

/// The path a `diff --git a/<path> b/<path>` line shows for one side.
///
/// Git always prints *some* path on both sides of that line, falling back to the other
/// side's path when this one doesn't exist (an added or deleted file) — the two sides
/// only ever truly differ for a rename or a copy.
fn git_header_path(path: Option<&PathBuf>, other: Option<&PathBuf>) -> String {
    git_path(path)
        .or_else(|| git_path(other))
        .unwrap_or_default()
}

/// The `a/<path>` / `b/<path>` / `/dev/null` form Git uses on a `---`, `+++` or `Binary
/// files` line, where — unlike [`git_header_path`] — a missing side stays `/dev/null`
/// rather than borrowing the other side's path.
fn side_path(path: Option<&PathBuf>, prefix: char) -> String {
    match git_path(path) {
        Some(path) => format!("{prefix}/{path}"),
        None => "/dev/null".to_string(),
    }
}

/// Rebuilds the unified diff text `git diff` would print for `patch` — so it can be fed
/// to a real editor for syntax highlighting, selection and virtualised scrolling — paired
/// with the byte range of every added and deleted line, computed in the same pass so the
/// text and the ranges can never disagree about where a line starts. Reconstructing the
/// text and then re-scanning it for `+`/`-` lines would be a second implementation of the
/// same rule, and a line whose own content begins with `+` or `-` is exactly where the
/// two could differ.
///
/// Mode lines (`new file mode`, `deleted file mode`) are Git prints that [`FilePatch`] has
/// no data for, and are left out rather than fabricated. Each range starts at the line's
/// leading marker byte, read from [`LineOrigin`] structurally rather than from the first
/// byte of `line.content` — a line about a diff can itself start with `+` or `-`.
pub fn unified_diff_text_with_line_ranges(patch: &Patch) -> (String, DiffLineRanges) {
    let mut text = String::new();
    let mut ranges = DiffLineRanges::default();
    for file in &patch.files {
        write_file(&mut text, file, &mut ranges);
    }
    (text, ranges)
}

fn write_file(text: &mut String, file: &FilePatch, ranges: &mut DiffLineRanges) {
    let old = file.old_path.as_ref();
    let new = file.new_path.as_ref();

    let _ = writeln!(
        text,
        "diff --git a/{} b/{}",
        git_header_path(old, new),
        git_header_path(new, old)
    );

    match &file.status {
        FileStatus::Renamed { similarity } => {
            let _ = writeln!(text, "similarity index {similarity}%");
            if let (Some(old), Some(new)) = (git_path(old), git_path(new)) {
                let _ = writeln!(text, "rename from {old}");
                let _ = writeln!(text, "rename to {new}");
            }
        }
        FileStatus::Copied { similarity } => {
            let _ = writeln!(text, "similarity index {similarity}%");
            if let (Some(old), Some(new)) = (git_path(old), git_path(new)) {
                let _ = writeln!(text, "copy from {old}");
                let _ = writeln!(text, "copy to {new}");
            }
        }
        FileStatus::Added
        | FileStatus::Deleted
        | FileStatus::Modified
        | FileStatus::TypeChanged => {}
    }

    if file.is_binary {
        let _ = writeln!(
            text,
            "Binary files {} and {} differ",
            side_path(old, 'a'),
            side_path(new, 'b')
        );
        return;
    }

    if file.hunks.is_empty() {
        return;
    }

    let _ = writeln!(text, "--- {}", side_path(old, 'a'));
    let _ = writeln!(text, "+++ {}", side_path(new, 'b'));
    for hunk in &file.hunks {
        let _ = writeln!(text, "{}", hunk_heading(hunk));
        for line in &hunk.lines {
            let marker = match line.origin {
                LineOrigin::Addition => '+',
                LineOrigin::Deletion => '-',
                LineOrigin::Context => ' ',
            };
            let start = text.len();
            let _ = writeln!(text, "{marker}{}", line.content);
            let end = text.len() - 1;
            match line.origin {
                LineOrigin::Addition => ranges.additions.push(start..end),
                LineOrigin::Deletion => ranges.deletions.push(start..end),
                LineOrigin::Context => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::DiffLine;

    fn unified_diff_text(patch: &Patch) -> String {
        unified_diff_text_with_line_ranges(patch).0
    }

    fn id(nibble: char) -> ObjectId {
        nibble.to_string().repeat(40).parse().unwrap()
    }

    fn file(
        old_path: Option<&str>,
        new_path: Option<&str>,
        status: FileStatus,
        is_binary: bool,
        hunks: Vec<Hunk>,
    ) -> FilePatch {
        FilePatch {
            old_path: old_path.map(PathBuf::from),
            new_path: new_path.map(PathBuf::from),
            status,
            is_binary,
            hunks,
        }
    }

    fn hunk(lines: Vec<DiffLine>) -> Hunk {
        Hunk {
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 2,
            heading: "fn existing()".to_string(),
            lines,
        }
    }

    fn line(origin: LineOrigin, content: &str) -> DiffLine {
        DiffLine {
            origin,
            old_number: None,
            new_number: None,
            content: content.to_string(),
        }
    }

    #[test]
    fn abbreviate_truncates_to_seven_hex_characters() {
        assert_eq!(abbreviate(id('a')), "aaaaaaa");
    }

    #[test]
    fn the_epoch_renders_at_a_zero_offset() {
        let timestamp = Timestamp {
            seconds: 0,
            offset_minutes: 0,
        };
        assert_eq!(format_timestamp(timestamp), "1 January 1970 at 00:00:00");
    }

    #[test]
    fn a_known_anchor_renders_correctly_at_a_zero_offset() {
        let timestamp = Timestamp {
            seconds: 946_684_800,
            offset_minutes: 0,
        };
        assert_eq!(format_timestamp(timestamp), "1 January 2000 at 00:00:00");
    }

    #[test]
    fn a_positive_offset_advances_the_wall_clock_without_touching_the_instant() {
        let timestamp = Timestamp {
            seconds: 946_684_800,
            offset_minutes: 180,
        };
        assert_eq!(format_timestamp(timestamp), "1 January 2000 at 03:00:00");
    }

    #[test]
    fn a_negative_offset_can_move_the_wall_clock_into_the_previous_year() {
        let timestamp = Timestamp {
            seconds: 946_684_800,
            offset_minutes: -300,
        };
        assert_eq!(format_timestamp(timestamp), "31 December 1999 at 19:00:00");
    }

    #[test]
    fn hunk_heading_includes_the_function_context_when_git_found_one() {
        assert_eq!(hunk_heading(&hunk(vec![])), "@@ -1,1 +1,2 @@ fn existing()");
    }

    #[test]
    fn hunk_heading_omits_the_trailing_space_when_git_found_no_context() {
        let mut hunk = hunk(vec![]);
        hunk.heading = String::new();
        assert_eq!(hunk_heading(&hunk), "@@ -1,1 +1,2 @@");
    }

    #[test]
    fn escape_markdown_neutralises_ascii_punctuation_without_touching_letters_digits_or_spaces() {
        assert_eq!(
            escape_markdown("fix_bug 100% [WIP] (v1)"),
            "fix\\_bug 100\\% \\[WIP\\] \\(v1\\)"
        );
    }

    #[test]
    fn escape_markdown_escapes_a_literal_backslash_first() {
        assert_eq!(escape_markdown("a\\b"), "a\\\\b");
    }

    #[test]
    fn escape_markdown_leaves_non_ascii_text_untouched() {
        assert_eq!(
            escape_markdown("caf\u{e9} \u{2014} r\u{e9}sum\u{e9}"),
            "caf\u{e9} \u{2014} r\u{e9}sum\u{e9}"
        );
    }

    #[test]
    fn unified_diff_text_of_an_empty_patch_is_empty() {
        let patch = Patch { files: vec![] };
        assert_eq!(unified_diff_text(&patch), "");
    }

    #[test]
    fn unified_diff_text_reconstructs_a_single_hunk() {
        let patch = Patch {
            files: vec![file(
                Some("src/lib.rs"),
                Some("src/lib.rs"),
                FileStatus::Modified,
                false,
                vec![hunk(vec![
                    line(LineOrigin::Context, "fn existing() {"),
                    line(LineOrigin::Deletion, "    old();"),
                    line(LineOrigin::Addition, "    new();"),
                    line(LineOrigin::Addition, "    more();"),
                ])],
            )],
        };
        assert_eq!(
            unified_diff_text(&patch),
            "diff --git a/src/lib.rs b/src/lib.rs\n".to_string()
                + "--- a/src/lib.rs\n"
                + "+++ b/src/lib.rs\n"
                + "@@ -1,1 +1,2 @@ fn existing()\n"
                + " fn existing() {\n"
                + "-    old();\n"
                + "+    new();\n"
                + "+    more();\n"
        );
    }

    #[test]
    fn unified_diff_text_uses_dev_null_for_an_added_file() {
        let patch = Patch {
            files: vec![file(
                None,
                Some("new.rs"),
                FileStatus::Added,
                false,
                vec![hunk(vec![line(LineOrigin::Addition, "fn new() {}")])],
            )],
        };
        let text = unified_diff_text(&patch);
        assert!(text.starts_with("diff --git a/new.rs b/new.rs\n"));
        assert!(text.contains("--- /dev/null\n"));
        assert!(text.contains("+++ b/new.rs\n"));
    }

    #[test]
    fn unified_diff_text_uses_dev_null_for_a_deleted_file() {
        let patch = Patch {
            files: vec![file(
                Some("old.rs"),
                None,
                FileStatus::Deleted,
                false,
                vec![hunk(vec![line(LineOrigin::Deletion, "fn old() {}")])],
            )],
        };
        let text = unified_diff_text(&patch);
        assert!(text.starts_with("diff --git a/old.rs b/old.rs\n"));
        assert!(text.contains("--- a/old.rs\n"));
        assert!(text.contains("+++ /dev/null\n"));
    }

    #[test]
    fn unified_diff_text_marks_binary_files_without_a_hunk() {
        let patch = Patch {
            files: vec![file(
                Some("image.png"),
                Some("image.png"),
                FileStatus::Modified,
                true,
                vec![],
            )],
        };
        assert_eq!(
            unified_diff_text(&patch),
            "diff --git a/image.png b/image.png\nBinary files a/image.png and b/image.png differ\n"
        );
    }

    #[test]
    fn unified_diff_text_includes_rename_headers_and_omits_hunks_for_a_pure_rename() {
        let patch = Patch {
            files: vec![file(
                Some("old.rs"),
                Some("new.rs"),
                FileStatus::Renamed { similarity: 87 },
                false,
                vec![],
            )],
        };
        assert_eq!(
            unified_diff_text(&patch),
            "diff --git a/old.rs b/new.rs\n\
             similarity index 87%\n\
             rename from old.rs\n\
             rename to new.rs\n"
        );
    }

    #[test]
    fn unified_diff_text_includes_copy_headers() {
        let patch = Patch {
            files: vec![file(
                Some("old.rs"),
                Some("copy.rs"),
                FileStatus::Copied { similarity: 100 },
                false,
                vec![],
            )],
        };
        let text = unified_diff_text(&patch);
        assert!(text.contains("copy from old.rs\n"));
        assert!(text.contains("copy to copy.rs\n"));
    }

    #[test]
    fn unified_diff_text_concatenates_multiple_files_in_order() {
        let patch = Patch {
            files: vec![
                file(
                    Some("a.rs"),
                    Some("a.rs"),
                    FileStatus::Modified,
                    false,
                    vec![hunk(vec![line(LineOrigin::Addition, "a")])],
                ),
                file(
                    Some("b.rs"),
                    Some("b.rs"),
                    FileStatus::Modified,
                    false,
                    vec![hunk(vec![line(LineOrigin::Addition, "b")])],
                ),
            ],
        };
        let text = unified_diff_text(&patch);
        let a_pos = text.find("diff --git a/a.rs").unwrap();
        let b_pos = text.find("diff --git a/b.rs").unwrap();
        assert!(a_pos < b_pos);
    }

    #[test]
    fn line_ranges_cover_additions_and_deletions_and_nothing_else() {
        let patch = Patch {
            files: vec![file(
                Some("src/lib.rs"),
                Some("src/lib.rs"),
                FileStatus::Modified,
                false,
                vec![hunk(vec![
                    line(LineOrigin::Context, "fn existing() {"),
                    line(LineOrigin::Deletion, "    old();"),
                    line(LineOrigin::Addition, "    new();"),
                ])],
            )],
        };
        let (text, ranges) = unified_diff_text_with_line_ranges(&patch);

        assert_eq!(ranges.additions.len(), 1);
        assert_eq!(ranges.deletions.len(), 1);

        let addition = &text[ranges.additions[0].clone()];
        assert_eq!(addition, "+    new();");

        let deletion = &text[ranges.deletions[0].clone()];
        assert_eq!(deletion, "-    old();");

        let file_header_pos = text.find("diff --git").unwrap();
        let hunk_header_pos = text.find("@@").unwrap();
        let context_pos = text.find(" fn existing() {").unwrap();
        for range in ranges.additions.iter().chain(&ranges.deletions) {
            assert!(!range.contains(&file_header_pos));
            assert!(!range.contains(&hunk_header_pos));
            assert!(!range.contains(&context_pos));
        }
    }

    #[test]
    fn line_ranges_are_read_from_the_marker_column_not_the_lines_own_content() {
        let patch = Patch {
            files: vec![file(
                Some("src/lib.rs"),
                Some("src/lib.rs"),
                FileStatus::Modified,
                false,
                vec![hunk(vec![
                    line(LineOrigin::Context, "+not actually an addition"),
                    line(LineOrigin::Addition, "-not actually a deletion"),
                    line(LineOrigin::Deletion, "+not actually an addition either"),
                ])],
            )],
        };
        let (text, ranges) = unified_diff_text_with_line_ranges(&patch);

        assert_eq!(ranges.additions.len(), 1);
        assert_eq!(ranges.deletions.len(), 1);

        let addition = &text[ranges.additions[0].clone()];
        assert!(addition.starts_with('+'));
        assert_eq!(addition, "+-not actually a deletion");

        let deletion = &text[ranges.deletions[0].clone()];
        assert!(deletion.starts_with('-'));
        assert_eq!(deletion, "-+not actually an addition either");
    }
}
