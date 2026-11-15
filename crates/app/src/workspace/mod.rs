//! The window's root: a sidebar of tabs and loose terminals, and the active tab's panes. The
//! layout is kept in the core (`doc.set`), so it survives closing the app.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use app_state::layout::{Axis, Branch, Layout, Node, split_size};
use app_state::{
  Entry, Indicator, Terminals, clean_exits, display_name, entry_label, indicator, panes_to_close,
  sidebar_entries,
};
use config::{Action, Config, KeyPress};
use gpui::{
  AnyElement, App, Bounds, Context, Entity, FocusHandle, Hsla, KeyDownEvent, MouseButton,
  MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, SharedString, Subscription, Task,
  Window, anchored, canvas, deferred, div, point, prelude::*, px, relative, rgb,
};
use hc_client::{Client, Notice};
use hc_proto::types::{
  Change, RemoveResult, RunParams, Size, SubscribeResult, TemplateInfo, Terminal, TerminalParams,
  TerminalState,
};
use ui::icons::{
  ICON_CODE, ICON_ELLIPSIS, ICON_EXPAND, ICON_FILES, ICON_FOLDER_OPENED, ICON_FONT, ICON_RESTORE,
  ICON_SOURCE_CONTROL, ICON_TERMINAL, ShellColors, StackColors, close_icon, glyph_icon, plus_icon,
  shell_icon, sidebar_icon, split_icon, stack_icon, titlebar_button,
};
use ui::menu::{MenuColors, menu_item, menu_item_icon, menu_separator};
use ui::motion::{self, Tween};
use ui::text_field::{FieldEvent, TextField, TextFieldColors};
use ui::tokens::{
  DIVIDER_WIDTH, HEADER_PADDING, PANE_BORDER_WIDTH, PANE_CHROME_HEIGHT, PANE_CHROME_WIDTH,
  PANE_EDGE, PANE_HEADER_HEIGHT, PANE_PADDING_BOTTOM, PANE_PADDING_LEFT, PANE_RADIUS, ROW_HEIGHT,
  ROW_INSET, TITLEBAR_BUTTON, TITLEBAR_HEIGHT, TITLEBAR_NUDGE, TOGGLE_GAP, TOGGLE_LEFT, TOGGLE_TOP,
  WINDOW_GAP,
};

mod actions;
mod changes;
mod files;
mod panel;
mod panes;
mod reload;
mod render;
mod resize;
mod session;
mod sidebar;
pub(crate) mod theme;
mod toolbar;

use changes::GitView;
use panel::{OutputLook, PANEL_WIDTH, Panel, PanelSwitch, TreeState};
use resize::{Edge, EdgeDrag};
use sidebar::row_key;
use theme::*;
use toolbar::{TOOLBAR_HEIGHT, ToolbarMenu};

use crate::SIDEBAR_WIDTH;
use crate::connection::{Connection, LAYOUT_DOC, missed_snapshot, subscribe};
use crate::editors::{self, Editor};
use crate::input;
use crate::terminal_view::{Output, PaneFocused, TerminalView, hsla};

const RECONNECT_INTERVAL: Duration = Duration::from_secs(1);

/// The config file's modification time when last looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigSeen {
  /// No look yet: the first one only records.
  NotYet,
  /// The file did not exist, or could not be read.
  Missing,
  /// Last modified at this time.
  At(SystemTime),
}

/// The context menu of a sidebar row, open at `position`. A row is known by its first terminal,
/// since rows move and tabs renumber while the menu is open.
struct RowMenu {
  position: Point<Pixels>,
  row: String,
  /// Fades in.
  shown: Tween,
}

/// A sidebar row being renamed in place.
struct Renaming {
  row: String,
  field: Entity<TextField>,
  _events: Subscription,
}

/// A pane zoomed over its tab, or on its way back.
struct Zoom {
  tab: usize,
  address: String,
  /// 0 in its place, 1 over the whole tab.
  tween: Tween,
  /// Set when restoring: drops the zoom once it is back in place.
  _done: Option<Task<()>>,
}

/// A pane that is animating out before it is removed from the layout.
struct ClosingPane {
  tab: usize,
  /// The split the pane is a side of.
  path: Vec<Branch>,
  address: String,
  /// Its title as it was: the terminal may leave the core before the pane is gone.
  title: String,
  tween: Tween,
  _done: Task<()>,
}

/// The view of one terminal in the layout.
struct Pane {
  view: Entity<TerminalView>,
  _focused: Subscription,
  _output: Subscription,
}

