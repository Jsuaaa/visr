//! One terminal on screen: draws its grid, and turns keys, mouse and paste into input for it.

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
  App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, EventEmitter,
  FocusHandle, Font, FontStyle, FontWeight, Global, Hsla, KeyDownEvent, Modifiers, MouseButton,
  MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Rgba, ScrollWheelEvent, StyledText,
  Subscription, Task, TextRun, UTF16Selection, UnderlineStyle, Window, canvas, div, fill, font,
  point, prelude::*, px, size,
};
use hc_client::{Attachment, Client};
use hc_proto::types::AttachParams;

use crate::SIDEBAR_WIDTH;
use crate::input::{self, Wheel};
use config::Config;
use term::keys::{Key, Mods, encode, paste};
use term::mouse::{self, Button, Kind, MouseEvent};
use term::palette::Palette;
use term::render::{Rgb, row_blocks, row_runs};
use term::screen::{SCROLLBACK_LINES, Screen};

const RESIZE_SETTLE: Duration = Duration::from_millis(60);
const BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// A terminal color as an opaque GPUI color.
pub(crate) fn hsla(color: Rgb) -> Hsla {
  Rgba {
    r: color.r as f32 / 255.0,
    g: color.g as f32 / 255.0,
    b: color.b as f32 / 255.0,
    a: 1.0,
  }
  .into()
}

/// The terminal colors of the active theme, as the palette every terminal draws with.
pub(crate) struct TerminalColors(Palette);

impl Global for TerminalColors {}

impl TerminalColors {
  pub(crate) fn new(colors: &theme::TerminalColors) -> Self {
    let rgb = |color: theme::Rgba| Rgb {
      r: color.r,
      g: color.g,
      b: color.b,
    };
    Self(Palette {
      ansi: colors.ansi.map(rgb),
      foreground: rgb(colors.foreground),
      background: rgb(colors.background),
      cursor: rgb(colors.cursor),
      selection: rgb(colors.selection),
    })
  }

  /// The installed palette.
  pub(crate) fn palette(cx: &App) -> Palette {
    cx.global::<Self>().0.clone()
  }
}

/// The pane was clicked: the workspace makes it the focused one.
pub(crate) struct PaneFocused;

/// The terminal printed something: the workspace may look again at the folder it is in.
pub(crate) struct Output;

/// One terminal drawn in a pane: its screen, input, selection and the stream to the core.
pub(crate) struct TerminalView {
  attachment: Arc<Attachment>,
  /// Where the view was last laid out; the grid follows its size.
  bounds: Bounds<Pixels>,
  /// The core closed the stream: the program ended or was lost.
  ended: bool,
  screen: Screen,
  palette: Palette,
  font: Font,
  font_size: Pixels,
  line_height: Pixels,
  cell_width: Pixels,
  focus_handle: FocusHandle,
  wheel: Wheel,
  selecting: bool,
  /// A button held down that the program is being told about, and the cell it was last over.
  reporting: Option<(Button, (usize, usize))>,
  blink_on: bool,
  /// IME composition in progress (a dead key's accent, a CJK candidate).
  marked_text: Option<String>,
  /// Tells the core the new size once the pane stops changing.
  resize_task: Option<Task<()>>,
  /// The layout is animating: the grid keeps its size, clipped, until the pane settles.
  frozen: bool,
  _tasks: Vec<Task<()>>,
  _subscriptions: Vec<Subscription>,
}

impl TerminalView {
  /// Attaches at `size`, or at a guess from the window when the terminal's own size is not known;
  /// the pane fixes it once laid out. A terminal just started keeps the size it was started at:
  /// attaching wider would resize its shell, and its first prompt would be redrawn narrower.
  /// Fails when the terminal is gone or finished by the time it's shown.
  pub(crate) fn attach(
    client: &Client,
    address: String,
    size: Option<(u16, u16)>,
    config: &Config,
    window: &Window,
  ) -> Result<Attachment, hc_client::Error> {
    let (cols, rows) = size.unwrap_or_else(|| {
      let font = font(config.font.family.clone());
      let cell_width = cell_width(&font, px(config.font.size), window);
      grid_size(window, cell_width, px(config.font.line_height))
    });
    let params = AttachParams {
      terminal: address,
      cols,
      rows,
      history: Some(SCROLLBACK_LINES as u32),
      ..Default::default()
    };
    client.attach(&params)
  }

