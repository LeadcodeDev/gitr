//! Pure formatting and classification logic for the commit detail panel.
//!
//! Nothing here touches gpui's `App` or `Window`: every function is a plain transformation
//! from domain types to strings or theme colours, so it is testable without a running
//! window. `ThemeColor::light()` and `ThemeColor::dark()` work the same way, which is what
//! makes `classify_line` testable against both palettes below.

use std::path::PathBuf;

use domain::{FilePatch, FileStatus, Hunk, LineOrigin, ObjectId, Timestamp};
use gpui::Hsla;
use gpui_component::ThemeColor;

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

/// A file's `+added −deleted` summary.
pub fn diff_stat(file: &FilePatch) -> String {
    format!("+{} \u{2212}{}", file.added_lines(), file.deleted_lines())
}

fn path_or_empty(path: Option<&PathBuf>) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_default()
}

/// The label half of a file's header: everything but the `+n −m` summary, which
/// [`diff_stat`] renders separately so a view can lay the two out independently.
pub fn file_header(file: &FilePatch) -> String {
    match &file.status {
        FileStatus::Added => format!("{} (new file)", path_or_empty(file.display_path())),
        FileStatus::Deleted => format!("{} (deleted)", path_or_empty(file.display_path())),
        FileStatus::TypeChanged => {
            format!("{} (type changed)", path_or_empty(file.display_path()))
        }
        FileStatus::Modified => path_or_empty(file.display_path()),
        FileStatus::Renamed { similarity } => format!(
            "{} \u{2192} {} ({similarity}% similar)",
            path_or_empty(file.old_path.as_ref()),
            path_or_empty(file.new_path.as_ref())
        ),
        FileStatus::Copied { similarity } => format!(
            "{} \u{2192} {} (copied, {similarity}% similar)",
            path_or_empty(file.old_path.as_ref()),
            path_or_empty(file.new_path.as_ref())
        ),
    }
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

/// What a file's body should show.
///
/// `hunks` is empty both for a binary file and for a pure rename, and [`FilePatch::is_binary`]
/// is the only thing that tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBody<'a> {
    Binary,
    NoTextChange,
    Hunks(&'a [Hunk]),
}

pub fn file_body(file: &FilePatch) -> FileBody<'_> {
    if file.is_binary {
        FileBody::Binary
    } else if file.hunks.is_empty() {
        FileBody::NoTextChange
    } else {
        FileBody::Hunks(&file.hunks)
    }
}

/// The colours one [`DiffLine`] renders with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineColors {
    pub foreground: Hsla,
    pub background: Option<Hsla>,
}

