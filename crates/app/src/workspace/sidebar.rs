//! The sidebar: its rows, the context menu, renaming and reordering.

use super::*;

/// What a row does when opened, with the workspace it acts on.
type OnOpen = Box<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

/// One sidebar row's content: what `sidebar_row` draws.
struct SidebarRow {
  /// Names the row: its first terminal's address.
  row: String,
  icon: AnyElement,
  name: String,
  dot: Indicator,
  is_active: bool,
  is_live: bool,
  /// What clicking the row does.
  on_open: Option<OnOpen>,
}

pub(super) fn indicator_color(indicator: Indicator, colors: Colors) -> Hsla {
  match indicator {
    Indicator::Quiet => colors.status_quiet,
    Indicator::Working => colors.status_working,
    Indicator::NeedsYou => colors.status_blocked,
    Indicator::Done => colors.status_done,
    Indicator::Finished => colors.status_exited,
  }
}

/// A sidebar row is known by its first terminal.
pub(super) fn row_key(entry: &Entry) -> Option<&str> {
  entry.terminals().first().copied()
}

impl Workspace {
  /// The row a terminal is in, and where it sits.
  pub(super) fn find_row(&self, row: &str) -> Option<(usize, Entry)> {
    sidebar_entries(&self.layout, &self.terminals)
      .into_iter()
      .enumerate()
      .find(|(_, entry)| entry.terminals().contains(&row))
  }

  pub(super) fn move_row(&mut self, row: &str, step: isize) {
    let Some((from, _)) = self.find_row(row) else {
      return;
    };
    let entries = sidebar_entries(&self.layout, &self.terminals);
    if let Some(to) = from.checked_add_signed(step) {
      self.layout.move_entry(&entries, from, to);
    }
  }

  /// Ends every terminal in the row: a tab's panes, or the one loose terminal.
  pub(super) fn close_row(&mut self, row: &str, cx: &mut Context<Self>) {
    if let Some((_, entry)) = self.find_row(row) {
      for address in entry.terminals() {
        self.close_address(address);
      }
    }
    cx.notify();
  }

  pub(super) fn start_rename(&mut self, row: &str, window: &mut Window, cx: &mut Context<Self>) {
    let Some((_, entry)) = self.find_row(row) else {
      return;
    };
    let text = entry_label(&self.layout, &self.terminals, &entry);
    // The sidebar's own font, not the terminal's.
    let family = window.text_style().font_family;
    let colors = Colors::active(cx);
    let field_colors = TextFieldColors {
      text: colors.text,
      selection: colors.selection,
      cursor: colors.text,
    };
    let field = cx.new(|cx| TextField::new(&text, family, field_colors, window, cx));
    let events = cx.subscribe_in(&field, window, |workspace, _, event, window, cx| {
      let name = match event {
        FieldEvent::Submit(name) => Some(name.clone()),
        FieldEvent::Cancel => None,
      };
      workspace.finish_rename(name, window, cx);
    });
    self.renaming = Some(Renaming {
      row: row.to_string(),
      field,
      _events: events,
    });
    cx.notify();
  }

  /// Ends the rename, keeping `name` if there is one, and hands the keyboard back to the pane.
  pub(super) fn finish_rename(
    &mut self,
    name: Option<String>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(renaming) = self.renaming.take() else {
      return;
    };
    if let Some(name) = name
      && let Some((_, entry)) = self.find_row(&renaming.row)
    {
      self.layout.rename(&entry, &name);
    }
    self.changed(window, cx);
  }