  pub(crate) fn new(
    attachment: Attachment,
    config: &Config,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Self {
    let font = font(config.font.family.clone());
    let font_size = px(config.font.size);
    let line_height = px(config.font.line_height);
    let cell_width = cell_width(&font, font_size, window);
    // The terminal as attached: the core sized it to the guess, so the screen starts in step.
    let (cols, rows) = (attachment.info.terminal.cols, attachment.info.terminal.rows);
    let attachment = Arc::new(attachment);
    let reader = Arc::clone(&attachment);
    let reader_task = cx.spawn(async move |this, cx| {
      loop {
        // Awaiting costs no thread while the pane is quiet, however many panes there are.
        let Ok(Some(bytes)) = reader.recv_async().await else {
          break;
        };
        let fed = this.update(cx, |view, cx| {
          view.screen.feed(&bytes);
          cx.emit(Output);
          cx.notify();
        });
        if fed.is_err() {
          break;
        }
      }
      let _ = this.update(cx, |view, cx| {
        view.ended = true;
        cx.notify();
      });
    });
    let window_handle = window.window_handle();
    let blink_task = cx.spawn(async move |this, cx| {
      loop {
        cx.background_executor().timer(BLINK_INTERVAL).await;
        // Only the pane with the keyboard blinks: a redraw per half second for every pane, hidden
        // tabs included, would keep the whole window busy while idle.
        let blinked = cx.update_window(window_handle, |_, window, cx| {
          this.update(cx, |view, cx| {
            if view.focus_handle.is_focused(window) && window.is_window_active() {
              view.blink_on = !view.blink_on;
              cx.notify();
            }
          })
        });
        if blinked.is_err() {
          break;
        }
      }
    });

    let focus_handle = cx.focus_handle();
    let subscriptions = vec![
      cx.observe_window_activation(window, |view, window, cx| {
        let focused = window.is_window_active() && view.focus_handle.is_focused(window);
        let _ = view.attachment.focus(focused);
        cx.notify();
      }),
      cx.on_focus(&focus_handle, window, |view, _, cx| {
        let _ = view.attachment.focus(true);
        cx.notify();
      }),
      cx.on_blur(&focus_handle, window, |view, _, cx| {
        let _ = view.attachment.focus(false);
        cx.notify();
      }),
    ];

    Self {
      attachment,
      bounds: Bounds::default(),
      ended: false,
      screen: Screen::new(cols, rows),
      palette: TerminalColors::palette(cx),
      font,
      font_size,
      line_height,
      cell_width,
      focus_handle,
      wheel: Wheel::default(),
      selecting: false,
      reporting: None,
      blink_on: true,
      marked_text: None,
      resize_task: None,
      frozen: false,
      _tasks: vec![reader_task, blink_task],
      _subscriptions: subscriptions,
    }
  }

  /// A new config: colors now, and the grid follows the new cell size on the next layout.
  pub(crate) fn set_config(
    &mut self,
    config: &Config,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    self.font = font(config.font.family.clone());
    self.font_size = px(config.font.size);
    self.line_height = px(config.font.line_height);
    self.cell_width = cell_width(&self.font, self.font_size, window);
    cx.notify();
  }

  pub(crate) fn copy(&self, cx: &mut Context<Self>) {
    if let Some(text) = self.screen.selected_text() {
      cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
  }

  pub(crate) fn paste(&mut self, cx: &mut Context<Self>) {
    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
      let bytes = paste(&text, self.screen.bracketed_paste());
      self.send(&bytes, cx);
    }
  }

  pub(crate) fn focus(&self, window: &mut Window) {
    self.focus_handle.focus(window);
  }

  pub(crate) fn cell_size(&self) -> (f32, f32) {
    (self.cell_width / px(1.0), self.line_height / px(1.0))
  }

  /// Columns and rows the pane shows.
  pub(crate) fn size(&self) -> (u16, u16) {
    self.screen.size()
  }

  pub(crate) fn set_frozen(&mut self, frozen: bool) {
    self.frozen = frozen;
  }

  /// Called on every layout; the grid changes only when the size in cells does.
  fn fit(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
    self.bounds = bounds;
    if self.frozen {
      return;
    }
    let cols = (bounds.size.width / self.cell_width).floor().max(1.0) as u16;
    let rows = (bounds.size.height / self.line_height).floor().max(1.0) as u16;
    if (cols, rows) != self.screen.size() {
      self.screen.resize(cols, rows);
      // While a pane animates its size changes every frame; the program redraws on each resize,
      // so it only hears about the size it settles at. Replacing the task cancels the last one.
      self.resize_task = Some(cx.spawn(async move |this, cx| {
        cx.background_executor().timer(RESIZE_SETTLE).await;
        let _ = this.update(cx, |view, _| {
          let (cols, rows) = view.screen.size();
          let _ = view.attachment.resize(cols, rows);
        });
      }));
      cx.notify();
    }
  }

  /// Everything typed or pasted goes through here: back to the live screen, cursor shown.
  fn send(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
    let _ = self.attachment.send_input(bytes);
    self.screen.scroll_to_bottom();
    self.screen.clear_selection();
    self.blink_on = true;
    cx.notify();
  }

  fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
    if let Some(bytes) = input::keystroke_bytes(&event.keystroke, self.screen.app_cursor()) {
      self.send(&bytes, cx);
      cx.stop_propagation();
    }
  }

