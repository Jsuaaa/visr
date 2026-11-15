//! The parts every context menu shares.

use super::*;

pub(super) fn menu_separator() -> gpui::Div {
  div().h(px(1.0)).mx_2().my_1().bg(rgb(MENU_BORDER))
}

fn menu_row(id: &str, enabled: bool) -> gpui::Stateful<gpui::Div> {
  div()
    .id(SharedString::from(format!("menu-{id}")))
    .mx_1()
    .px_2()
    .h(px(26.0))
    .flex()
    .items_center()
    .justify_between()
    .gap_4()
    .rounded_md()
    .when(enabled, |item| {
      item.hover(|item| item.bg(rgb(MENU_HOVER_BG)))
    })
    .when(!enabled, |item| item.text_color(rgb(DIM_TEXT)))
}

pub(super) fn menu_item(
  id: &str,
  label: &str,
  shortcut: Option<String>,
  enabled: bool,
) -> gpui::Stateful<gpui::Div> {
  menu_row(id, enabled)
    .child(label.to_string())
    .children(shortcut.map(|shortcut| div().text_color(rgb(DIM_TEXT)).child(shortcut)))
}

/// An item with a 16px icon before its label.
pub(super) fn menu_item_icon(
  id: &str,
  icon: AnyElement,
  label: &str,
  enabled: bool,
) -> gpui::Stateful<gpui::Div> {
  menu_row(id, enabled).child(
    div()
      .flex()
      .items_center()
      .gap_2()
      .child(
        div()
          .size(px(16.0))
          .flex_none()
          .flex()
          .items_center()
          .justify_center()
          .child(icon),
      )
      .child(label.to_string()),
  )
}
