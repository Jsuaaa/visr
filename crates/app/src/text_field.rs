//! A one-line text input: selection, clipboard, word and line moves, mouse, and the IME (so dead
//! keys and accents work), drawn in one canvas.

use std::ops::Range;

use gpui::{
  Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, EventEmitter,
  FocusHandle, Hsla, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point,
  ShapedLine, SharedString, TextRun, UTF16Selection, UnderlineStyle, Window, canvas, div, fill,
  font, point, prelude::*, px, size,
};

use crate::edit::EditBuffer;

/// What a text field tells its owner.
pub(crate) enum FieldEvent {
  Submit(String),
  Cancel,
}

/// A single-line text input with selection, mouse and IME support.
pub(crate) struct TextField {
  buffer: EditBuffer,
  focus: FocusHandle,
  family: SharedString,
  text_color: Hsla,
  selection_color: Hsla,
  cursor_color: Hsla,
  /// The last shaped line, where it was painted, and how far it scrolled left to keep the cursor
  /// in view: what the mouse and the IME measure against.
  layout: Option<(ShapedLine, Bounds<Pixels>, Pixels)>,
  selecting: bool,
}

const FONT_SIZE: Pixels = px(13.0);

impl TextField {
  pub(crate) fn new(
    text: &str,
    family: SharedString,
    colors: [Hsla; 3],
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Self {
    let focus = cx.focus_handle();
    window.focus(&focus);
    let [text_color, selection_color, cursor_color] = colors;
    Self {
      buffer: EditBuffer::new(text),
      focus,
      family,
      text_color,
      selection_color,
      cursor_color,
      layout: None,
      selecting: false,
    }
  }

  pub(crate) fn text(&self) -> &str {
    self.buffer.text()
  }

  fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
    let keystroke = &event.keystroke;
    let modifiers = &keystroke.modifiers;
    // Shortcuts take cmd on macOS and ctrl elsewhere. Words move with alt on macOS and ctrl
    // elsewhere; only macOS jumps a whole line, with cmd.
    let command = modifiers.secondary();
    let mac = cfg!(target_os = "macos");
    let word = if mac {
      modifiers.alt
    } else {
      modifiers.control
    };
    let line = mac && modifiers.platform;
    let extend = modifiers.shift;
    let buffer = &mut self.buffer;
    match keystroke.key.as_str() {
      "enter" => cx.emit(FieldEvent::Submit(buffer.text().to_string())),
      "escape" => cx.emit(FieldEvent::Cancel),
      "backspace" if line => buffer.delete_to_start(),
      "backspace" if word => buffer.delete_word_back(),
      "backspace" => buffer.backspace(),
      "delete" => buffer.delete(),
      "left" if line => buffer.move_home(extend),
      "right" if line => buffer.move_end(extend),
      "left" if word => buffer.move_word_left(extend),
      "right" if word => buffer.move_word_right(extend),
      "left" => buffer.move_left(extend),
      "right" => buffer.move_right(extend),
      "home" | "up" => buffer.move_home(extend),
      "end" | "down" => buffer.move_end(extend),
      "a" if command => buffer.select_all(),
      "c" | "x" if command => {
        let selected = buffer.selected_text().to_string();
        if !selected.is_empty() {
          cx.write_to_clipboard(ClipboardItem::new_string(selected));
          if keystroke.key == "x" {
            buffer.insert("");
          }
        }
      }
      "v" if command => {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
          buffer.insert(&text);
        }
      }
      // Letters go on to the IME, which hands them back through `replace_text_in_range`.
      _ => return,
    }
    cx.stop_propagation();
    cx.notify();
  }

  fn index_at(&self, position: Point<Pixels>) -> Option<usize> {
    let (line, bounds, scroll) = self.layout.as_ref()?;
    Some(line.closest_index_for_x(position.x - bounds.origin.x + *scroll))
  }

  fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    cx.stop_propagation();
    window.focus(&self.focus);
    if event.click_count > 1 {
      self.buffer.select_all();
    } else if let Some(index) = self.index_at(event.position) {
      if event.modifiers.shift {
        self.buffer.select_to(index);
      } else {
        self.buffer.place(index);
      }
      self.selecting = true;
    }
    cx.notify();
  }

  fn on_mouse_move(
    &mut self,
    event: &MouseMoveEvent,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if !self.selecting || event.pressed_button != Some(MouseButton::Left) {
      self.selecting = false;
      return;
    }
    if let Some(index) = self.index_at(event.position) {
      self.buffer.select_to(index);
      cx.notify();
    }
  }
}

impl EventEmitter<FieldEvent> for TextField {}