  /// Draws the parts every sidebar row shares.
  fn sidebar_row(&self, content: SidebarRow, cx: &mut Context<Self>) -> AnyElement {
    let colors = Colors::active(cx);
    let SidebarRow {
      row,
      icon,
      name,
      dot,
      is_active,
      is_live,
      on_open,
    } = content;
    let dim = colors.text_muted;
    let renaming = self
      .renaming
      .as_ref()
      .filter(|renaming| renaming.row == row);
    let label = match renaming {
      Some(renaming) => {
        let field = renaming.field.clone();
        div()
          .on_mouse_down_out(cx.listener(move |workspace, _, window, cx| {
            let name = field.read(cx).text().to_string();
            workspace.finish_rename(Some(name), window, cx);
          }))
          .flex_1()
          .min_w_0()
          .h(px(22.0))
          .px_1()
          .rounded_sm()
          .border_1()
          .border_color(colors.border_focused)
          .bg(colors.surface)
          .child(renaming.field.clone())
      }
      None => div()
        .flex_1()
        .min_w_0()
        .truncate()
        .when(!is_live, |name| name.text_color(dim))
        .child(name),
    };
    let menu_row = row.clone();
    let closing = row.clone();
    div()
      .id(SharedString::from(format!("row-{row}")))
      .group("row")
      .mx_2()
      .px_2()
      .h(px(ROW_HEIGHT))
      .flex()
      .items_center()
      .gap_2()
      .rounded_lg()
      .when(is_active, |row| row.bg(colors.surface_active))
      .hover(|row| row.bg(colors.surface_hover))
      .when_some(on_open, |row, on_open| {
        row.on_click(cx.listener(move |workspace, _, window, cx| on_open(workspace, window, cx)))
      })
      .on_mouse_down(
        MouseButton::Right,
        cx.listener(move |workspace, event: &MouseDownEvent, _, cx| {
          workspace.menu = Some(RowMenu {
            position: event.position,
            row: menu_row.clone(),
            shown: Tween::new(0.0, 1.0, Instant::now(), motion::duration()),
          });
          cx.notify();
        }),
      )
      .child(icon)
      .child(label)
      .child(
        div()
          .size(px(6.0))
          .flex_none()
          .rounded_full()
          .bg(indicator_color(dot, colors)),
      )
      .child(
        div()
          .id(SharedString::from(format!("close-{row}")))
          .h(px(20.0))
          .min_w(px(20.0))
          .flex()
          .items_center()
          .justify_center()
          .rounded_md()
          .text_color(dim)
          .hover(|button| button.bg(colors.surface_hover_strong))
          // A live row shows its close button on hover only; a finished one always offers rm.
          .when(is_live, |button| {
            button
              .opacity(0.0)
              .group_hover("row", |button| button.opacity(1.0))
              .child(close_icon(dim))
          })
          .when(!is_live, |button| button.px_1().child("rm"))
          .on_click(cx.listener(move |workspace, _, _, cx| {
            cx.stop_propagation();
            workspace.close_row(&closing, cx);
          })),
      )
      .into_any_element()
  }

  pub(super) fn render_tab_row(
    &self,
    entry: &Entry,
    index: usize,
    cx: &mut Context<Self>,
  ) -> AnyElement {
    let colors = Colors::active(cx);
    let members = entry.terminals();
    let dots: Vec<Indicator> = members
      .iter()
      .filter_map(|address| self.terminals.get(address))
      .map(indicator)
      .collect();
    // The stack's dot is its most urgent pane's.
    let dot = [Indicator::NeedsYou, Indicator::Working, Indicator::Done]
      .into_iter()
      .find(|urgent| dots.contains(urgent))
      .unwrap_or(Indicator::Quiet);
    let icon = if members.len() > 1 {
      stack_icon(StackColors {
        shell: shell_colors(colors),
        behind: colors.border_focused,
        outline: colors.surface,
      })
    } else {
      shell_icon(shell_colors(colors)).into_any_element()
    };
    let content = SidebarRow {
      row: members
        .first()
        .map(|first| first.to_string())
        .unwrap_or_default(),
      icon,
      name: entry_label(&self.layout, &self.terminals, entry),
      dot,
      is_active: index == self.layout.active_tab,
      is_live: true,
      on_open: Some(Box::new(move |workspace, window, cx| {
        workspace.layout.select_tab(index);
        workspace.changed(window, cx);
      })),
    };
    self.sidebar_row(content, cx)
  }

