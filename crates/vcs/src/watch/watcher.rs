use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use domain::{Aspect, RepositoryChange, RepositoryWatcher, WatchError};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
};

use super::classify::{Location, classify};
use super::git_dir;

/// Coalescing window. Short enough that a save in an editor still feels immediate, long
/// enough to absorb the burst of ref, index and lock-file writes a single `git` command
/// produces — a `checkout -b` alone touches `HEAD`, a lock file, and a new ref.
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(200);

/// An FSEvents-backed [`RepositoryWatcher`].
///
/// ```no_run
/// use std::path::Path;
/// use std::sync::mpsc::channel;
///
/// use domain::RepositoryWatcher;
/// use vcs::watch::FsRepositoryWatcher;
///
/// let (sender, receiver) = channel();
/// let guard = FsRepositoryWatcher::new().watch(Path::new("/path/to/repo"), sender)?;
/// let change = receiver.recv().expect("the watcher thread is still alive");
/// drop(guard);
/// # Ok::<(), domain::WatchError>(())
/// ```
#[derive(Default)]
pub struct FsRepositoryWatcher;

impl FsRepositoryWatcher {
    pub fn new() -> Self {
        Self
    }
}

impl RepositoryWatcher for FsRepositoryWatcher {
    type Guard = WatchGuard;

    fn watch(
        &self,
        repository: &Path,
        changes: Sender<RepositoryChange>,
    ) -> Result<Self::Guard, WatchError> {
        let git_dir = canonicalize(&git_dir::resolve(repository)?)?;
        let working_tree = canonicalize(repository)?;

        let (raw_tx, raw_rx) = std::sync::mpsc::channel::<DebounceEventResult>();
        let debouncer = new_debouncer(DEBOUNCE_WINDOW, None, raw_tx)
            .map_err(|source| rejected(repository, source))?;

        let shared = Arc::new(Shared {
            debouncer: Mutex::new(Some(debouncer)),
        });

        if let Err(error) = register_initial_watches(&shared, &working_tree, &git_dir) {
            shared.stop();
            return Err(error);
        }

        let pump_shared = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("gitr-repository-watcher".to_owned())
            .spawn(move || pump(raw_rx, pump_shared, working_tree, git_dir, changes));

        match spawned {
            Ok(_) => Ok(WatchGuard { shared }),
            Err(source) => {
                shared.stop();
                Err(rejected(repository, source))
            }
        }
    }
}

/// Stops watching when dropped.
///
/// Drop tears down the FSEvents subscription immediately. The watcher thread notices the
/// debounced event stream disconnect within one debounce tick and, in turn, drops the
/// `Sender<RepositoryChange>` the caller was given, closing it.
pub struct WatchGuard {
    shared: Arc<Shared>,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.shared.stop();
    }
}

struct Shared {
    debouncer: Mutex<Option<Debouncer<RecommendedWatcher, RecommendedCache>>>,
}

impl Shared {
    fn watch(&self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        let mut debouncer = self
            .debouncer
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match debouncer.as_mut() {
            Some(debouncer) => debouncer.watch(path, mode),
            None => Ok(()),
        }
    }

    fn stop(&self) {
        let mut debouncer = self
            .debouncer
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(debouncer) = debouncer.take() {
            debouncer.stop_nonblocking();
        }
    }
}

/// Resolves symlinks in `path`.
///
/// macOS FSEvents reports paths in their canonical form — notably, `/var` is a symlink to
/// `/private/var`, so a repository under `/var/folders/...` (as every `TempDir` is) would
/// otherwise never match the roots this watcher registers, and every event would be
/// silently dropped by `notify`'s own filtering before it ever reached this crate.
fn canonicalize(path: &Path) -> Result<PathBuf, WatchError> {
    path.canonicalize()
        .map_err(|_| WatchError::Unreadable(path.to_path_buf()))
}

fn rejected(path: &Path, source: impl std::error::Error + Send + Sync + 'static) -> WatchError {
    WatchError::Rejected {
        path: path.to_path_buf(),
        source: Box::new(source),
    }
}

