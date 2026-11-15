//! Colors and sizes of the window's chrome.

pub(crate) const WINDOW_BG: u32 = 0x141516;
pub(super) const ROW_HOVER_BG: u32 = 0x222427;
pub(super) const ROW_ACTIVE_BG: u32 = 0x2a2d31;
/// A button's hover inside a row that is itself hovered or active.
pub(super) const ROW_HOVER_BG_STRONG: u32 = 0x3a3d42;
pub(super) const ICON_BG: u32 = 0x2a2d31;
pub(super) const PANE_BORDER: u32 = 0x2a2c2f;
pub(super) const PANE_BORDER_FOCUSED: u32 = 0x44484d;
pub(super) const ROW_HEIGHT: f32 = 32.0;
pub(super) const MENU_BG: u32 = 0x232528;
pub(super) const MENU_BORDER: u32 = 0x3a3d41;
pub(super) const MENU_HOVER_BG: u32 = 0x34373b;
/// Room the macOS traffic lights take at the window's top left.
pub(super) const TRAFFIC_LIGHTS_WIDTH: f32 = 72.0;
/// The title bar's buttons are squares this big.
pub(super) const TITLEBAR_BUTTON: f32 = 24.0;
/// The gap between the window's edges and the panes, the same on every side and in every state.
pub(super) const WINDOW_GAP: f32 = 8.0;
/// How far down the middle of a collapsed top left pane header is: what the traffic lights, the
/// sidebar toggle and the sidebar's + all center on.
pub(crate) const TITLE_CENTER: f32 = WINDOW_GAP + PANE_BORDER_WIDTH + PANE_HEADER_HEIGHT / 2.0;
/// How tall the macOS traffic lights are.
const TRAFFIC_LIGHTS_HEIGHT: f32 = 14.0;
/// Where the traffic lights' top is, so that their middle is the title's.
pub(crate) const TRAFFIC_LIGHTS_TOP: f32 = TITLE_CENTER - TRAFFIC_LIGHTS_HEIGHT / 2.0;
/// Pushes the sidebar strip's content down until it is centered on the title's middle.
pub(super) const TITLEBAR_NUDGE: f32 = 2.0 * TITLE_CENTER - crate::TITLEBAR_HEIGHT;
/// Where a pane's card starts, with the sidebar gone: the main area's margin, then its border.
pub(super) const PANE_EDGE: f32 = WINDOW_GAP + PANE_BORDER_WIDTH;
/// Where the sidebar toggle is fixed: past the traffic lights on macOS, else a little inside the
/// pane's edge. Its top centers it on the traffic lights.
pub(super) const TOGGLE_LEFT: f32 = PANE_EDGE
  + if cfg!(target_os = "macos") {
    TRAFFIC_LIGHTS_WIDTH
  } else {
    4.0
  };
pub(super) const TOGGLE_TOP: f32 = TITLE_CENTER - TITLEBAR_BUTTON / 2.0;
/// The gap between the fixed toggle and what follows it.
pub(super) const TOGGLE_GAP: f32 = 8.0;
pub(super) const PANE_RADIUS: f32 = 10.0;
pub(super) const PANE_HEADER_HEIGHT: f32 = 30.0;
/// A pane header's left padding when nothing in the corner needs room.
pub(super) const HEADER_PADDING: f32 = 12.0;
/// The gap between panes, which is also the handle that resizes them.
pub(super) const DIVIDER_WIDTH: f32 = 8.0;
pub(super) const PANE_BORDER_WIDTH: f32 = 1.0;
pub(super) const PANE_PADDING_LEFT: f32 = 8.0;
pub(super) const PANE_PADDING_BOTTOM: f32 = 4.0;
/// What a pane's card takes around its terminal, from the measures above: a new pane's size is
/// worked out with these, so the card must be styled from the same consts.
pub(super) const PANE_CHROME_WIDTH: f32 = 2.0 * PANE_BORDER_WIDTH + PANE_PADDING_LEFT;
pub(super) const PANE_CHROME_HEIGHT: f32 =
  PANE_HEADER_HEIGHT + PANE_PADDING_BOTTOM + 2.0 * PANE_BORDER_WIDTH;
pub(super) const DIM_TEXT: u32 = 0x808080;
pub(super) const ERROR_BG: u32 = 0x5c2b2b;
/// Room before a panel row's content; its highlight reaches both edges of the panel.
pub(super) const ROW_INSET: f32 = 12.0;
/// What each kind of change shows as in the changes panel.
pub(super) const GIT_MODIFIED: u32 = 0xe2c08d;
pub(super) const GIT_ADDED: u32 = 0x81b88b;
pub(super) const GIT_DELETED: u32 = 0xc74e39;
pub(super) const GIT_RENAMED: u32 = 0x73c991;
pub(super) const GIT_UNTRACKED: u32 = 0x73c991;
pub(super) const GIT_CONFLICT: u32 = 0xe4676b;
/// The line of a panel's edge while its width is the default one.
pub(super) const EDGE_SNAPPED: u32 = 0x3b82f6;

/// The setup window's buttons, links, secondary text, log box and warning.
pub(crate) const SETUP_ACCENT: u32 = 0x3b6fd4;
pub(crate) const SETUP_BUTTON_BG: u32 = 0x2a2d31;
pub(crate) const SETUP_LINK: u32 = 0x7aa2f7;
pub(crate) const SETUP_DIM: u32 = 0x909090;
pub(crate) const SETUP_LOG_BG: u32 = 0x1c1e21;
pub(crate) const SETUP_WARNING: u32 = 0xe2a04a;