pub fn classify_line(origin: LineOrigin, theme: &ThemeColor) -> LineColors {
    match origin {
        LineOrigin::Addition => LineColors {
            foreground: theme.green,
            background: Some(theme.green_light),
        },
        LineOrigin::Deletion => LineColors {
            foreground: theme.red,
            background: Some(theme.red_light),
        },
        LineOrigin::Context => LineColors {
            foreground: theme.foreground,
            background: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::DiffLine;

    fn id(nibble: char) -> ObjectId {
        nibble.to_string().repeat(40).parse().unwrap()
    }

    fn file(status: FileStatus, is_binary: bool, hunks: Vec<Hunk>) -> FilePatch {
        FilePatch {
            old_path: Some(PathBuf::from("old.rs")),
            new_path: Some(PathBuf::from("new.rs")),
            status,
            is_binary,
            hunks,
        }
    }

    fn hunk(lines: Vec<DiffLine>) -> Hunk {
        Hunk {
            old_start: 1,
            old_lines: 7,
            new_start: 1,
            new_lines: 9,
            heading: "fn existing()".to_string(),
            lines,
        }
    }

    fn line(origin: LineOrigin) -> DiffLine {
        DiffLine {
            origin,
            old_number: None,
            new_number: None,
            content: String::new(),
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
    fn diff_stat_reports_additions_and_deletions_with_a_minus_sign() {
        let patch = file(
            FileStatus::Modified,
            false,
            vec![hunk(vec![
                DiffLine {
                    new_number: Some(1),
                    ..line(LineOrigin::Addition)
                },
                DiffLine {
                    new_number: Some(2),
                    ..line(LineOrigin::Addition)
                },
                DiffLine {
                    old_number: Some(1),
                    ..line(LineOrigin::Deletion)
                },
            ])],
        );
        assert_eq!(diff_stat(&patch), "+2 \u{2212}1");
    }

    #[test]
    fn diff_stat_of_an_untouched_file_is_zero_and_zero() {
        let patch = file(FileStatus::Modified, false, vec![]);
        assert_eq!(diff_stat(&patch), "+0 \u{2212}0");
    }

    #[test]
    fn file_header_reports_a_rename_as_old_arrow_new_with_similarity() {
        let patch = file(FileStatus::Renamed { similarity: 87 }, false, vec![]);
        assert_eq!(file_header(&patch), "old.rs \u{2192} new.rs (87% similar)");
    }

    #[test]
    fn file_header_reports_a_copy_as_old_arrow_new_with_similarity() {
        let patch = file(FileStatus::Copied { similarity: 60 }, false, vec![]);
        assert_eq!(
            file_header(&patch),
            "old.rs \u{2192} new.rs (copied, 60% similar)"
        );
    }

    #[test]
    fn file_header_marks_an_added_file() {
        let mut patch = file(FileStatus::Added, false, vec![]);
        patch.old_path = None;
        assert_eq!(file_header(&patch), "new.rs (new file)");
    }

    #[test]
    fn file_header_marks_a_deleted_file() {
        let mut patch = file(FileStatus::Deleted, false, vec![]);
        patch.new_path = None;
        assert_eq!(file_header(&patch), "old.rs (deleted)");
    }

    #[test]
    fn file_header_marks_a_type_change() {
        let patch = file(FileStatus::TypeChanged, false, vec![]);
        assert_eq!(file_header(&patch), "new.rs (type changed)");
    }

    #[test]
    fn file_header_of_a_plain_modification_is_just_the_path() {
        let patch = file(FileStatus::Modified, false, vec![]);
        assert_eq!(file_header(&patch), "new.rs");
    }

    #[test]
    fn hunk_heading_includes_the_function_context_when_git_found_one() {
        assert_eq!(hunk_heading(&hunk(vec![])), "@@ -1,7 +1,9 @@ fn existing()");
    }

    #[test]
    fn hunk_heading_omits_the_trailing_space_when_git_found_no_context() {
        let mut hunk = hunk(vec![]);
        hunk.heading = String::new();
        assert_eq!(hunk_heading(&hunk), "@@ -1,7 +1,9 @@");
    }

    #[test]
    fn a_binary_file_and_a_pure_rename_both_have_empty_hunks_but_render_differently() {
        let binary = file(FileStatus::Modified, true, vec![]);
        let pure_rename = file(FileStatus::Renamed { similarity: 100 }, false, vec![]);

        assert_eq!(file_body(&binary), FileBody::Binary);
        assert_eq!(file_body(&pure_rename), FileBody::NoTextChange);
        assert_ne!(file_body(&binary), file_body(&pure_rename));
    }

    #[test]
    fn a_file_with_hunks_exposes_them_for_rendering() {
        let lines = vec![DiffLine {
            content: "line".into(),
            ..line(LineOrigin::Context)
        }];
        let patch = file(FileStatus::Modified, false, vec![hunk(lines.clone())]);
        assert_eq!(file_body(&patch), FileBody::Hunks(&[hunk(lines)]));
    }

    #[test]
    fn an_empty_patch_has_no_files() {
        let patch = domain::Patch { files: vec![] };
        assert!(patch.files.is_empty());
    }

    #[test]
    fn a_mode_only_change_has_no_hunks_and_no_line_totals() {
        let patch = file(FileStatus::Modified, false, vec![]);
        assert_eq!(file_body(&patch), FileBody::NoTextChange);
        assert_eq!(diff_stat(&patch), "+0 \u{2212}0");
    }

    #[test]
    fn classify_line_uses_green_for_additions_in_both_palettes() {
        for theme in [ThemeColor::light(), ThemeColor::dark()] {
            let colors = classify_line(LineOrigin::Addition, &theme);
            assert_eq!(colors.foreground, theme.green);
            assert_eq!(colors.background, Some(theme.green_light));
        }
    }

    #[test]
    fn classify_line_uses_red_for_deletions_in_both_palettes() {
        for theme in [ThemeColor::light(), ThemeColor::dark()] {
            let colors = classify_line(LineOrigin::Deletion, &theme);
            assert_eq!(colors.foreground, theme.red);
            assert_eq!(colors.background, Some(theme.red_light));
        }
    }

    #[test]
    fn classify_line_leaves_context_lines_uncoloured_in_both_palettes() {
        for theme in [ThemeColor::light(), ThemeColor::dark()] {
            let colors = classify_line(LineOrigin::Context, &theme);
            assert_eq!(colors.foreground, theme.foreground);
            assert_eq!(colors.background, None);
        }
    }

    #[test]
    fn addition_and_deletion_colours_are_distinct_in_both_palettes() {
        for theme in [ThemeColor::light(), ThemeColor::dark()] {
            let addition = classify_line(LineOrigin::Addition, &theme);
            let deletion = classify_line(LineOrigin::Deletion, &theme);
            assert_ne!(addition.foreground, deletion.foreground);
        }
    }
}
