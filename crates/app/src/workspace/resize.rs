//! Dragging the sidebar's and the right panel's edge to change their width, with a pull toward
//! the default one.

use super::*;

/// How close to the default a drag lands on it.
const SNAP: f32 = 12.0;
const SIDEBAR_RANGE: (f32, f32) = (160.0, 480.0);
const PANEL_RANGE: (f32, f32) = (200.0, 640.0);
/// The panes always keep this much of the window.
const MIN_MAIN: f32 = 320.0;

/// A draggable edge between the main area and a side column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Edge {
  Sidebar,
  Panel,
}

/// An edge being dragged: where the pointer was, and how wide it was, when the drag began.
pub(super) struct EdgeDrag {
  edge: Edge,
  start_x: Pixels,
  start_width: f32,
}

/// The width a drag asks for, kept within `range` and pulled onto `default` when near it.
fn snapped(raw: f32, default: f32, range: (f32, f32)) -> f32 {
  let width = raw.clamp(range.0, range.1);
  if (width - default).abs() <= SNAP {
    default
  } else {
    width
  }
}

/// The widths to draw: as wide as asked, narrower when the window has no room for them and the
/// panes' minimum. The panel gives way first.
pub(super) fn fit_widths(
  viewport: f32,
  (sidebar, sidebar_open): (f32, bool),
  (panel, panel_open): (f32, bool),
) -> (f32, f32) {
  let room = (viewport - MIN_MAIN).max(0.0);
  let beside_panel = if sidebar_open { sidebar } else { 0.0 };
  let panel = if panel_open {
    panel.min((room - beside_panel).max(PANEL_RANGE.0))
  } else {
    panel
  };
  let beside_sidebar = if panel_open { panel } else { 0.0 };
  let sidebar = if sidebar_open {
    sidebar.min((room - beside_sidebar).max(SIDEBAR_RANGE.0))
  } else {
    sidebar
  };
  (sidebar, panel)
}

/// The widest an edge may go: its range's end, less what the window cannot spare.
fn widest(range: (f32, f32), viewport: f32, other_side: f32) -> f32 {
  range.1.min(viewport - MIN_MAIN - other_side).max(range.0)
}

impl Workspace {
  /// How wide an edge is drawn now.
  fn edge_width(&self, edge: Edge) -> f32 {
    match edge {
      Edge::Sidebar => self.sidebar_drawn,
      Edge::Panel => self.panel_drawn,
    }
  }

  fn edge_default_and_range(edge: Edge) -> (f32, (f32, f32)) {
    match edge {
      Edge::Sidebar => (SIDEBAR_WIDTH, SIDEBAR_RANGE),
      Edge::Panel => (PANEL_WIDTH, PANEL_RANGE),
    }
  }

  fn set_edge_width(&mut self, edge: Edge, width: f32) {
    match edge {
      Edge::Sidebar => self.sidebar_width = width,
      Edge::Panel => self.panel_width = width,
    }
  }

  /// A drag in progress moves its edge; true when there was one.
  pub(super) fn drag_edge(
    &mut self,
    event: &MouseMoveEvent,
    viewport: f32,
    cx: &mut Context<Self>,
  ) -> bool {
    let Some(drag) = &self.edge_drag else {
      return false;
    };
    if event.pressed_button != Some(MouseButton::Left) {
      self.edge_drag = None;
      return true;
    }
    let delta = f32::from(event.position.x - drag.start_x);
    // The panel's edge is its left one: dragging left makes it wider.
    let raw = match drag.edge {
      Edge::Sidebar => drag.start_width + delta,
      Edge::Panel => drag.start_width - delta,
    };
    let edge = drag.edge;
    let (default, (least, most)) = Self::edge_default_and_range(edge);
    let other_side = match edge {
      Edge::Sidebar if self.panel.is_some() => self.panel_drawn,
      Edge::Panel if self.sidebar_open => self.sidebar_drawn,
      _ => 0.0,
    };
    let most = widest((least, most), viewport, other_side);
    self.set_edge_width(edge, snapped(raw, default, (least, most)));
    cx.notify();
    true
  }

  /// The strip over an edge that takes the pointer to resize it; a double click resets it. Its
  /// line shows on hover and all through a drag, in another color while the width is the
  /// default one.
  pub(super) fn edge_handle(
    &self,
    edge: Edge,
    cx: &mut Context<Self>,
  ) -> gpui::Stateful<gpui::Div> {
    let (id, group) = match edge {
      Edge::Sidebar => ("edge-sidebar", "edge-sidebar-group"),
      Edge::Panel => ("edge-panel", "edge-panel-group"),
    };
    let dragged = self
      .edge_drag
      .as_ref()
      .is_some_and(|drag| drag.edge == edge);
    let at_default = self.edge_width(edge) == Self::edge_default_and_range(edge).0;
    let line = div()
      .w(px(2.0))
      .h_full()
      .rounded_full()
      .when(dragged, |line| {
        line.bg(rgb(if at_default {
          EDGE_SNAPPED
        } else {
          PANE_BORDER_FOCUSED
        }))
      })
      .group_hover(group, |line| line.bg(rgb(PANE_BORDER_FOCUSED)));
    div()
      .id(id)
      .group(group)
      .absolute()
      .top_0()
      .h_full()
      .w(px(6.0))
      .flex()
      .justify_center()
      .cursor_col_resize()
      .child(line)
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |workspace, event: &MouseDownEvent, _, cx| {
          cx.stop_propagation();
          if event.click_count == 2 {
            workspace.set_edge_width(edge, Self::edge_default_and_range(edge).0);
          } else {
            workspace.edge_drag = Some(EdgeDrag {
              edge,
              start_x: event.position.x,
              start_width: workspace.edge_width(edge),
            });
          }
          cx.notify();
        }),
      )
  }
}
