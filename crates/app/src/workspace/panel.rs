//! The panel on the window's right: the files of where the active terminal is, or what git sees
//! changed there. One at a time; the toolbar picks. It is looked up when something may have
//! changed, not on a timer: see `refresh_panel`. What the active terminal prints is one of the
//! signs, thinned out by `note_output`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::git;
use crate::tree;

use super::toolbar::{TOOLBAR_HEIGHT, terminal_dir};
use super::*;

pub(super) const PANEL_WIDTH: f32 = 280.0;

/// How long the active terminal stays quiet before the panel looks again.
const OUTPUT_QUIET: Duration = Duration::from_millis(300);
/// The longest the panel waits, and the least between two looks, while output keeps coming.
const OUTPUT_INTERVAL: Duration = Duration::from_secs(1);

/// Where the look that follows the active terminal's output stands.
#[derive(Default)]
pub(super) struct OutputLook {
  /// When the output that the scheduled look is for began.
  since: Option<Instant>,
  /// When the last look it scheduled ran.
  last: Option<Instant>,
  /// The scheduled look; replaced, and so cancelled, by newer output.
  task: Option<Task<()>>,
}

/// Which panel is open on the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Panel {
  Files,
  Changes,
}

impl Panel {
  /// Where the toolbar lists it, left to right: switching slides toward the new one's side.
  fn order(self) -> u8 {
    match self {
      Panel::Files => 0,
      Panel::Changes => 1,
    }
  }
}

/// One view of the panel sliding out and another in.
pub(super) struct PanelSwitch {
  from: Panel,
  to: Panel,
  pub(super) tween: Tween,
}

impl Workspace {
  /// Opens `panel`, or closes the panel if it is the one open. Another panel slides in its place.
  pub(super) fn toggle_panel(&mut self, panel: Panel, cx: &mut Context<Self>) {
    let closing = self.panel == Some(panel);
    let was_open = self.panel.is_some();
    self.panel = (!closing).then_some(panel);
    self.panel_switch = (was_open && !closing).then(|| PanelSwitch {
      from: self.shown_panel,
      to: panel,
      tween: Tween::new(0.0, 1.0, Instant::now(), motion::duration()),
    });
    if !closing {
      self.shown_panel = panel;
    }
    if closing || !was_open {
      let target = if closing { 0.0 } else { 1.0 };
      self.panel_motion = self
        .panel_motion
        .retarget(target, Instant::now(), motion::duration());
    }
    self.refresh_panel(cx);
    cx.notify();
  }

  /// Looks at the active terminal's folder for the open panel, off the UI thread: its git status
  /// or its file listing. A newer look replaces one still under way, and with no panel open
  /// nothing runs or is scheduled.
  ///
  /// There is no polling. It runs when a panel is opened or switched, when another terminal becomes
  /// the active one, when the window becomes active, and when the active terminal's foreground
  /// program, status or end is reported by the core, and after the active terminal prints
  /// something (see `note_output`). So a program that changes files without printing, while the
  /// window stays active and the terminal's status stays the same, is not noticed until one of
  /// those happens. Neither is output or changes from a terminal other than the active one, such
  /// as an agent in another split working in the same repository.
  pub(super) fn refresh_panel(&mut self, cx: &mut Context<Self>) {
    let Some(panel) = self.panel else {
      self._refresh = None;
      self.output_look = OutputLook::default();
      return;
    };
    let client = Arc::clone(&self.client);
    let focused = self.layout.focused().map(str::to_string);
    let expanded = self.files.expanded.clone();
    let last = self
      .git
      .as_ref()
      .map(|view| (view.dir.clone(), view.root()));
    self._refresh = Some(cx.spawn(async move |this, cx| {
      let seen = cx
        .background_executor()
        .spawn({
          let expanded = expanded.clone();
          async move {
            let dir = focused.and_then(|address| terminal_dir(&client, &address));
            look(panel, dir, &expanded, last)
          }
        })
        .await;
      let _ = this.update(cx, |workspace, cx| {
        workspace.apply_look(seen, &expanded, cx)
      });
    }));
  }

