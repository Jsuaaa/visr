//! Colors of the window's chrome.

pub(crate) const WINDOW_BG: u32 = 0x141516;
pub(super) const ROW_HOVER_BG: u32 = 0x222427;
pub(super) const ROW_ACTIVE_BG: u32 = 0x2a2d31;
/// A button's hover inside a row that is itself hovered or active.
pub(super) const ROW_HOVER_BG_STRONG: u32 = 0x3a3d42;
pub(super) const ICON_BG: u32 = 0x2a2d31;
pub(super) const PANE_BORDER: u32 = 0x2a2c2f;
pub(super) const PANE_BORDER_FOCUSED: u32 = 0x44484d;
pub(super) const MENU_BG: u32 = 0x232528;
pub(super) const MENU_BORDER: u32 = 0x3a3d41;
pub(super) const MENU_HOVER_BG: u32 = 0x34373b;
pub(super) const DIM_TEXT: u32 = 0x808080;
pub(super) const ERROR_BG: u32 = 0x5c2b2b;
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
