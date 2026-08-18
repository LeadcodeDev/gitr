//! Turns `git clone --progress`'s stderr into the phase and percentage
//! [`super::GitRunner::clone_bare_with_progress`] reports as a clone runs.
//!
//! `--progress` writes a line, then overwrites it in place with a carriage return rather
//! than ending it with a newline — the same trick a terminal spinner uses. A reader that
//! only splits on `\n` sees nothing until the very last line, which lands right as the
//! process exits: it would compile, pass, and never show a caller anything incremental.
//! [`ProgressLineSplitter`] treats `\r` as an equally valid line terminator so every
//! intermediate percentage is delivered as it lands.
//!
//! Git's progress wording is not a versioned API and has changed between releases.
//! [`parse_progress_line`] is total — every input either yields a [`CloneProgress`] or is
//! silently skipped — so an unfamiliar line can never turn into a parse error that fails
//! a clone over cosmetic output.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClonePhase {
    EnumeratingObjects,
    CountingObjects,
    CompressingObjects,
    ReceivingObjects,
    ResolvingDeltas,
}

impl ClonePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::EnumeratingObjects => "Enumerating objects",
            Self::CountingObjects => "Counting objects",
            Self::CompressingObjects => "Compressing objects",
            Self::ReceivingObjects => "Receiving objects",
            Self::ResolvingDeltas => "Resolving deltas",
        }
    }
}

const PHASES: [(&str, ClonePhase); 5] = [
    ("Enumerating objects", ClonePhase::EnumeratingObjects),
    ("Counting objects", ClonePhase::CountingObjects),
    ("Compressing objects", ClonePhase::CompressingObjects),
    ("Receiving objects", ClonePhase::ReceivingObjects),
    ("Resolving deltas", ClonePhase::ResolvingDeltas),
];

/// One point along a clone's progress, read off a single line of `git`'s `--progress`
/// stderr.
///
/// `percent` is `None` for the lines git emits before it has a total to divide by, such
/// as `remote: Enumerating objects: 9, done.` — that line names a count, never a
/// percentage, and no later line for the same phase corrects it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloneProgress {
    pub phase: ClonePhase,
    pub percent: Option<u8>,
}

/// Reads one already-complete line — no trailing `\r` or `\n` — and returns the phase and
/// percentage it reports, or `None` if the line does not match any phase this recognises.
///
/// The server-side phases (`remote: Enumerating/Counting/Compressing objects`) carry a
/// `remote: ` prefix that the client-side ones (`Receiving objects`, `Resolving deltas`)
/// do not; both are tried through the same table after stripping it.
pub(crate) fn parse_progress_line(line: &str) -> Option<CloneProgress> {
    let unprefixed = line.strip_prefix("remote: ").unwrap_or(line);
    let (phase, rest) = PHASES
        .iter()
        .find_map(|&(label, phase)| unprefixed.strip_prefix(label).map(|rest| (phase, rest)))?;
    let rest = rest.strip_prefix(':')?;
    Some(CloneProgress {
        phase,
        percent: parse_percent(rest),
    })
}

fn parse_percent(rest: &str) -> Option<u8> {
    let rest = rest.trim_start();
    let digits_end = rest.find(|character: char| !character.is_ascii_digit())?;
    if digits_end == 0 || rest.as_bytes().get(digits_end) != Some(&b'%') {
        return None;
    }
    rest[..digits_end].parse().ok()
}

/// Reassembles complete lines out of raw byte chunks read from a pipe.
///
/// A chunk boundary lands wherever the OS delivers it, not wherever a progress line
/// happens to end — a percentage can arrive split across two reads. Bytes after the last
/// `\r` or `\n` a call to [`Self::feed`] sees are incomplete and stay buffered for the
/// next one, so a line is only ever handed out whole.
#[derive(Default)]
pub(crate) struct ProgressLineSplitter {
    buffer: Vec<u8>,
}

impl ProgressLineSplitter {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Feeds one chunk in and returns every line it completed, in the order they closed.
    pub(crate) fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut lines = Vec::new();
        let mut start = 0;
        for (index, &byte) in self.buffer.iter().enumerate() {
            if byte == b'\r' || byte == b'\n' {
                lines.push(String::from_utf8_lossy(&self.buffer[start..index]).into_owned());
                start = index + 1;
            }
        }
        self.buffer.drain(..start);
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_carriage_return_separated_burst_in_one_chunk_yields_every_line() {
        let mut splitter = ProgressLineSplitter::new();
        let chunk = b"Receiving objects:  11% (1/9)\rReceiving objects:  22% (2/9)\rReceiving objects:  33% (3/9)\r";

        let lines = splitter.feed(chunk);

        assert_eq!(
            lines,
            vec![
                "Receiving objects:  11% (1/9)",
                "Receiving objects:  22% (2/9)",
                "Receiving objects:  33% (3/9)",
            ]
        );
        let progress: Vec<CloneProgress> = lines
            .iter()
            .filter_map(|l| parse_progress_line(l))
            .collect();
        assert_eq!(
            progress,
            vec![
                CloneProgress {
                    phase: ClonePhase::ReceivingObjects,
                    percent: Some(11)
                },
                CloneProgress {
                    phase: ClonePhase::ReceivingObjects,
                    percent: Some(22)
                },
                CloneProgress {
                    phase: ClonePhase::ReceivingObjects,
                    percent: Some(33)
                },
            ]
        );
    }