  /// The terminal at `address` printed something. Commands like `touch` and `rm` change files
  /// without the core reporting anything, but they print, at least the next prompt. If it is the
  /// active terminal and a panel is open, one look is scheduled for when the output has been quiet
  /// for a moment. Output that never stops is looked at after about a second, and no two looks
  /// from here come closer than that. A newer output moves the scheduled look, never adds one.
  pub(super) fn note_output(&mut self, address: &str, cx: &mut Context<Self>) {
    if self.panel.is_none() || self.layout.focused() != Some(address) {
      return;
    }
    let now = Instant::now();
    let since = *self.output_look.since.get_or_insert(now);
    let mut at = (now + OUTPUT_QUIET).min(since + OUTPUT_INTERVAL);
    if let Some(last) = self.output_look.last {
      at = at.max(last + OUTPUT_INTERVAL);
    }
    self.output_look.task = Some(cx.spawn(async move |this, cx| {
      cx.background_executor()
        .timer(at.saturating_duration_since(Instant::now()))
        .await;
      let _ = this.update(cx, |workspace, cx| {
        workspace.output_look.since = None;
        workspace.output_look.last = Some(Instant::now());
        workspace.refresh_panel(cx);
      });
    }));
  }

  /// Looks again when another terminal has become the active one.
  pub(super) fn note_focus(&mut self, cx: &mut Context<Self>) {
    let focused = self.layout.focused().map(str::to_string);
    if focused != self.panel_focus {
      self.panel_focus = focused;
      self.refresh_panel(cx);
    }
  }

  /// Shows what a look found, telling the window only when it differs from what is shown.
  fn apply_look(&mut self, seen: Seen, asked: &HashSet<PathBuf>, cx: &mut Context<Self>) {
    match seen {
      Seen::Git(view) => {
        let view = Some(view);
        if self.git != view {
          self.git = view;
          cx.notify();
        }
      }
      Seen::Files(dir, mut listings) => {
        // A folder opened while this was listing was listed by its click, and the listing above
        // did not know of it: keep that one, or it shows empty until the next look.
        for (opened, entries) in &self.files.listings {
          if self.files.expanded.contains(opened) && !asked.contains(opened) {
            listings
              .entry(opened.clone())
              .or_insert_with(|| entries.clone());
          }
        }
        let view = TreeState::new(
          Some(PathBuf::from(dir)),
          self.files.expanded.clone(),
          listings,
        );
        if self.files != view {
          self.files = view;
          cx.notify();
        }
      }
      Seen::NoFolder => {
        if self.git.is_some() || self.files.root.is_some() {
          self.git = None;
          self.files = TreeState::default();
          cx.notify();
        }
      }
    }
  }

  /// Where a new editor window or file opens: the best editor found, if any.
  pub(super) fn open_in_editor(&self, path: &std::path::Path) {
    if let Some(editor) = self.editors.first() {
      editor.open(path);
    }
  }

  fn render_view(&self, panel: Panel, cx: &mut Context<Self>) -> AnyElement {
    match panel {
      Panel::Changes => self.render_changes(cx).into_any_element(),
      Panel::Files => self.render_files(cx).into_any_element(),
    }
  }

  pub(super) fn render_panel(&self, now: Instant, cx: &mut Context<Self>) -> AnyElement {
    // Ends where the panes do: above the toolbar's room, when there is a toolbar.
    let bottom = if self.layout.focused().is_some() {
      TOOLBAR_HEIGHT + 8.0
    } else {
      8.0
    };
    // Every view has the same frame, moving or still, so nothing shifts when a slide ends.
    let framed = |view: AnyElement| {
      div()
        .size_full()
        .pb(px(bottom))
        // The window's padding on its right, as the panes have.
        .pr_2()
        .child(view)
        .into_any_element()
    };
    match &self.panel_switch {
      // Both views side by side, moving together: the old one leaves toward the side the new
      // one's button is on, and the new one comes from there.
      Some(switch) if !switch.tween.done(now) => {
        let t = switch.tween.value(now);
        let toward = if switch.to.order() > switch.from.order() {
          1.0
        } else {
          -1.0
        };
        let at = |x: f32, view: AnyElement| {
          div()
            .absolute()
            .top_0()
            .left(px(x))
            .w(px(self.panel_drawn))
            .h_full()
            .child(view)
        };
        div()
          .relative()
          .size_full()
          .overflow_hidden()
          .child(at(
            -toward * t * self.panel_drawn,
            framed(self.render_view(switch.from, cx)),
          ))
          .child(at(
            toward * (1.0 - t) * self.panel_drawn,
            framed(self.render_view(switch.to, cx)),
          ))
          .into_any_element()
      }
      _ => framed(self.render_view(self.shown_panel, cx)),
    }
  }
}

