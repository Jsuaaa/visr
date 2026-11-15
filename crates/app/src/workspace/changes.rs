//! The right panel listing the files git sees changed where the active terminal is.

use ui::file_icons;

use crate::git::{self, Change, Status};

use super::panel::{count_pill, panel_frame};
use super::*;

const BRANCH_ICON: &str = "\u{ea68}";
const ROW_HEIGHT_CHANGES: f32 = 24.0;

/// A line of the list: a group's header, or one of the snapshot's changes by its index.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ChangeRow {
  Header(&'static str, usize),
  Item(usize),
}

/// What `git status` said about a directory, and the lines it makes.
#[derive(Debug, PartialEq)]
pub(super) struct GitView {
  pub(super) dir: String,
  pub(super) result: Result<git::Snapshot, git::Error>,
  rows: Vec<ChangeRow>,
}

impl GitView {
  /// The repository's top folder, once git has said.
  pub(super) fn root(&self) -> Option<String> {
    self.result.as_ref().ok()?.root.clone()
  }

  pub(super) fn new(dir: String, result: Result<git::Snapshot, git::Error>) -> Self {
    let mut rows = Vec::new();
    if let Ok(snapshot) = &result {
      // The snapshot lists staged changes first.
      let staged = snapshot
        .changes
        .iter()
        .filter(|change| change.staged)
        .count();
      let unstaged = snapshot.changes.len() - staged;
      if staged > 0 {
        rows.push(ChangeRow::Header("Staged Changes", staged));
        rows.extend((0..staged).map(ChangeRow::Item));
      }
      if unstaged > 0 {
        // With nothing staged the panel's own title already says it.
        if staged > 0 {
          rows.push(ChangeRow::Header("Changes", unstaged));
        }
        rows.extend((staged..snapshot.changes.len()).map(ChangeRow::Item));
      }
    }
    Self { dir, result, rows }
  }
}

/// The letter and color the usual editors give each status.
fn badge(status: Status, colors: Colors) -> (&'static str, Hsla) {
  match status {
    Status::Modified => ("M", colors.git_modified),
    Status::Added => ("A", colors.git_added),
    Status::Deleted => ("D", colors.git_deleted),
    Status::Renamed => ("R", colors.git_untracked),
    Status::Untracked => ("U", colors.git_untracked),
    Status::Conflicted => ("!", colors.git_conflict),
  }
}

/// `src/lib.rs` as its name and the folder it is in.
fn split_path(path: &str) -> (&str, &str) {
  let trimmed = path.trim_end_matches('/');
  match trimmed.rfind('/') {
    Some(slash) => (&trimmed[slash + 1..], &trimmed[..slash]),
    None => (trimmed, ""),
  }
}

/// The last two folders of a path: what tells files apart, in the room there is.
fn short_folder(folder: &str) -> String {
  let parts: Vec<&str> = folder.split('/').collect();
  if parts.len() > 2 {
    format!("…/{}", parts[parts.len() - 2..].join("/"))
  } else {
    folder.to_string()
  }
}

fn section_header(title: &str, count: usize, colors: Colors) -> AnyElement {
  div()
    .h(px(ROW_HEIGHT_CHANGES))
    .w_full()
    .px(px(ROW_INSET))
    .flex()
    .items_center()
    .gap_2()
    .text_size(px(11.0))
    .text_color(colors.text_muted)
    .child(title.to_string())
    .child(count_pill(count, colors))
    .into_any_element()
}