    #[test]
    fn a_phase_transition_is_visible_across_two_lines() {
        let enumerating =
            parse_progress_line("remote: Enumerating objects: 17047, done.        ").unwrap();
        let counting =
            parse_progress_line("remote: Counting objects:   1% (1/59)        ").unwrap();

        assert_eq!(enumerating.phase, ClonePhase::EnumeratingObjects);
        assert_eq!(counting.phase, ClonePhase::CountingObjects);
        assert_ne!(enumerating.phase, counting.phase);
    }

    #[test]
    fn a_trailing_done_suffix_still_yields_the_final_percentage() {
        let progress = parse_progress_line("Resolving deltas: 100% (9527/9527), done.").unwrap();

        assert_eq!(progress.phase, ClonePhase::ResolvingDeltas);
        assert_eq!(progress.percent, Some(100));
    }

    #[test]
    fn a_remote_prefixed_phase_is_recognised() {
        let progress = parse_progress_line("remote: Compressing objects:  50% (12/24)").unwrap();

        assert_eq!(progress.phase, ClonePhase::CompressingObjects);
        assert_eq!(progress.percent, Some(50));
    }

    #[test]
    fn a_line_with_no_percentage_yields_the_phase_with_percent_none() {
        let progress =
            parse_progress_line("remote: Enumerating objects: 9, done.        ").unwrap();

        assert_eq!(progress.phase, ClonePhase::EnumeratingObjects);
        assert_eq!(progress.percent, None);
    }

    #[test]
    fn an_unrecognised_line_is_skipped_rather_than_misparsed() {
        assert_eq!(
            parse_progress_line("Cloning into bare repository 'dest.git'..."),
            None
        );
        assert_eq!(
            parse_progress_line(
                "remote: Total 9 (delta 0), reused 0 (delta 0), pack-reused 9 (from 1)        "
            ),
            None
        );
        assert_eq!(parse_progress_line(""), None);
    }

    #[test]
    fn a_percentage_split_across_two_reads_is_reassembled_before_parsing() {
        let mut splitter = ProgressLineSplitter::new();

        let first = splitter.feed(b"Receiving objects:  4");
        assert!(first.is_empty());

        let second = splitter.feed(b"5% (10/20)\rReceiving objects:  50% (11/20)\r");
        assert_eq!(
            second,
            vec![
                "Receiving objects:  45% (10/20)",
                "Receiving objects:  50% (11/20)"
            ]
        );

        let progress = parse_progress_line(&second[0]).unwrap();
        assert_eq!(progress.phase, ClonePhase::ReceivingObjects);
        assert_eq!(progress.percent, Some(45));
    }

    #[test]
    fn a_chunk_split_exactly_on_the_percent_sign_still_parses() {
        let mut splitter = ProgressLineSplitter::new();

        assert!(splitter.feed(b"Receiving objects: 100").is_empty());
        let lines = splitter.feed(b"% (20/20), done.\n");

        assert_eq!(lines, vec!["Receiving objects: 100% (20/20), done."]);
        assert_eq!(parse_progress_line(&lines[0]).unwrap().percent, Some(100));
    }

    #[test]
    fn captured_real_bytes_from_a_network_clone_parse_into_the_expected_phase_sequence() {
        let captured: &[u8] = b"Cloning into bare repository 'progress-test2'...\nremote: Enumerating objects: 17047, done.        \nremote: Counting objects:   1% (1/59)        \rremote: Counting objects: 100% (59/59)        \rremote: Counting objects: 100% (59/59), done.        \nremote: Compressing objects:   2% (1/45)        \rremote: Compressing objects: 100% (45/45)        \rremote: Compressing objects: 100% (45/45), done.        \nReceiving objects:   0% (1/17047)\rReceiving objects:  50% (8524/17047), 691.98 KiB | 1.32 MiB/s\rReceiving objects: 100% (17047/17047), 1.38 MiB | 1.35 MiB/s\rReceiving objects: 100% (17047/17047), 2.10 MiB | 1.20 MiB/s, done.\nResolving deltas:   0% (0/9527)\rResolving deltas:  50% (4764/9527)\rResolving deltas: 100% (9527/9527)\rResolving deltas: 100% (9527/9527), done.\n";

        let mut splitter = ProgressLineSplitter::new();
        let phases: Vec<CloneProgress> = splitter
            .feed(captured)
            .iter()
            .filter_map(|line| parse_progress_line(line))
            .collect();

        let sequence: Vec<ClonePhase> =
            phases
                .iter()
                .map(|p| p.phase)
                .fold(Vec::new(), |mut acc, phase| {
                    if acc.last() != Some(&phase) {
                        acc.push(phase);
                    }
                    acc
                });

        assert_eq!(
            sequence,
            vec![
                ClonePhase::EnumeratingObjects,
                ClonePhase::CountingObjects,
                ClonePhase::CompressingObjects,
                ClonePhase::ReceivingObjects,
                ClonePhase::ResolvingDeltas,
            ]
        );
        assert_eq!(phases.last().unwrap().percent, Some(100));
    }
}
