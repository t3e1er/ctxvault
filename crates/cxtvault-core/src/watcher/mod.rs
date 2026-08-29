//! File system watcher: debounced change detection for markdown files.
//!
//! Uses [`notify`] to watch a corpus directory recursively and emits
//! classified [`FileEvent`]s through a tokio mpsc channel.

use std::path::{Path, PathBuf};

use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use cxtvault_common::{Error, Result};

// ─── Public Types ────────────────────────────────────────────────────────────

/// A file system event classified for the indexer.
#[derive(Debug, Clone)]
pub enum FileEvent {
    /// A new `.md` file was created.
    Created(PathBuf),
    /// An existing `.md` file was modified.
    Modified(PathBuf),
    /// An `.md` file was deleted.
    Deleted(PathBuf),
    /// An `.md` file was renamed (from, to).
    Renamed {
        /// Original path before the rename.
        from: PathBuf,
        /// New path after the rename.
        to: PathBuf,
    },
}

/// Watches a directory for markdown file changes.
///
/// Events are delivered through an internal tokio mpsc channel. The watcher
/// runs in the background on an OS thread managed by `notify`; dropping the
/// struct stops the watcher.
pub struct CorpusWatcher {
    /// Channel receiver for file events.
    receiver: mpsc::Receiver<FileEvent>,
    /// Handle to the watcher (kept alive to prevent drop).
    _watcher: RecommendedWatcher,
}

// ─── Implementation ──────────────────────────────────────────────────────────

impl CorpusWatcher {
    /// Start watching a directory for `.md` file changes.
    ///
    /// Returns a `CorpusWatcher` whose [`recv`](Self::recv) and
    /// [`try_recv`](Self::try_recv) methods yield classified events.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying OS watcher cannot be created or the
    /// path cannot be watched.
    pub fn start(watch_path: &Path) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<FileEvent>(256);

        let mut watcher =
            notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if let Some(file_event) = classify_event(&event) {
                        // Best-effort send; if the receiver is gone we silently drop.
                        let _ = tx.blocking_send(file_event);
                    }
                }
            })
            .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

        watcher
            .watch(watch_path, RecursiveMode::Recursive)
            .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

        Ok(Self { receiver: rx, _watcher: watcher })
    }

    /// Receive the next file event (async). Returns `None` if the watcher stopped.
    pub async fn recv(&mut self) -> Option<FileEvent> {
        self.receiver.recv().await
    }

    /// Non-blocking attempt to receive an event.
    ///
    /// Returns `None` if no event is currently available or the channel closed.
    pub fn try_recv(&mut self) -> Option<FileEvent> {
        self.receiver.try_recv().ok()
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Returns `true` if the path has a `.md` extension.
fn is_markdown(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("md")
}

/// Classify a raw notify [`Event`] into an optional [`FileEvent`].
///
/// Only events affecting `.md` files produce a result.
fn classify_event(event: &Event) -> Option<FileEvent> {
    let md_paths: Vec<&PathBuf> = event.paths.iter().filter(|p| is_markdown(p)).collect();

    if md_paths.is_empty() {
        return None;
    }

    match event.kind {
        EventKind::Create(_) => Some(FileEvent::Created(md_paths[0].clone())),

        EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Any) => {
            Some(FileEvent::Modified(md_paths[0].clone()))
        }

        EventKind::Remove(_) => Some(FileEvent::Deleted(md_paths[0].clone())),

        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if md_paths.len() >= 2 => {
            Some(FileEvent::Renamed { from: md_paths[0].clone(), to: md_paths[1].clone() })
        }

        _ => None,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, RemoveKind};

    /// Helper to build a notify Event with given kind and paths.
    fn make_event(kind: EventKind, paths: Vec<PathBuf>) -> Event {
        Event { kind, paths, attrs: Default::default() }
    }

    #[test]
    fn test_classify_create_md() {
        let event =
            make_event(EventKind::Create(CreateKind::File), vec![PathBuf::from("/docs/note.md")]);
        let result = classify_event(&event);
        assert!(
            matches!(result, Some(FileEvent::Created(p)) if p == PathBuf::from("/docs/note.md"))
        );
    }

    #[test]
    fn test_classify_modify_md() {
        let event = make_event(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            vec![PathBuf::from("/docs/note.md")],
        );
        let result = classify_event(&event);
        assert!(
            matches!(result, Some(FileEvent::Modified(p)) if p == PathBuf::from("/docs/note.md"))
        );
    }

    #[test]
    fn test_classify_delete_md() {
        let event =
            make_event(EventKind::Remove(RemoveKind::File), vec![PathBuf::from("/docs/note.md")]);
        let result = classify_event(&event);
        assert!(
            matches!(result, Some(FileEvent::Deleted(p)) if p == PathBuf::from("/docs/note.md"))
        );
    }

    #[test]
    fn test_classify_rename_md() {
        let event = make_event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            vec![PathBuf::from("/docs/old.md"), PathBuf::from("/docs/new.md")],
        );
        let result = classify_event(&event);
        assert!(matches!(
            result,
            Some(FileEvent::Renamed { from, to })
                if from == PathBuf::from("/docs/old.md") && to == PathBuf::from("/docs/new.md")
        ));
    }

    #[test]
    fn test_classify_ignores_non_md() {
        let event =
            make_event(EventKind::Create(CreateKind::File), vec![PathBuf::from("/docs/image.png")]);
        assert!(classify_event(&event).is_none());
    }

    #[test]
    fn test_classify_modify_any() {
        let event =
            make_event(EventKind::Modify(ModifyKind::Any), vec![PathBuf::from("/docs/note.md")]);
        let result = classify_event(&event);
        assert!(matches!(result, Some(FileEvent::Modified(_))));
    }

    #[test]
    fn test_watcher_start_stop() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let watcher = CorpusWatcher::start(dir.path());
        assert!(watcher.is_ok(), "watcher should start without error");
        // Dropping the watcher stops it cleanly.
        drop(watcher);
    }
}