  pub(super) fn render_loose_row(
    &self,
    entry: &Entry,
    terminal: &Terminal,
    cx: &mut Context<Self>,
  ) -> AnyElement {
    let colors = Colors::active(cx);
    let address = terminal.address.clone();
    let is_live = terminal.state == TerminalState::Live;
    let content = SidebarRow {
      row: address.clone(),
      icon: shell_icon(shell_colors(colors)).into_any_element(),
      name: entry_label(&self.layout, &self.terminals, entry),
      dot: indicator(terminal),
      is_active: false,
      is_live,
      on_open: is_live.then(|| -> OnOpen {
        Box::new(move |workspace, window, cx| {
          workspace.layout.show(&address);
          workspace.changed(window, cx);
        })
      }),
    };
    self.sidebar_row(content, cx)
  }

  pub(super) fn render_menu(&self, menu: &RowMenu, cx: &mut Context<Self>) -> impl IntoElement {
    let rows = sidebar_entries(&self.layout, &self.terminals);
    let position = rows
      .iter()
      .position(|entry| entry.terminals().contains(&menu.row.as_str()));
    let can_move_up = position.is_some_and(|position| position > 0);
    let can_move_down = position.is_some_and(|position| position + 1 < rows.len());
    let noun = match self.find_row(&menu.row) {
      Some((_, Entry::Loose(_))) => "Terminal",
      _ => "Tab",
    };
    // The shortcuts act on the active tab, so only that tab's row shows them.
    let is_active_tab = matches!(
      self.find_row(&menu.row),
      Some((_, Entry::Tab { index, .. })) if index == self.layout.active_tab
    );
    let shortcut = |action| is_active_tab.then(|| self.shortcut(action)).flatten();
    let rename_row = menu.row.clone();
    let up_row = menu.row.clone();
    let down_row = menu.row.clone();
    let close_row = menu.row.clone();
    let colors = Colors::active(cx);
    let menu_colors = menu_colors(colors);
    div()
      .w(px(220.0))
      .py_1()
      .rounded_lg()
      .border_1()
      .border_color(colors.surface_hover_strong)
      .bg(colors.surface_raised)
      .shadow_lg()
      .text_size(px(13.0))
      .text_color(colors.text)
      .occlude()
      .on_mouse_down_out(cx.listener(|workspace, _, _, cx| {
        workspace.menu = None;
        cx.notify();
      }))
      .child(
        menu_item(
          "rename",
          &format!("Rename {noun}…"),
          None,
          true,
          menu_colors,
        )
        .on_click(cx.listener(move |workspace, _, window, cx| {
          workspace.menu = None;
          workspace.start_rename(&rename_row, window, cx);
        })),
      )
      .child(menu_separator(menu_colors))
      .child(
        menu_item(
          "up",
          &format!("Move {noun} Up"),
          shortcut(Action::MoveTabUp),
          can_move_up,
          menu_colors,
        )
        .when(can_move_up, |item| {
          item.on_click(cx.listener(move |workspace, _, window, cx| {
            workspace.menu = None;
            workspace.move_row(&up_row, -1);
            workspace.changed(window, cx);
          }))
        }),
      )
      .child(
        menu_item(
          "down",
          &format!("Move {noun} Down"),
          shortcut(Action::MoveTabDown),
          can_move_down,
          menu_colors,
        )
        .when(can_move_down, |item| {
          item.on_click(cx.listener(move |workspace, _, window, cx| {
            workspace.menu = None;
            workspace.move_row(&down_row, 1);
            workspace.changed(window, cx);
          }))
        }),
      )
      .child(menu_separator(menu_colors))
      .child(
        menu_item(
          "close",
          &format!("Close {noun}"),
          shortcut(Action::CloseTab),
          true,
          menu_colors,
        )
        .on_click(cx.listener(move |workspace, _, _, cx| {
          workspace.menu = None;
          workspace.close_row(&close_row, cx);
        })),
      )
  }