impl EntityInputHandler for TextField {
  fn text_for_range(
    &mut self,
    range: Range<usize>,
    adjusted_range: &mut Option<Range<usize>>,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<String> {
    let (text, covered) = self.buffer.text_utf16(range);
    adjusted_range.replace(covered);
    Some(text)
  }

  fn selected_text_range(
    &mut self,
    _ignore_disabled_input: bool,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<UTF16Selection> {
    Some(UTF16Selection {
      range: self.buffer.selected_range_utf16(),
      reversed: false,
    })
  }

  fn marked_text_range(
    &self,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<Range<usize>> {
    self.buffer.marked_range_utf16()
  }

  fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
    self.buffer.unmark();
  }

  fn replace_text_in_range(
    &mut self,
    range: Option<Range<usize>>,
    text: &str,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    self.buffer.replace_utf16(range, text);
    cx.notify();
  }

  fn replace_and_mark_text_in_range(
    &mut self,
    range: Option<Range<usize>>,
    new_text: &str,
    new_selected_range: Option<Range<usize>>,
    _window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    self.buffer.mark_utf16(range, new_text, new_selected_range);
    cx.notify();
  }

  /// Puts the IME candidate window under the text it composes.
  fn bounds_for_range(
    &mut self,
    range_utf16: Range<usize>,
    _element_bounds: Bounds<Pixels>,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<Bounds<Pixels>> {
    let (line, bounds, scroll) = self.layout.as_ref()?;
    let start = self.buffer.offset_from_utf16(range_utf16.start);
    let x = bounds.origin.x + line.x_for_index(start) - *scroll;
    Some(Bounds::new(
      point(x, bounds.origin.y),
      size(px(1.0), bounds.size.height),
    ))
  }

  fn character_index_for_point(
    &mut self,
    point: Point<Pixels>,
    _window: &mut Window,
    _cx: &mut Context<Self>,
  ) -> Option<usize> {
    let index = self.index_at(point)?;
    Some(self.buffer.offset_to_utf16(index))
  }
}

impl Render for TextField {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let entity = cx.entity();
    let text = self.buffer.text().to_string();
    let selection = self.buffer.selection();
    let cursor = self.buffer.cursor();
    let marked = self.buffer.marked();
    let focused = self.focus.is_focused(window);
    let (text_color, selection_color, cursor_color) =
      (self.text_color, self.selection_color, self.cursor_color);
    let text_font = font(self.family.clone());
    let focus = self.focus.clone();
    let measured = entity.clone();
    let field = canvas(
      move |bounds, window, cx| {
        let run = |len: usize, underline: bool| TextRun {
          len,
          font: text_font.clone(),
          color: text_color,
          background_color: None,
          underline: underline.then(|| UnderlineStyle {
            thickness: px(1.0),
            color: Some(text_color),
            wavy: false,
          }),
          strikethrough: None,
        };
        // Text the IME is still composing is underlined, as the platform does.
        let runs: Vec<TextRun> = match &marked {
          Some(marked) => [
            run(marked.start, false),
            run(marked.len(), true),
            run(text.len() - marked.end, false),
          ]
          .into_iter()
          .filter(|run| run.len > 0)
          .collect(),
          None => vec![run(text.len(), false)],
        };
        let line = window
          .text_system()
          .shape_line(text.clone().into(), FONT_SIZE, &runs, None);
        let cursor_x = line.x_for_index(cursor);
        let scroll = (cursor_x - bounds.size.width + px(2.0)).max(px(0.0));
        measured.update(cx, |field, _| {
          field.layout = Some((line.clone(), bounds, scroll));
        });
        (line, scroll)
      },
      move |bounds, (line, scroll), window, cx| {
        let origin = point(bounds.origin.x - scroll, bounds.origin.y);
        let height = bounds.size.height;
        if !selection.is_empty() {
          let start = line.x_for_index(selection.start);
          let end = line.x_for_index(selection.end);
          window.paint_quad(fill(
            Bounds::new(point(origin.x + start, origin.y), size(end - start, height)),
            selection_color,
          ));
        }
        let _ = line.paint(origin, height, window, cx);
        if focused && selection.is_empty() {
          let x = origin.x + line.x_for_index(cursor);
          window.paint_quad(fill(
            Bounds::new(
              point(x, origin.y + px(2.0)),
              size(px(1.0), height - px(4.0)),
            ),
            cursor_color,
          ));
        }
        window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
      },
    )
    .size_full();
    div()
      .track_focus(&self.focus)
      .on_key_down(cx.listener(Self::on_key_down))
      .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
      .on_mouse_move(cx.listener(Self::on_mouse_move))
      .size_full()
      .overflow_hidden()
      .cursor_text()
      .child(field)
  }
}
