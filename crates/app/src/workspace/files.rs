//! The files panel: the tree of the folder the active terminal is in.

use std::ops::Range;
use std::path::PathBuf;

use ui::file_icons;

use crate::tree;

use super::panel::panel_frame;
use super::*;

const ROW_HEIGHT_FILES: f32 = 24.0;
const INDENT: f32 = 14.0;
/// Codicons: chevrons for a folder closed and open.
const CHEVRON_RIGHT: &str = "\u{eab6}";
const CHEVRON_DOWN: &str = "\u{eab4}";

impl Workspace {
  /// Opens or closes a folder. It opens at once and its entries follow as soon as the background
  /// listing is back. A folder whose listing is still in `files.listings` shows its old entries
  /// meanwhile; one opened for the first time, or after a refresh dropped the listings of closed
  /// folders, shows no children until the listing arrives. Closing drops a listing still under
  /// way, so it can't open the folder again.
  fn toggle_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    if self.files.expanded.remove(&path) {
      self.listing_tasks.remove(&path);
    } else {
      self.files.expanded.insert(path.clone());
      let listed = path.clone();
      let task = cx.spawn(async move |this, cx| {
        let entries = cx
          .background_executor()
          .spawn({
            let listed = listed.clone();
            async move { tree::list(&listed) }
          })
          .await;
        let _ = this.update(cx, |workspace, cx| {
          workspace.listing_tasks.remove(&listed);
          if workspace.files.expanded.contains(&listed) {
            workspace.files.listings.insert(listed, entries);
            workspace.files.rebuild();
            cx.notify();
          }
        });
      });
      self.listing_tasks.insert(path, task);
    }
    self.files.rebuild();
    cx.notify();
  }

  fn file_rows(&self, range: Range<usize>) -> Vec<tree::Row> {
    let rows = &self.files.rows;
    let end = range.end.min(rows.len());
    rows[range.start.min(end)..end].to_vec()
  }

  fn file_row(&self, row: tree::Row, cx: &mut Context<Self>) -> AnyElement {
    let icon = if row.is_dir {
      file_icons::folder(&row.name)
    } else {
      file_icons::file(&row.name)
    };
    let chevron = row.is_dir.then_some(if row.expanded {
      CHEVRON_DOWN
    } else {
      CHEVRON_RIGHT
    });
    let path = row.path.clone();
    // A folder opens; a file opens in the editor, when there is one.
    let acts = row.is_dir || !self.editors.is_empty();
    let is_dir = row.is_dir;
    div()
      .id(SharedString::from(row.path.to_string_lossy().into_owned()))
      .relative()
      .h(px(ROW_HEIGHT_FILES))
      .w_full()
      .pl(px(ROW_INSET + INDENT * row.depth as f32))
      .pr(px(ROW_INSET))
      .flex()
      .items_center()
      .gap_1()
      .rounded_md()
      .when(acts, |item| {
        item
          .cursor_pointer()
          .hover(|item| item.bg(rgb(ROW_HOVER_BG)))
          .on_click(cx.listener(move |workspace, _, _, cx| {
            if is_dir {
              workspace.toggle_dir(path.clone(), cx);
            } else {
              workspace.open_in_editor(&path);
            }
          }))
      })
      // A faint line under each ancestor's chevron, to follow deep nesting.
      .children((0..row.depth).map(|level| {
        div()
          .absolute()
          .top_0()
          .h_full()
          .left(px(ROW_INSET + INDENT * level as f32 + 6.0))
          .w(px(1.0))
          .bg(rgb(PANE_BORDER))
      }))
      .child(
        div()
          .w(px(12.0))
          .flex_none()
          .font_family(ICON_FONT)
          .text_size(px(11.0))
          .text_color(rgb(DIM_TEXT))
          .children(chevron),
      )
      .child(gpui::img(icon).size(px(16.0)).flex_none())
      .child(div().min_w_0().truncate().child(row.name))
      .into_any_element()
  }

  pub(super) fn render_files(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let folder = self
      .files
      .root
      .as_ref()
      .and_then(|root| root.file_name())
      .map(|name| name.to_string_lossy().into_owned())
      .map(|name| div().min_w_0().truncate().child(name).into_any_element());
    let body = if self.files.root.is_some() {
      gpui::uniform_list(
        "files-list",
        self.files.rows.len(),
        cx.processor(|workspace, range: Range<usize>, _, cx| {
          let mut rows = Vec::new();
          for row in workspace.file_rows(range) {
            rows.push(workspace.file_row(row, cx));
          }
          rows
        }),
      )
      .flex_1()
      .w_full()
      .min_h_0()
      .into_any_element()
    } else if self.layout.focused().is_none() {
      div()
        .px_4()
        .py_2()
        .text_size(px(12.0))
        .text_color(rgb(DIM_TEXT))
        .child("No terminal open.")
        .into_any_element()
    } else {
      // Nothing to show until the first listing comes back.
      div().into_any_element()
    };
    panel_frame("FILES", None, folder, body)
  }
}
