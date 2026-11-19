//! The sidebar: its rows and folders, the context menu, renaming and reordering.

use super::*;

/// What a row does when opened, with the workspace it acts on.
type OnOpen = Box<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

/// One sidebar line's content, a row's or a folder's: what `sidebar_row` draws.
struct SidebarRow {
  key: SidebarKey,
  icon: AnyElement,
  name: String,
  /// `None` keeps the dot's room empty.
  dot: Option<Indicator>,
  is_active: bool,
  is_live: bool,
  /// Indents the row under its folder's name.
  in_folder: bool,
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

/// The sidebar's lines on screen: top-level rows, folders, and the rows of open folders.
pub(super) fn visible_keys(items: &[SidebarItem]) -> HashSet<SidebarKey> {
  let mut keys = HashSet::new();
  for item in items {
    match item {
      SidebarItem::Row(entry) => keys.extend(row_key(entry)),
      SidebarItem::Folder(folder) => {
        keys.insert(SidebarKey::Folder(folder.id));
        if !folder.collapsed {
          keys.extend(folder.rows.iter().filter_map(row_key));
        }
      }
    }
  }
  keys
}

impl Workspace {
  /// The active tab's row: its first terminal.
  pub(super) fn active_row(&self) -> Option<String> {
    let tab = self.layout.active()?;
    tab.root.leaves().first().map(|first| first.to_string())
  }

  /// The row a terminal is in, at the top level or in a folder.
  pub(super) fn find_row(&self, row: &str) -> Option<Entry> {
    find_row(&sidebar_items(&self.layout, &self.terminals), row).cloned()
  }

  /// Moves a row one step, into and out of folders as it meets their edges; nothing when it
  /// can't move.
  pub(super) fn move_row(&mut self, row: &str, direction: Direction) {
    let items = sidebar_items(&self.layout, &self.terminals);
    if let Some(items) = moved_row(&items, row, direction) {
      self.layout.arrange(&items);
    }
  }

  /// Ends every terminal in the row: a tab's panes, or the one loose terminal.
  pub(super) fn close_row(&mut self, row: &str, cx: &mut Context<Self>) {
    if let Some(entry) = self.find_row(row) {
      for address in entry.terminals() {
        self.close_address(address);
      }
    }
    cx.notify();
  }

  pub(super) fn start_rename(&mut self, row: &str, window: &mut Window, cx: &mut Context<Self>) {
    let Some(entry) = self.find_row(row) else {
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
      && let Some(entry) = self.find_row(&renaming.row)
    {
      self.layout.rename(&entry, &name);
    }
    self.changed(window, cx);
  }

  /// Draws the parts every sidebar line shares. Only a row has a menu, a close button and a name
  /// to edit; a folder's line keeps the close button's room, so the dots line up.
  fn sidebar_row(&self, content: SidebarRow, cx: &mut Context<Self>) -> AnyElement {
    let colors = Colors::active(cx);
    let SidebarRow {
      key,
      icon,
      name,
      dot,
      is_active,
      is_live,
      in_folder,
      on_open,
    } = content;
    let (id, row) = match key {
      SidebarKey::Row(row) => (format!("row-{row}"), Some(row)),
      SidebarKey::Folder(folder) => (format!("folder-{folder}"), None),
    };
    let dim = colors.text_muted;
    let renaming = self
      .renaming
      .as_ref()
      .filter(|renaming| row.as_ref() == Some(&renaming.row));
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
    let close_slot = div()
      .h(px(20.0))
      .min_w(px(20.0))
      .flex()
      .items_center()
      .justify_center()
      .rounded_md()
      .text_color(dim);
    let close = match row.clone() {
      Some(closing) => close_slot
        .id(SharedString::from(format!("close-{closing}")))
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
        }))
        .into_any_element(),
      None => close_slot.into_any_element(),
    };
    sidebar_line()
      .id(SharedString::from(id))
      .group("row")
      .rounded_lg()
      .when(is_active, |line| line.bg(colors.surface_active))
      .hover(|line| line.bg(colors.surface_hover))
      .when_some(on_open, |line, on_open| {
        line.on_click(cx.listener(move |workspace, _, window, cx| on_open(workspace, window, cx)))
      })
      .when_some(row, |line, menu_row| {
        line.on_mouse_down(
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
      })
      .when(in_folder, |line| line.child(indent()))
      .child(icon)
      .child(label)
      .child(
        div()
          .size(px(6.0))
          .flex_none()
          .rounded_full()
          .when_some(dot, |slot, dot| slot.bg(indicator_color(dot, colors))),
      )
      .child(close)
      .into_any_element()
  }

  pub(super) fn render_tab_row(
    &self,
    entry: &Entry,
    index: usize,
    in_folder: bool,
    cx: &mut Context<Self>,
  ) -> AnyElement {
    let colors = Colors::active(cx);
    let members = entry.terminals();
    // The stack's dot is its most urgent pane's.
    let dot = most_urgent(
      members
        .iter()
        .filter_map(|address| self.terminals.get(address))
        .map(indicator),
    );
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
      key: SidebarKey::Row(
        members
          .first()
          .map(|first| first.to_string())
          .unwrap_or_default(),
      ),
      icon,
      name: entry_label(&self.layout, &self.terminals, entry),
      dot: Some(dot),
      is_active: index == self.layout.active_tab,
      is_live: true,
      in_folder,
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
    in_folder: bool,
    cx: &mut Context<Self>,
  ) -> AnyElement {
    let colors = Colors::active(cx);
    let address = terminal.address.clone();
    let is_live = terminal.state == TerminalState::Live;
    let content = SidebarRow {
      key: SidebarKey::Row(address.clone()),
      icon: shell_icon(shell_colors(colors)).into_any_element(),
      name: entry_label(&self.layout, &self.terminals, entry),
      dot: Some(indicator(terminal)),
      is_active: false,
      is_live,
      in_folder,
      on_open: is_live.then(|| -> OnOpen {
        Box::new(move |workspace, window, cx| {
          workspace.layout.show(&address);
          workspace.changed(window, cx);
        })
      }),
    };
    self.sidebar_row(content, cx)
  }