impl Workspace {
  /// A change as a row: it opens its file in the editor the toolbar's menu offers, unless the
  /// file is gone.
  fn change_row(&self, change: &Change, root: Option<&str>, cx: &mut Context<Self>) -> AnyElement {
    let colors = Colors::active(cx);
    let (letter, color) = badge(change.status, colors);
    let (name, folder) = split_path(&change.path);
    // An untracked folder is listed by its own name, with a slash.
    let icon = if change.path.ends_with('/') {
      file_icons::folder(name)
    } else {
      file_icons::file(name)
    };
    let target = root.map(|root| std::path::Path::new(root).join(&change.path));
    let opens = change.status != Status::Deleted && !self.editors.is_empty();
    div()
      .id(SharedString::from(format!(
        "change-{}-{}",
        change.staged, change.path
      )))
      .h(px(ROW_HEIGHT_CHANGES))
      .w_full()
      .px(px(ROW_INSET))
      .flex()
      .items_center()
      .gap_2()
      .rounded_md()
      .when_some(target.filter(|_| opens), |row, target| {
        row
          .cursor_pointer()
          .hover(|row| row.bg(colors.surface_hover))
          .on_click(cx.listener(move |workspace, _, _, _| {
            workspace.open_in_editor(&target);
          }))
      })
      .child(gpui::img(icon).size(px(16.0)).flex_none())
      .child(
        div()
          .min_w_0()
          .truncate()
          .text_color(color)
          .when(change.status == Status::Deleted, |name| name.line_through())
          .child(name.to_string()),
      )
      .child(
        div()
          .flex_1()
          .min_w_0()
          .truncate()
          .text_size(px(11.0))
          .text_color(colors.text_muted)
          .child(short_folder(folder)),
      )
      .child(
        div()
          .w(px(12.0))
          .flex_none()
          .text_size(px(11.0))
          .text_color(color)
          .child(letter),
      )
      .into_any_element()
  }

  pub(super) fn render_changes(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let colors = Colors::active(cx);
    let dim = |text: &str| {
      div()
        .px_4()
        .py_2()
        .text_size(px(12.0))
        .text_color(colors.text_muted)
        .child(text.to_string())
        .into_any_element()
    };
    let git = self.git.as_ref();
    let (branch, count, body) = match git.map(|view| &view.result) {
      // Nothing to say until the first answer, unless there is nothing to ask about.
      None if self.layout.focused().is_none() => (None, None, dim("No terminal open.")),
      None => (None, None, div().into_any_element()),
      Some(Err(git::Error::NotARepository)) => (None, None, dim("Not a git repository.")),
      Some(Err(git::Error::Unavailable)) => (None, None, dim("git is not available.")),
      Some(Err(git::Error::Failed)) => (None, None, dim("git status failed.")),
      Some(Ok(snapshot)) if snapshot.changes.is_empty() => {
        (snapshot.branch.clone(), None, dim("No changes."))
      }
      Some(Ok(snapshot)) => (
        snapshot.branch.clone(),
        // The header that would carry the count is not drawn when nothing is staged.
        snapshot
          .changes
          .iter()
          .all(|change| !change.staged)
          .then_some(snapshot.changes.len()),
        gpui::uniform_list(
          "changes-list",
          git.map_or(0, |view| view.rows.len()),
          cx.processor(move |workspace, range: std::ops::Range<usize>, _, cx| {
            let Some(GitView {
              result: Ok(snapshot),
              rows,
              ..
            }) = &workspace.git
            else {
              return Vec::new();
            };
            let end = range.end.min(rows.len());
            let mut lines = Vec::new();
            for row in &rows[range.start.min(end)..end] {
              lines.push(match *row {
                ChangeRow::Header(title, count) => section_header(title, count, colors),
                ChangeRow::Item(index) => {
                  workspace.change_row(&snapshot.changes[index], snapshot.root.as_deref(), cx)
                }
              });
            }
            lines
          }),
        )
        .flex_1()
        .w_full()
        .min_h_0()
        .into_any_element(),
      ),
    };
    let branch = branch.map(|branch| {
      div()
        .min_w_0()
        .flex()
        .items_center()
        .gap_1()
        .child(div().font_family(ICON_FONT).child(BRANCH_ICON))
        .child(div().truncate().child(branch))
        .into_any_element()
    });
    panel_frame("CHANGES", count, branch, body, colors)
  }
}