/// What a look at the active terminal's folder found.
enum Seen {
  Git(GitView),
  /// The folder and its listings.
  Files(String, HashMap<PathBuf, Vec<tree::Entry>>),
  /// The active terminal's folder is not known, or there is no terminal.
  NoFolder,
}

/// Reads what `panel` shows for `dir`: blocking work, for the background executor. `last` is the
/// folder and repository root of the git view shown, whose root is reused while the folder is
/// the same.
fn look(
  panel: Panel,
  dir: Option<String>,
  expanded: &HashSet<PathBuf>,
  last: Option<(String, Option<String>)>,
) -> Seen {
  let Some(dir) = dir else {
    return Seen::NoFolder;
  };
  match panel {
    Panel::Changes => {
      let root = last.and_then(|(last_dir, root)| (last_dir == dir).then_some(root)?);
      let result = git::status(std::path::Path::new(&dir), root);
      Seen::Git(GitView::new(dir, result))
    }
    Panel::Files => {
      let listings = list_open(&dir, expanded);
      Seen::Files(dir, listings)
    }
  }
}

/// The folders the panel shows: the root and each one opened under it, and the rows they make.
#[derive(Debug, Default, PartialEq)]
pub(super) struct TreeState {
  pub(super) root: Option<PathBuf>,
  pub(super) expanded: HashSet<PathBuf>,
  pub(super) listings: HashMap<PathBuf, Vec<tree::Entry>>,
  /// Kept rather than rebuilt per frame: a big opened folder is thousands of rows.
  pub(super) rows: Vec<tree::Row>,
}

impl TreeState {
  pub(super) fn new(
    root: Option<PathBuf>,
    expanded: HashSet<PathBuf>,
    listings: HashMap<PathBuf, Vec<tree::Entry>>,
  ) -> Self {
    let mut state = Self {
      root,
      expanded,
      listings,
      rows: Vec::new(),
    };
    state.rebuild();
    state
  }

  /// Call after changing `expanded` or `listings`.
  pub(super) fn rebuild(&mut self) {
    self.rows = match &self.root {
      Some(root) => tree::flatten(root, &self.listings, &self.expanded),
      None => Vec::new(),
    };
  }
}

/// The root's folders and those opened under an opened folder: what the rows can show. A folder
/// opened inside one that was closed since is not read.
fn list_open(root: &str, expanded: &HashSet<PathBuf>) -> HashMap<PathBuf, Vec<tree::Entry>> {
  let mut listings = HashMap::new();
  let mut pending = vec![PathBuf::from(root)];
  while let Some(dir) = pending.pop() {
    let entries = tree::list(&dir);
    pending.extend(
      entries
        .iter()
        .filter(|entry| entry.is_dir)
        .map(|entry| dir.join(&entry.name))
        .filter(|path| expanded.contains(path)),
    );
    listings.insert(dir, entries);
  }
  listings
}

/// Both panels' frame, laid out like the sidebar: a small spacer, the label row (`side` dimmed at
/// its right), then the body.
pub(super) fn panel_frame(
  title: &str,
  count: Option<usize>,
  side: Option<AnyElement>,
  body: AnyElement,
) -> impl IntoElement {
  let label = div()
    .px(px(ROW_INSET))
    .pt_1()
    .pb_1()
    .flex()
    .items_center()
    .justify_between()
    .gap_2()
    .text_size(px(11.0))
    .text_color(rgb(DIM_TEXT))
    .child(
      div()
        .flex()
        .items_center()
        .gap_2()
        .child(title.to_string())
        .children(count.map(count_pill)),
    )
    .children(side);
  div()
    .size_full()
    .flex()
    .flex_col()
    .text_size(px(13.0))
    // Level with the panes' top edge, where the first pane's header starts.
    .child(div().h(px(8.0)).flex_none())
    .child(label)
    .child(body)
}

/// A small count beside a title.
pub(super) fn count_pill(count: usize) -> AnyElement {
  div()
    .px_1()
    .rounded_sm()
    .bg(rgb(ICON_BG))
    .text_size(px(10.0))
    .child(count.to_string())
    .into_any_element()
}
