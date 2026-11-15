//! Icons drawn from boxes, strokes and glyphs, so they center like each other.

use super::*;

/// The bundled Nerd Font the glyph icons come from.
pub(super) const ICON_FONT: &str = "Lilex Nerd Font Mono";
/// Material Design icons: diagonal arrows out to expand a pane, in to put it back.
pub(super) const ICON_EXPAND: &str = "\u{f0616}";
pub(super) const ICON_RESTORE: &str = "\u{f0615}";
/// Each with the size that makes it look as big as the others: the glyphs fill their boxes
/// differently.
pub(super) const ICON_FILES: (&str, f32) = ("\u{ea83}", 19.0);
pub(super) const ICON_SOURCE_CONTROL: (&str, f32) = ("\u{ea68}", 15.0);
pub(super) const ICON_ELLIPSIS: (&str, f32) = ("\u{ea7c}", 19.0);
pub(super) const ICON_FOLDER_OPENED: (&str, f32) = ("\u{eaf7}", 20.0);
pub(super) const ICON_TERMINAL: (&str, f32) = ("\u{ea85}", 20.0);
/// For an editor without a logo of its own.
pub(super) const ICON_CODE: (&str, f32) = ("\u{eac4}", 20.0);

/// A codicon centered in a box of one size, so every toolbar icon sits in the same place.
pub(super) fn glyph_icon((glyph, size): (&'static str, f32)) -> impl IntoElement {
  div()
    .size(px(18.0))
    .overflow_hidden()
    .flex()
    .items_center()
    .justify_center()
    .font_family(ICON_FONT)
    .text_size(px(size))
    .child(glyph)
}

/// A small window outline cut in two along `axis`, drawn with boxes so it needs no icon font.
pub(super) fn split_icon(axis: Axis, color: Hsla) -> AnyElement {
  let line = div().bg(color);
  let line = match axis {
    Axis::Horizontal => line.w(px(1.0)).h_full(),
    Axis::Vertical => line.h(px(1.0)).w_full(),
  };
  div()
    .w(px(13.0))
    .h(px(11.0))
    .border_1()
    .border_color(color)
    .rounded_sm()
    .flex()
    .when(axis == Axis::Vertical, |icon| icon.flex_col())
    .justify_center()
    .items_center()
    .child(line)
    .into_any_element()
}

/// A window with its left panel marked off: the sidebar.
pub(super) fn sidebar_icon(color: Hsla) -> impl IntoElement {
  div()
    .w(px(15.0))
    .h(px(12.0))
    .flex_none()
    .border_1()
    .border_color(color)
    .rounded_sm()
    .flex()
    .child(div().w(px(4.0)).h_full().border_r_1().border_color(color))
}

pub(super) fn titlebar_button(id: &'static str) -> gpui::Stateful<gpui::Div> {
  div()
    .id(id)
    .size(px(TITLEBAR_BUTTON))
    .flex()
    .items_center()
    .justify_center()
    .rounded_md()
    .cursor_pointer()
    .text_color(rgb(DIM_TEXT))
    .hover(|button| button.bg(rgb(ROW_HOVER_BG)))
    // A quick second click is the button's, not a double click on the title bar.
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// An X drawn as two strokes: a typed × sits below the middle in most fonts.
pub(super) fn close_icon(color: Hsla) -> impl IntoElement {
  let size = 9.0;
  canvas(
    |_, _, _| {},
    move |bounds, _, window, _| {
      let mut path = gpui::PathBuilder::stroke(px(1.25));
      let (left, top) = (bounds.origin.x, bounds.origin.y);
      let (right, bottom) = (left + px(size), top + px(size));
      path.move_to(point(left, top));
      path.line_to(point(right, bottom));
      path.move_to(point(right, top));
      path.line_to(point(left, bottom));
      if let Ok(path) = path.build() {
        window.paint_path(path, color);
      }
    },
  )
  .size(px(size))
  .flex_none()
}

/// A double click on the title bar does what the system says: zoom the window by default.
pub(super) fn on_titlebar_mouse_down(event: &MouseDownEvent, window: &mut Window, _cx: &mut App) {
  if event.click_count == 2 {
    window.titlebar_double_click();
  }
}

/// Drawn rather than typed, so it centers exactly like the other icons.
pub(super) fn plus_icon(color: Hsla) -> impl IntoElement {
  let (size, stroke) = (12.0, 1.5);
  let middle = (size - stroke) / 2.0;
  div()
    .relative()
    .size(px(size))
    .child(
      div()
        .absolute()
        .left_0()
        .top(px(middle))
        .w_full()
        .h(px(stroke))
        .bg(color),
    )
    .child(
      div()
        .absolute()
        .top_0()
        .left(px(middle))
        .h_full()
        .w(px(stroke))
        .bg(color),
    )
}

pub(super) fn shell_icon() -> impl IntoElement {
  div()
    .size(px(18.0))
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .rounded_sm()
    .bg(rgb(ICON_BG))
    .text_size(px(10.0))
    .text_color(rgb(DIM_TEXT))
    .child(">_")
}

/// Two shells, one behind the other: a tab split into several panes.
pub(super) fn stack_icon() -> AnyElement {
  let layer = || {
    div()
      .absolute()
      .size(px(15.0))
      .rounded_sm()
      .border_1()
      .border_color(rgb(WINDOW_BG))
  };
  div()
    .relative()
    .size(px(18.0))
    .flex_none()
    .child(layer().top_0().right_0().bg(rgb(PANE_BORDER_FOCUSED)))
    .child(
      layer()
        .bottom_0()
        .left_0()
        .bg(rgb(ICON_BG))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(9.0))
        .text_color(rgb(DIM_TEXT))
        .child(">_"),
    )
    .into_any_element()
}