  /// A folder's line; a click opens or collapses it. Collapsed, it stands in for its rows: it
  /// shows their most urgent dot, and the active tab's highlight when that tab is one of them.
  fn render_folder_row(&self, folder: &SidebarFolder, cx: &mut Context<Self>) -> AnyElement {
    let colors = Colors::active(cx);
    let icon = if folder.collapsed {
      ICON_FOLDER
    } else {
      ICON_FOLDER_OPENED
    };
    let dot = folder.collapsed.then(|| {
      most_urgent(
        folder
          .rows
          .iter()
          .flat_map(Entry::terminals)
          .filter_map(|address| self.terminals.get(address))
          .map(indicator),
      )
    });
    let holds_active_tab = folder
      .rows
      .iter()
      .any(|entry| matches!(entry, Entry::Tab { index, .. } if *index == self.layout.active_tab));
    let id = folder.id;
    let content = SidebarRow {
      key: SidebarKey::Folder(id),
      icon: glyph(icon, colors),
      name: folder.name.clone(),
      dot,
      is_active: folder.collapsed && holds_active_tab,
      is_live: true,
      in_folder: false,
      on_open: Some(Box::new(move |workspace, window, cx| {
        workspace.layout.toggle_folder(id);
        workspace.changed(window, cx);
      })),
    };
    self.sidebar_row(content, cx)
  }

  pub(super) fn render_menu(&self, menu: &RowMenu, cx: &mut Context<Self>) -> impl IntoElement {
    let items = sidebar_items(&self.layout, &self.terminals);
    let can_move_up = moved_row(&items, &menu.row, Direction::Up).is_some();
    let can_move_down = moved_row(&items, &menu.row, Direction::Down).is_some();
    let entry = find_row(&items, &menu.row);
    let noun = match entry {
      Some(Entry::Loose(_)) => "Terminal",
      _ => "Tab",
    };
    // The shortcuts act on the active tab, so only that tab's row shows them.
    let is_active_tab = matches!(
      entry,
      Some(Entry::Tab { index, .. }) if *index == self.layout.active_tab
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
            workspace.move_row(&up_row, Direction::Up);
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
            workspace.move_row(&down_row, Direction::Down);
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
    let mut lines = Vec::new();
    for item in &sidebar_items(&self.layout, &self.terminals) {
      match item {
        SidebarItem::Row(entry) => lines.extend(self.render_entry(entry, false, now, cx)),
        SidebarItem::Folder(folder) => {
          let line = self.render_folder_row(folder, cx);
          lines.push(self.grow_in(&SidebarKey::Folder(folder.id), line, now));
          if folder.collapsed {
            continue;
          }
          if folder.rows.is_empty() {
            lines.push(no_sessions(colors));
          }
          for entry in &folder.rows {
            lines.extend(self.render_entry(entry, true, now, cx));
          }
        }
      }
    }
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
          .children(lines)
          .children(templates),
      )
  }

  /// A tab's or a loose terminal's row, growing in while new; `None` for a loose terminal the
  /// core no longer has.
  fn render_entry(
    &self,
    entry: &Entry,
    in_folder: bool,
    now: Instant,
    cx: &mut Context<Self>,
  ) -> Option<AnyElement> {
    let row = match entry {
      Entry::Tab { index, .. } => self.render_tab_row(entry, *index, in_folder, cx),
      Entry::Loose(address) => {
        let terminal = self.terminals.get(address)?.clone();
        self.render_loose_row(entry, &terminal, in_folder, cx)
      }
    };
    Some(self.grow_in(&row_key(entry)?, row, now))
  }

  /// A line new on the sidebar grows to its height and fades in.
  fn grow_in(&self, key: &SidebarKey, line: AnyElement, now: Instant) -> AnyElement {
    let grown = self
      .row_motion
      .get(key)
      .map_or(1.0, |tween| tween.value(now));
    div()
      .flex_none()
      .h(px(ROW_HEIGHT * grown))
      .opacity(grown)
      .overflow_hidden()
      .child(line)
      .into_any_element()
  }
}

/// A sidebar row is known by its first terminal.
fn row_key(entry: &Entry) -> Option<SidebarKey> {
  let first = entry.terminals().first()?.to_string();
  Some(SidebarKey::Row(first))
}

/// The frame of every sidebar line, so their icons and names line up.
fn sidebar_line() -> gpui::Div {
  div()
    .mx_2()
    .px_2()
    .h(px(ROW_HEIGHT))
    .flex()
    .items_center()
    .gap_2()
}

/// An empty icon's room before a nested line's content. With the line's gap after it, the
/// content starts right under its folder's name.
fn indent() -> gpui::Div {
  div().size(px(ICON_BOX)).flex_none()
}

/// What an open folder with no rows shows under its name. It isn't a row, so it has no hover and
/// no click.
fn no_sessions(colors: Colors) -> AnyElement {
  sidebar_line()
    .flex_none()
    .text_color(colors.text_muted)
    .child(indent())
    .child("No sessions")
    .into_any_element()
}

/// The colors of a shell's icon in the sidebar.
fn shell_colors(colors: Colors) -> ShellColors {
  ShellColors {
    background: colors.surface_active,
    text: colors.text_muted,
  }
}