/// The window's content: sidebar, tabs and panes, toolbar and side panel.
pub(crate) struct Workspace {
  client: Arc<Client>,
  terminals: Terminals,
  templates: Vec<TemplateInfo>,
  layout: Layout,
  layout_version: u64,
  /// Views for every pane in every tab, kept while hidden so a tab comes back as it was.
  panes: HashMap<String, Pane>,
  /// Where each split was laid out, written during layout and read while dragging a divider.
  split_bounds: Rc<RefCell<HashMap<Vec<Branch>, Bounds<Pixels>>>>,
  dragging: Option<(Vec<Branch>, Axis)>,
  config: Config,
  /// The config file is invalid; the last good config stays in use.
  config_error: Option<String>,
  /// The connection to the core dropped and a new one is being tried.
  reconnecting: bool,
  menu: Option<RowMenu>,
  toolbar_menu: Option<ToolbarMenu>,
  /// Where the toolbar's menu button was laid out, for the menu to open from.
  more_bounds: Rc<RefCell<Option<Bounds<Pixels>>>>,
  /// The panel on the right: which is open, the one drawn (it stays while the panel slides
  /// shut), and how open it is drawn, 0 to 1.
  panel: Option<Panel>,
  shown_panel: Panel,
  panel_motion: Tween,
  /// How wide the sidebar and the panel are, as dragged; the defaults to begin with.
  sidebar_width: f32,
  panel_width: f32,
  /// What is drawn: the widths asked for, fitted to the window.
  sidebar_drawn: f32,
  panel_drawn: f32,
  edge_drag: Option<EdgeDrag>,
  /// Views of the panel trading places, when the toolbar picks the other one.
  panel_switch: Option<PanelSwitch>,
  git: Option<GitView>,
  files: TreeState,
  /// The look at the active terminal's folder for the open panel, which a newer one replaces.
  _refresh: Option<Task<()>>,
  /// When the panel next looks again because the active terminal printed something.
  output_look: OutputLook,
  /// The size each terminal just started was given, for its first attach.
  started_sizes: HashMap<String, (u16, u16)>,
  /// The terminal the panel last looked for, to notice when another one becomes the active one.
  panel_focus: Option<String>,
  /// The listings under way for folders just opened; closing a folder drops its listing.
  listing_tasks: HashMap<PathBuf, Task<()>>,
  /// Code editors found at start, best first.
  editors: Vec<Editor>,
  renaming: Option<Renaming>,
  sidebar_open: bool,
  /// How open the sidebar is drawn, 0 to 1, easing toward `sidebar_open`.
  sidebar: Tween,
  /// A split just made, easing from all-first to its ratio: the tab, the split's path, the ratio.
  split_motion: Option<(usize, Vec<Branch>, Tween)>,
  /// A pane on its way out: its split folds toward the pane that stays, then it leaves the layout.
  closing_pane: Option<ClosingPane>,
  /// A pane drawn over the rest of its tab, as if alone.
  zoom: Option<Zoom>,
  /// Where each pane and the area they share were laid out, for a zoom to grow from.
  pane_bounds: Rc<RefCell<HashMap<String, Bounds<Pixels>>>>,
  panes_area: Rc<RefCell<Option<Bounds<Pixels>>>>,
  /// The tab in sight, by its first terminal, and its fade in when it comes into sight.
  shown_tab: Option<String>,
  tab_fade: Tween,
  /// The window coming in from the setup: the curtain of the window's color lifts.
  entrance: Tween,
  /// Sidebar rows by their first terminal: those seen so far, and new ones growing in.
  /// `None` until the first frame, so rows already there at start don't grow.
  known_rows: Option<HashSet<String>>,
  row_motion: HashMap<String, Tween>,
  /// Keeps the shortcuts working while no pane has the keyboard (no terminal is open).
  focus_handle: FocusHandle,
  config_seen: ConfigSeen,
  /// The look at the config file in flight; a newer one replaces it.
  _config_check: Option<Task<()>>,
  /// Looks at the config file whenever the window becomes active.
  _activation: Subscription,
  _tasks: Vec<Task<()>>,
}