  fn cell_under(&self, position: Point<Pixels>) -> (usize, usize) {
    let cell_width = self.cell_width / px(1.0);
    input::cell_at(
      (position.x - self.bounds.origin.x) / px(1.0),
      (position.y - self.bounds.origin.y) / px(1.0),
      cell_width,
      self.line_height / px(1.0),
      self.screen.size(),
    )
  }

  /// Tells the program about the mouse, if it asked. Shift keeps the mouse for selecting text,
  /// as other terminals do.
  fn report(
    &self,
    button: Button,
    kind: Kind,
    position: Point<Pixels>,
    modifiers: Modifiers,
  ) -> bool {
    let Some(mode) = self.screen.mouse() else {
      return false;
    };
    if modifiers.shift {
      return false;
    }
    let (row, col) = self.cell_under(position);
    let event = MouseEvent {
      button,
      kind,
      col,
      row,
      mods: Mods {
        ctrl: modifiers.control,
        alt: modifiers.alt,
        shift: false,
      },
    };
    let _ = self.attachment.send_input(&mouse::encode(event, mode.sgr));
    true
  }

  fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    self.focus_handle.focus(window);
    cx.emit(PaneFocused);
    let button = match event.button {
      MouseButton::Left => Button::Left,
      MouseButton::Middle => Button::Middle,
      MouseButton::Right => Button::Right,
      _ => return,
    };
    let cell = self.cell_under(event.position);
    if self.report(button, Kind::Press, event.position, event.modifiers) {
      self.reporting = Some((button, cell));
      return;
    }
    if button == Button::Left {
      self.screen.select_start(cell.0, cell.1);
      self.selecting = true;
      cx.notify();
    }
  }

  fn on_mouse_move(
    &mut self,
    event: &MouseMoveEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if let Some((button, last)) = self.reporting {
      let cell = self.cell_under(event.position);
      let drags = self.screen.mouse().is_some_and(|mode| mode.drags);
      if drags && event.pressed_button.is_some() && cell != last {
        self.report(button, Kind::Drag, event.position, event.modifiers);
        self.reporting = Some((button, cell));
      }
      return;
    }
    if self.selecting && event.pressed_button == Some(MouseButton::Left) {
      let (row, col) = self.cell_under(event.position);
      self.screen.select_to(row, col);
      cx.notify();
    }
  }

  fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
    if let Some((button, _)) = self.reporting.take() {
      self.report(button, Kind::Release, event.position, event.modifiers);
      return;
    }
    self.selecting = false;
    if self.screen.selected_text().is_none() {
      self.screen.clear_selection();
    }
    cx.notify();
  }

  fn on_scroll_wheel(
    &mut self,
    event: &ScrollWheelEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let delta = event.delta.pixel_delta(self.line_height).y / px(1.0);
    let lines = self.wheel.lines(delta, self.line_height / px(1.0));
    if lines == 0 {
      return;
    }
    let button = if lines > 0 {
      Button::WheelUp
    } else {
      Button::WheelDown
    };
    if self.screen.mouse().is_some() && !event.modifiers.shift {
      for _ in 0..lines.unsigned_abs() {
        self.report(button, Kind::Press, event.position, event.modifiers);
      }
    } else if self.screen.alternate_scroll() {
      let key = if lines > 0 { Key::Up } else { Key::Down };
      let bytes = encode(key, Mods::default(), self.screen.app_cursor())
        .repeat(lines.unsigned_abs() as usize);
      let _ = self.attachment.send_input(&bytes);
    } else {
      self.screen.scroll(lines);
    }
    cx.notify();
  }

  fn text_runs(&self, runs: &[term::render::Run]) -> Vec<TextRun> {
    runs
      .iter()
      .map(|run| {
        let fg = hsla(run.fg);
        TextRun {
          len: run.len,
          font: Font {
            weight: if run.bold {
              FontWeight::BOLD
            } else {
              FontWeight::NORMAL
            },
            style: if run.italic {
              FontStyle::Italic
            } else {
              FontStyle::Normal
            },
            ..self.font.clone()
          },
          color: fg,
          background_color: run.bg.map(hsla),
          underline: run.underline.then_some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(fg),
            wavy: false,
          }),
          strikethrough: None,
        }
      })
      .collect()
  }
}

