use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc::RecvTimeoutError, Arc, Mutex, RwLock};
use std::time::Duration;

use crate::server::is_hidden;
use crate::utils::get_file_name;

/// In-memory filesystem index used to serve `?q=` searches without walking the
/// disk on every request. It is built once (in parallel) on startup and kept
/// up to date by a background filesystem watcher.
pub struct SearchIndex {
    entries: RwLock<Vec<IndexEntry>>,
    root: PathBuf,
    hidden: Arc<Vec<String>>,
    allow_symlink: bool,
}

struct IndexEntry {
    path: PathBuf,
    name_lower: String,
}

impl SearchIndex {
    pub fn build(root: PathBuf, hidden: Arc<Vec<String>>, allow_symlink: bool) -> Self {
        let entries = scan(&root, hidden.as_slice(), allow_symlink);
        Self {
            entries: RwLock::new(entries),
            root,
            hidden,
            allow_symlink,
        }
    }

    /// Return every indexed path located under one of `roots` whose file name
    /// (case-insensitively) contains `query_lower`.
    pub fn search(&self, roots: &[PathBuf], query_lower: &str) -> Vec<PathBuf> {
        let entries = self.entries.read().unwrap();
        entries
            .iter()
            .filter(|e| {
                roots.iter().any(|r| {
                    // Strict descendant: exclude `roots` themselves, matching the
                    // walker behavior that skips the root entry.
                    e.path
                        .strip_prefix(r)
                        .map(|p| !p.as_os_str().is_empty())
                        .unwrap_or(false)
                }) && e.name_lower.contains(query_lower)
            })
            .map(|e| e.path.clone())
            .collect()
    }

    fn rebuild(&self) {
        let entries = scan(&self.root, self.hidden.as_slice(), self.allow_symlink);
        if let Ok(mut guard) = self.entries.write() {
            *guard = entries;
        }
    }

    /// Start a background thread that watches `self.root` for changes and
    /// rebuilds the index after a short debounce. The thread exits once
    /// `running` becomes false.
    pub fn spawn_watcher(self: &Arc<Self>, running: Arc<AtomicBool>) {
        let index = Arc::clone(self);
        let root = self.root.clone();
        std::thread::spawn(move || watch_loop(index, root, running));
    }
}

fn scan(root: &Path, hidden: &[String], allow_symlink: bool) -> Vec<IndexEntry> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let collected: Arc<Mutex<Vec<IndexEntry>>> = Arc::new(Mutex::new(Vec::new()));

    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .standard_filters(false)
        .hidden(false)
        .follow_links(true)
        .threads(threads);

    let walker = builder.build_parallel();
    walker.run(|| {
        let collected = Arc::clone(&collected);
        Box::new(move |result| {
            let Ok(entry) = result else {
                return ignore::WalkState::Continue;
            };
            let path = entry.path().to_path_buf();
            let base_name = get_file_name(&path);
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if is_hidden(hidden, base_name, is_dir) {
                return ignore::WalkState::Continue;
            }
            if !allow_symlink {
                let inside = std::fs::canonicalize(&path)
                    .map(|p| p.starts_with(root))
                    .unwrap_or(false);
                if !inside {
                    return ignore::WalkState::Continue;
                }
            }
            collected.lock().unwrap().push(IndexEntry {
                path,
                name_lower: base_name.to_lowercase(),
            });
            ignore::WalkState::Continue
        })
    });

    match Arc::try_unwrap(collected) {
        Ok(mutex) => mutex.into_inner().unwrap(),
        Err(arc) => arc.lock().unwrap().clone(),
    }
}

fn watch_loop(index: Arc<SearchIndex>, root: PathBuf, running: Arc<AtomicBool>) {
    use notify::{RecursiveMode, Watcher};

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(watcher) => watcher,
        Err(_) => return,
    };
    if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
        return;
    }

    let mut dirty = false;
    while running.load(Ordering::SeqCst) {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(_)) => dirty = true,
            Ok(Err(_)) => {}
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                if dirty {
                    dirty = false;
                    index.rebuild();
                }
            }
        }
    }
}