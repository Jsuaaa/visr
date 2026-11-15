//! The files git sees changed in a directory, read from `git status`: no dependency, and the same
//! answer the user's own git gives.

use std::path::Path;
use std::process::Command;

/// What happened to a changed file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
  Modified,
  Added,
  Deleted,
  Renamed,
  Untracked,
  Conflicted,
}

/// One changed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
  pub(crate) path: String,
  pub(crate) status: Status,
  /// In the index, where `git commit` takes it from. A file changed in both places is listed
  /// once for each.
  pub(crate) staged: bool,
}

/// What `git status` reported for a folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
  /// `None` on a detached HEAD.
  pub(crate) branch: Option<String>,
  pub(crate) changes: Vec<Change>,
  /// The repository's top folder, which the changes' paths are relative to.
  pub(crate) root: Option<String>,
}

/// Why there is no snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Error {
  NotARepository,
  /// `git` is not installed, or could not run.
  Unavailable,
  /// It ran and refused: an unsafe directory, a damaged index.
  Failed,
}

/// A `git` run in `dir`, as that directory's own: not whatever repository the environment names.
fn command(dir: &Path) -> Command {
  let mut command = Command::new("git");
  crate::editors::no_console(&mut command);
  command
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .env_remove("GIT_INDEX_FILE")
    // Its messages are read in English.
    .env("LC_ALL", "C")
    .arg("--no-optional-locks")
    .current_dir(dir);
  command
}

/// `root` is the repository's top folder when it is already known for `dir`: it only changes
/// with the directory, so a later look need not ask for it again.
pub(crate) fn status(dir: &Path, root: Option<String>) -> Result<Snapshot, Error> {
  let output = command(dir)
    .args([
      "-c",
      "core.quotepath=off",
      "status",
      "--porcelain=v2",
      "-z",
      "--branch",
      // Every file, not the folder holding untracked ones.
      "--untracked-files=all",
    ])
    .output()
    .map_err(|_| Error::Unavailable)?;
  if !output.status.success() {
    let said = String::from_utf8_lossy(&output.stderr);
    return Err(if said.contains("not a git repository") {
      Error::NotARepository
    } else {
      Error::Failed
    });
  }
  let mut snapshot = parse(&String::from_utf8_lossy(&output.stdout));
  // The paths above are from the repository's root, wherever `dir` is inside it.
  snapshot.root = root.or_else(|| {
    command(dir)
      .args(["rev-parse", "--show-toplevel"])
      .output()
      .ok()
      .filter(|output| output.status.success())
      .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
  });
  Ok(snapshot)
}

fn status_of(code: char) -> Option<Status> {
  match code {
    'M' | 'T' => Some(Status::Modified),
    'A' | 'C' => Some(Status::Added),
    'D' => Some(Status::Deleted),
    'R' => Some(Status::Renamed),
    _ => None,
  }
}

/// `git status --porcelain=v2 -z --branch`: records end in NUL, and a rename is followed by a
/// record holding the path it came from.
fn parse(output: &str) -> Snapshot {
  let mut snapshot = Snapshot::default();
  let mut records = output.split('\0');
  while let Some(record) = records.next() {
    if let Some(head) = record.strip_prefix("# branch.head ") {
      snapshot.branch = (head != "(detached)").then(|| head.to_string());
    } else if let Some(path) = record.strip_prefix("? ") {
      snapshot.changes.push(Change {
        path: path.to_string(),
        status: Status::Untracked,
        staged: false,
      });
    } else if let Some(rest) = record.strip_prefix("u ") {
      // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
      if let Some(path) = rest.splitn(10, ' ').nth(9) {
        snapshot.changes.push(Change {
          path: path.to_string(),
          status: Status::Conflicted,
          staged: false,
        });
      }
    } else if let Some(rest) = record.strip_prefix("1 ") {
      // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
      let mut fields = rest.splitn(8, ' ');
      let code = fields.next().unwrap_or("..");
      if let Some(path) = fields.nth(6) {
        push_tracked(&mut snapshot, code, path);
      }
    } else if let Some(rest) = record.strip_prefix("2 ") {
      // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>, then the original path
      let mut fields = rest.splitn(9, ' ');
      let code = fields.next().unwrap_or("..");
      let path = fields.nth(7);
      let _original = records.next();
      if let Some(path) = path {
        push_tracked(&mut snapshot, code, path);
      }
    }
  }
  snapshot
    .changes
    .sort_by(|a, b| b.staged.cmp(&a.staged).then_with(|| a.path.cmp(&b.path)));
  snapshot
}

fn push_tracked(snapshot: &mut Snapshot, code: &str, path: &str) {
  let mut letters = code.chars();
  let (index, tree) = (letters.next().unwrap_or('.'), letters.next().unwrap_or('.'));
  for (letter, staged) in [(index, true), (tree, false)] {
    if let Some(status) = status_of(letter) {
      snapshot.changes.push(Change {
        path: path.to_string(),
        status,
        staged,
      });
    }
  }
}