/// Establishes the watch set for a fresh guard.
///
/// The working tree is watched one top-level entry at a time, recursively, skipping `.git`:
/// a single recursive watch rooted at `repository` would also subscribe to everything under
/// the Git directory, including `objects/`. The Git directory gets the same per-entry
/// treatment, skipping `objects` and `logs`. A non-recursive watch on each root additionally
/// catches entries that do not exist yet — `MERGE_HEAD`, `rebase-merge`, a freshly packed
/// `packed-refs` — so [`discover_new_top_level_child`] can promote them once they appear.
fn register_initial_watches(
    shared: &Shared,
    working_tree: &Path,
    git_dir: &Path,
) -> Result<(), WatchError> {
    shared
        .watch(working_tree, RecursiveMode::NonRecursive)
        .map_err(|source| rejected(working_tree, source))?;
    shared
        .watch(git_dir, RecursiveMode::NonRecursive)
        .map_err(|source| rejected(git_dir, source))?;

    for child in top_level_children(working_tree)
        .map_err(|_| WatchError::Unreadable(working_tree.to_path_buf()))?
    {
        if is_dot_git(&child) {
            continue;
        }
        shared
            .watch(&child, RecursiveMode::Recursive)
            .map_err(|source| rejected(&child, source))?;
    }

    for child in
        top_level_children(git_dir).map_err(|_| WatchError::Unreadable(git_dir.to_path_buf()))?
    {
        if is_excluded_git_dir_child(&child) {
            continue;
        }
        shared
            .watch(&child, RecursiveMode::Recursive)
            .map_err(|source| rejected(&child, source))?;
    }

    Ok(())
}

fn top_level_children(directory: &Path) -> std::io::Result<Vec<PathBuf>> {
    std::fs::read_dir(directory)?
        .map(|entry| entry.map(|dir_entry| dir_entry.path()))
        .collect()
}

fn is_dot_git(path: &Path) -> bool {
    path.file_name() == Some(OsStr::new(".git"))
}

fn is_excluded_git_dir_child(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(OsStr::to_str),
        Some("objects" | "logs")
    )
}

fn pump(
    events: Receiver<DebounceEventResult>,
    shared: Arc<Shared>,
    working_tree: PathBuf,
    git_dir: PathBuf,
    changes: Sender<RepositoryChange>,
) {
    let mut known_working_tree_children: HashSet<PathBuf> = top_level_children(&working_tree)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let mut known_git_dir_children: HashSet<PathBuf> = top_level_children(&git_dir)
        .unwrap_or_default()
        .into_iter()
        .collect();

    while let Ok(result) = events.recv() {
        let change = match result {
            Ok(batch) => {
                for event in &batch {
                    for path in &event.paths {
                        discover_new_top_level_child(
                            &shared,
                            path,
                            &working_tree,
                            &mut known_working_tree_children,
                            is_dot_git,
                        );
                        discover_new_top_level_child(
                            &shared,
                            path,
                            &git_dir,
                            &mut known_git_dir_children,
                            is_excluded_git_dir_child,
                        );
                    }
                }
                change_from_batch(&batch, &working_tree, &git_dir)
            }
            Err(_) => Some(RepositoryChange::everything()),
        };

        if let Some(change) = change
            && changes.send(change).is_err()
        {
            return;
        }
    }
}

/// Promotes a freshly observed direct child of `root` to its own recursive watch.
///
/// Silently drops a failure to watch: the most common cause is the path having disappeared
/// again between the event that revealed it and this call (a transient lock file is the
/// typical case), and `root`'s own non-recursive watch keeps reporting on that directory's
/// activity regardless, so a missed promotion here is not silence, only reduced precision.
fn discover_new_top_level_child(
    shared: &Shared,
    path: &Path,
    root: &Path,
    known: &mut HashSet<PathBuf>,
    excluded: impl Fn(&Path) -> bool,
) {
    if path.parent() != Some(root) {
        return;
    }
    if excluded(path) || known.contains(path) || !path.exists() {
        return;
    }

    known.insert(path.to_path_buf());
    let _ = shared.watch(path, RecursiveMode::Recursive);
}

fn change_from_batch(
    events: &[DebouncedEvent],
    working_tree: &Path,
    git_dir: &Path,
) -> Option<RepositoryChange> {
    let mut change: Option<RepositoryChange> = None;
    for event in events {
        if event.need_rescan() {
            return Some(RepositoryChange::everything());
        }
        for path in &event.paths {
            if let Some(aspect) = classify_path(path, working_tree, git_dir) {
                change = Some(match change {
                    Some(existing) => existing.with(aspect),
                    None => RepositoryChange::only(aspect),
                });
            }
        }
    }
    change
}

fn classify_path(path: &Path, working_tree: &Path, git_dir: &Path) -> Option<Aspect> {
    if let Ok(relative) = path.strip_prefix(git_dir) {
        classify(Location::GitDir(relative))
    } else if path.starts_with(working_tree) {
        classify(Location::WorkingTree)
    } else {
        None
    }
}