  pub(super) fn sidebar_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let colors = Colors::active(cx);
    chrome_button("toggle-sidebar", colors)
      .child(sidebar_icon(colors.text_muted))
      .on_click(cx.listener(|workspace, _, _, cx| {
        workspace.toggle_sidebar();
        cx.notify();
      }))
  }

  /// Starts a shell. It belongs to the sidebar and goes with it.
  fn new_shell_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let colors = Colors::active(cx);
    chrome_button("new-shell", colors)
      .child(plus_icon(colors.text_muted))
      .on_click(cx.listener(|workspace, _, window, cx| {
        workspace.run_action(Action::NewTab, window, cx);
      }))
  }

  pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let colors = Colors::active(cx);
    // Leaves room for the macOS traffic lights, which sit over this strip.
    let titlebar = div()
      .h(px(TITLEBAR_HEIGHT))
      .flex_none()
      .flex()
      .items_center()
      .justify_end()
      .pr_2()
      // Leaves the fixed sidebar toggle its room.
      .pl(px(TOGGLE_LEFT + TITLEBAR_BUTTON + TOGGLE_GAP))
      .pt(px(TITLEBAR_NUDGE))
      .on_mouse_down(MouseButton::Left, on_titlebar_mouse_down)
      .child(self.new_shell_button(cx));
    let label = div()
      .px_4()
      .pt_2()
      .pb_1()
      .text_size(px(11.0))
      .text_color(colors.text_muted)
      .child("TERMINALS");
    let templates: Vec<AnyElement> = self
      .templates
      .iter()
      .map(|template| {
        let name = template.name.clone();
        div()
          .id(SharedString::from(format!("template-{name}")))
          .mx_2()
          .px_2()
          .h(px(28.0))
          .flex()
          .items_center()
          .rounded_lg()
          .text_color(colors.text_muted)
          .hover(|button| button.bg(colors.surface_hover))
          .child(format!("+ {name}"))
          .on_click(cx.listener(move |workspace, _, window, cx| {
            if workspace.open_tab(vec![name.clone()], None, cx) {
              workspace.changed(window, cx);
            }
          }))
          .into_any_element()
      })
      .collect();
    let now = Instant::now();
    let rows: Vec<AnyElement> = sidebar_entries(&self.layout, &self.terminals)
      .iter()
      .filter_map(|entry| {
        let row = match entry {
          Entry::Tab { index, .. } => self.render_tab_row(entry, *index, cx),
          Entry::Loose(address) => {
            let terminal = self.terminals.get(address)?.clone();
            self.render_loose_row(entry, &terminal, cx)
          }
        };
        // A new row grows to its height and fades in.
        let grown = row_key(entry)
          .and_then(|key| self.row_motion.get(key))
          .map_or(1.0, |tween| tween.value(now));
        Some(
          div()
            .flex_none()
            .h(px(ROW_HEIGHT * grown))
            .opacity(grown)
            .overflow_hidden()
            .child(row)
            .into_any_element(),
        )
      })
      .collect();
    div()
      .w(px(self.sidebar_drawn))
      .h_full()
      .flex_none()
      .flex()
      .flex_col()
      .text_color(colors.text)
      .text_size(px(13.0))
      .child(titlebar)
      .child(
        div()
          .id("sidebar")
          .flex_1()
          .min_h_0()
          .flex()
          .flex_col()
          .gap_0p5()
          .overflow_y_scroll()
          .child(label)
          .children(rows)
          .children(templates),
      )
  }
}

/// The colors of a shell's icon in the sidebar.
fn shell_colors(colors: Colors) -> ShellColors {
  ShellColors {
    background: colors.surface_active,
    text: colors.text_muted,
  }
}