/// A terminal has no editable text of its own: committed text is typed into the program and
/// composition is only held until the IME commits it.
impl EntityInputHandler for TerminalView {
  fn text_for_range(
    &mut self,
    _range: Range<usize>,
    _adjusted_range: &mut Option<Range<usize>>,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<String> {
    None
  }

  fn selected_text_range(
    &mut self,
    _ignore_disabled_input: bool,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<UTF16Selection> {
    Some(UTF16Selection {
      range: 0..0,
      reversed: false,
    })
  }

  fn marked_text_range(
    &self,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<Range<usize>> {
    self
      .marked_text
      .as_ref()
      .map(|text| 0..text.encode_utf16().count())
  }

  fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
    self.marked_text = None;
  }

  fn replace_text_in_range(
    &mut self,
    _range: Option<Range<usize>>,
    text: &str,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    self.marked_text = None;
    self.send(text.as_bytes(), cx);
  }

  fn replace_and_mark_text_in_range(
    &mut self,
    _range: Option<Range<usize>>,
    new_text: &str,
    _new_selected_range: Option<Range<usize>>,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    self.marked_text = (!new_text.is_empty()).then(|| new_text.to_string());
    cx.notify();
  }

  /// Puts the IME candidate window at the cursor.
  fn bounds_for_range(
    &mut self,
    _range_utf16: Range<usize>,
    element_bounds: Bounds<Pixels>,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<Bounds<Pixels>> {
    let (row, col) = self.screen.cursor_in_view()?;
    let origin =
      element_bounds.origin + point(self.cell_width * col as f32, self.line_height * row as f32);
    Some(Bounds::new(origin, size(self.cell_width, self.line_height)))
  }

  fn character_index_for_point(
    &mut self,
    _point: Point<Pixels>,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<usize> {
    None
  }
}

/// Snaps a quad outward to whole device pixels, so it meets its neighbors and the text
/// backgrounds beside it with no seam.
fn snapped(bounds: Bounds<Pixels>, scale: f32) -> Bounds<Pixels> {
  let down = |value: Pixels| (value * scale).floor() / scale;
  let up = |value: Pixels| (value * scale).ceil() / scale;
  let top_left = point(down(bounds.origin.x), down(bounds.origin.y));
  let bottom_right = bounds.bottom_right();
  Bounds::from_corners(top_left, point(up(bottom_right.x), up(bottom_right.y)))
}

fn cell_width(font: &Font, font_size: Pixels, window: &Window) -> Pixels {
  let font_id = window.text_system().resolve_font(font);
  window
    .text_system()
    .advance(font_id, font_size, 'm')
    .map(|advance| advance.width)
    .unwrap_or(px(8.0))
}

/// A first guess at the grid before the pane is laid out: the window minus the sidebar at its
/// default width (a dragged width or an open panel are not known here).
fn grid_size(window: &Window, cell_width: Pixels, line_height: Pixels) -> (u16, u16) {
  let viewport = window.viewport_size();
  let width = viewport.width - px(SIDEBAR_WIDTH);
  let cols = (width / cell_width).floor().max(1.0) as u16;
  let rows = (viewport.height / line_height).floor().max(1.0) as u16;
  (cols, rows)
}

impl Render for TerminalView {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let show_cursor = !self.ended
      && window.is_window_active()
      && self.focus_handle.is_focused(window)
      && self.blink_on
      && self.marked_text.is_none();
    let (_, rows) = self.screen.size();
    let lines = (0..rows as usize).map(|row| {
      let (text, runs) = row_runs(&self.screen, row, &self.palette, show_cursor);
      div()
        .h(self.line_height)
        .child(StyledText::new(text).with_runs(self.text_runs(&runs)))
    });
    let blocks: Vec<(usize, term::render::Block)> = (0..rows as usize)
      .flat_map(|row| {
        row_blocks(&self.screen, row, &self.palette, show_cursor)
          .into_iter()
          .map(move |block| (row, block))
      })
      .collect();
    let (cell_width, line_height) = (self.cell_width, self.line_height);
    // Block elements as quads over the text, edge to edge, snapped so rows and cells meet.
    let block_painter = canvas(
      |_, _, _| {},
      move |bounds, _, window, _| {
        let scale = window.scale_factor();
        for (row, block) in &blocks {
          let cell =
            bounds.origin + point(cell_width * block.col as f32, line_height * *row as f32);
          for rect in block.rects {
            let top_left = cell + point(cell_width * rect.x, line_height * rect.y);
            let bottom_right = cell
              + point(
                cell_width * (rect.x + rect.w),
                line_height * (rect.y + rect.h),
              );
            let quad = snapped(Bounds::from_corners(top_left, bottom_right), scale);
            window.paint_quad(fill(quad, hsla(block.color)));
          }
        }
      },
    )
    .absolute()
    // Pinned to the top: an absolute element with no inset sits after the rows, out of view.
    .top_0()
    .left_0()
    .size_full();
    // Text the input method is still composing, like a dead key's accent, shown underlined where
    // it will land.
    let preedit = self.marked_text.as_ref().and_then(|text| {
      let (row, col) = self.screen.cursor_in_view()?;
      let color = hsla(self.palette.foreground);
      let run = TextRun {
        len: text.len(),
        font: self.font.clone(),
        color,
        background_color: None,
        underline: Some(UnderlineStyle {
          thickness: px(1.0),
          color: Some(color),
          wavy: false,
        }),
        strikethrough: None,
      };
      Some(
        div()
          .absolute()
          .left(self.cell_width * col as f32)
          .top(self.line_height * row as f32)
          .h(self.line_height)
          .bg(hsla(self.palette.background))
          .child(StyledText::new(text.clone()).with_runs(vec![run])),
      )
    });
    let entity = cx.entity();
    let measured = entity.clone();
    let focus_handle = self.focus_handle.clone();
    // Measures the pane, and registers it as the window's text input so IME and dead keys reach it.
    let input_handler = canvas(
      move |bounds, _, cx| measured.update(cx, |view, cx| view.fit(bounds, cx)),
      move |bounds, _, window, cx| {
        window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity), cx);
      },
    )
    .absolute()
    .size_full();
    div()
      .track_focus(&self.focus_handle)
      .on_key_down(cx.listener(Self::on_key_down))
      .on_any_mouse_down(cx.listener(Self::on_mouse_down))
      .on_mouse_move(cx.listener(Self::on_mouse_move))
      .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
      .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
      .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
      .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
      .relative()
      .size_full()
      .overflow_hidden()
      .whitespace_nowrap()
      .bg(hsla(self.palette.background))
      .font_family(self.font.family.clone())
      .text_size(self.font_size)
      .line_height(self.line_height)
      .child(input_handler)
      .children(lines)
      .child(block_painter)
      .children(preedit)
  }
}

impl Drop for TerminalView {
  /// Ends the stream so the core stops sending and the blocked reader wakes up.
  fn drop(&mut self) {
    let _ = self.attachment.close();
  }
}

impl EventEmitter<PaneFocused> for TerminalView {}
impl EventEmitter<Output> for TerminalView {}
