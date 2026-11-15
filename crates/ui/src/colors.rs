//! The active theme as GPUI colors: the bridge from `theme` to what components draw with.

use gpui::{App, Global, Hsla, Rgba};

/// Every `[ui]` role of the active theme, named as in the theme file. Set once before the first
/// frame with [`install`] and read with [`Colors::active`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
  pub surface: Hsla,
  pub surface_hover: Hsla,
  pub surface_active: Hsla,
  pub surface_hover_strong: Hsla,
  pub surface_raised: Hsla,
  pub surface_raised_hover: Hsla,
  pub surface_sunken: Hsla,
  pub surface_error: Hsla,
  pub border: Hsla,
  pub border_focused: Hsla,
  pub border_snapped: Hsla,
  pub text: Hsla,
  pub text_muted: Hsla,
  pub text_secondary: Hsla,
  pub text_link: Hsla,
  pub text_warning: Hsla,
  pub selection: Hsla,
  pub button_primary: Hsla,
  pub git_modified: Hsla,
  pub git_added: Hsla,
  pub git_deleted: Hsla,
  pub git_untracked: Hsla,
  pub git_conflict: Hsla,
  pub status_quiet: Hsla,
  pub status_working: Hsla,
  pub status_blocked: Hsla,
  pub status_done: Hsla,
  pub status_exited: Hsla,
}

impl Global for Colors {}

impl Colors {
  /// The installed colors. Panics if [`install`] has not run, which `main` does before any window.
  pub fn active(cx: &App) -> Self {
    *cx.global::<Self>()
  }

  fn from_theme(ui: &theme::UiColors) -> Self {
    Self {
      surface: hsla(ui.surface),
      surface_hover: hsla(ui.surface_hover),
      surface_active: hsla(ui.surface_active),
      surface_hover_strong: hsla(ui.surface_hover_strong),
      surface_raised: hsla(ui.surface_raised),
      surface_raised_hover: hsla(ui.surface_raised_hover),
      surface_sunken: hsla(ui.surface_sunken),
      surface_error: hsla(ui.surface_error),
      border: hsla(ui.border),
      border_focused: hsla(ui.border_focused),
      border_snapped: hsla(ui.border_snapped),
      text: hsla(ui.text),
      text_muted: hsla(ui.text_muted),
      text_secondary: hsla(ui.text_secondary),
      text_link: hsla(ui.text_link),
      text_warning: hsla(ui.text_warning),
      selection: hsla(ui.selection),
      button_primary: hsla(ui.button_primary),
      git_modified: hsla(ui.git_modified),
      git_added: hsla(ui.git_added),
      git_deleted: hsla(ui.git_deleted),
      git_untracked: hsla(ui.git_untracked),
      git_conflict: hsla(ui.git_conflict),
      status_quiet: hsla(ui.status_quiet),
      status_working: hsla(ui.status_working),
      status_blocked: hsla(ui.status_blocked),
      status_done: hsla(ui.status_done),
      status_exited: hsla(ui.status_exited),
    }
  }
}

/// Makes `ui` the colors every component draws with. Call it before opening the first window.
pub fn install(ui: &theme::UiColors, cx: &mut App) {
  cx.set_global(Colors::from_theme(ui));
}

/// A theme color as a GPUI color.
fn hsla(color: theme::Rgba) -> Hsla {
  Rgba {
    r: f32::from(color.r) / 255.0,
    g: f32::from(color.g) / 255.0,
    b: f32::from(color.b) / 255.0,
    a: f32::from(color.a) / 255.0,
  }
  .into()
}