impl Workspace {
  pub(crate) fn new(
    connection: Connection,
    initial: Option<String>,
    config: Config,
    config_error: Option<String>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Self {
    let Connection {
      client,
      snapshot,
      events: stream,
      templates,
      saved_layout: saved,
      layout_version,
    } = connection;
    let client = Arc::new(client);

    let window_handle = window.window_handle();
    let events = cx.spawn(async move |this, cx| {
      let mut stream = stream;
      'connection: loop {
        let notice = stream.recv_async().await;
        // A `missed` notice in a shape we don't know is as good as a lost connection: subscribing
        // again brings a snapshot we can trust, where carrying on would let the state drift.
        let unusable = matches!(&notice, Ok(Some(Notice::Missed(params)))
          if missed_snapshot(params.clone()).is_none());
        let notice = match notice {
          Ok(Some(notice)) if !unusable => notice,
          Ok(_) | Err(hc_client::Error::Disconnected) => {
            // The core went away (a restart, an update): keep trying until a new one answers. The
            // banner is for that; a notice we can't read loses nothing, it only needs a new
            // subscription.
            let lost = this.update(cx, |workspace, cx| {
              workspace.reconnecting = !unusable;
              cx.notify();
            });
            if lost.is_err() {
              break;
            }
            let (client, snapshot, new_stream) = loop {
              cx.background_executor().timer(RECONNECT_INTERVAL).await;
              let attempt = cx.background_executor().spawn(async { subscribe() }).await;
              if let Ok(link) = attempt {
                break link;
              }
            };
            stream = new_stream;
            let client = Arc::new(client);
            let resumed = cx.update_window(window_handle, |_, window, cx| {
              this.update(cx, |workspace, cx| {
                workspace.reconnected(client, snapshot, window, cx)
              })
            });
            if resumed.is_err() {
              break;
            }
            continue 'connection;
          }
          // One notice that doesn't parse must not stop the ones after it.
          Err(_) => continue,
        };
        let applied = cx.update_window(window_handle, |_, window, cx| {
          this.update(cx, |workspace, cx| workspace.on_notice(notice, window, cx))
        });
        if applied.is_err() {
          break;
        }
      }
    });

    let activation = cx.observe_window_activation(window, |workspace, window, cx| {
      if window.is_window_active() {
        workspace.check_config(window, cx);
        workspace.refresh_panel(cx);
      }
    });

    let live: Vec<&str> = snapshot
      .terminals
      .iter()
      .filter(|terminal| terminal.state == TerminalState::Live)
      .map(|terminal| terminal.address.as_str())
      .collect();
    let mut layout = saved.unwrap_or_default();
    layout.keep_only(&live);
    if layout.active().is_none()
      && let Some(first) = live.first()
    {
      layout.new_tab(first);
    }
    if let Some(address) = &initial {
      layout.show(address);
    }

    let mut workspace = Self {
      client,
      terminals: Terminals::from_snapshot(snapshot),
      templates,
      layout,
      layout_version,
      panes: HashMap::new(),
      split_bounds: Rc::default(),
      dragging: None,
      config,
      config_error,
      reconnecting: false,
      menu: None,
      toolbar_menu: None,
      more_bounds: Rc::default(),
      panel: None,
      shown_panel: Panel::Changes,
      panel_motion: Tween::settled(0.0),
      sidebar_width: SIDEBAR_WIDTH,
      panel_width: PANEL_WIDTH,
      sidebar_drawn: SIDEBAR_WIDTH,
      panel_drawn: PANEL_WIDTH,
      edge_drag: None,
      panel_switch: None,
      git: None,
      files: TreeState::default(),
      _refresh: None,
      output_look: OutputLook::default(),
      started_sizes: HashMap::new(),
      panel_focus: None,
      listing_tasks: HashMap::new(),
      config_seen: ConfigSeen::NotYet,
      _config_check: None,
      _activation: activation,
      editors: editors::installed(),
      renaming: None,
      sidebar_open: true,
      sidebar: Tween::settled(1.0),
      split_motion: None,
      closing_pane: None,
      zoom: None,
      pane_bounds: Rc::default(),
      panes_area: Rc::default(),
      shown_tab: None,
      focus_handle: cx.focus_handle(),
      tab_fade: Tween::settled(1.0),
      entrance: Tween::settled(1.0),
      known_rows: None,
      row_motion: HashMap::new(),
      _tasks: vec![events],
    };
    workspace.remove_clean_exits();
    workspace.changed(window, cx);
    workspace.check_config(window, cx);
    workspace
  }

  /// After any layout change: save it, match the panes to the active tab, focus the right one.
  fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.sync_panes(window, cx);
    self.save();
    self.note_focus(cx);
    cx.notify();
  }
}

/// The colors every context menu draws its rows and separators with.
fn menu_colors() -> MenuColors {
  MenuColors {
    separator: rgb(MENU_BORDER).into(),
    hover_background: rgb(MENU_HOVER_BG).into(),
    dim_text: rgb(DIM_TEXT).into(),
  }
}

/// A title bar or toolbar button in the workspace's colors.
fn chrome_button(id: &'static str) -> gpui::Stateful<gpui::Div> {
  titlebar_button(id, rgb(ROW_HOVER_BG).into()).text_color(rgb(DIM_TEXT))
}

/// A double click on the title bar does what the system says: zoom the window by default.
fn on_titlebar_mouse_down(event: &MouseDownEvent, window: &mut Window, _cx: &mut App) {
  if event.click_count == 2 {
    window.titlebar_double_click();
  }
}
