//! A folder as a tree of rows: what the files panel lists, with the folders opened so far.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A file or folder directly inside a listed folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
  pub(crate) name: String,
  pub(crate) is_dir: bool,
}

/// One visible line of the file tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
  pub(crate) path: PathBuf,
  pub(crate) name: String,
  pub(crate) depth: usize,
  pub(crate) is_dir: bool,
  pub(crate) expanded: bool,
}

/// Never worth listing: git's own folder and what the Finder and Explorer leave around.
fn hidden(name: &str) -> bool {
  matches!(name, ".git" | ".DS_Store" | "Thumbs.db" | "desktop.ini")
}

/// What is in `dir`: folders first, then files, each by name without regard to case.
pub(crate) fn list(dir: &Path) -> Vec<Entry> {
  let Ok(read) = std::fs::read_dir(dir) else {
    return Vec::new();
  };
  let mut entries: Vec<Entry> = read
    .filter_map(Result::ok)
    .filter_map(|entry| {
      let name = entry.file_name().to_string_lossy().into_owned();
      // A link to a folder is a folder.
      let is_dir = entry.path().is_dir();
      (!hidden(&name)).then_some(Entry { name, is_dir })
    })
    .collect();
  entries.sort_by_cached_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
  entries
}

/// The rows to show: `root`'s entries, and under each opened folder its own, indented.
pub(crate) fn flatten(
  root: &Path,
  listings: &HashMap<PathBuf, Vec<Entry>>,
  expanded: &HashSet<PathBuf>,
) -> Vec<Row> {
  let mut rows = Vec::new();
  push(root, 0, listings, expanded, &mut rows);
  rows
}

fn push(
  dir: &Path,
  depth: usize,
  listings: &HashMap<PathBuf, Vec<Entry>>,
  expanded: &HashSet<PathBuf>,
  rows: &mut Vec<Row>,
) {
  for entry in listings.get(dir).into_iter().flatten() {
    let path = dir.join(&entry.name);
    let open = entry.is_dir && expanded.contains(&path);
    rows.push(Row {
      path: path.clone(),
      name: entry.name.clone(),
      depth,
      is_dir: entry.is_dir,
      expanded: open,
    });
    if open {
      push(&path, depth + 1, listings, expanded, rows);
    }
  }
}
