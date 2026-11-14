//! The screen of one terminal: bytes from the core go in, a grid comes out.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::grid::{Dimensions, GridCell};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;

/// How much history a terminal keeps; the app asks the core for as much.
pub const SCROLLBACK_LINES: usize = 10_000;
const CLEAR_SCREEN: &[u8] = b"\x1b[2J";

/// Grid dimensions handed to the emulator.
struct Size {
  cols: usize,
  rows: usize,
}

impl Dimensions for Size {
  fn total_lines(&self) -> usize {
    self.rows
  }

  fn screen_lines(&self) -> usize {
    self.rows
  }

  fn columns(&self) -> usize {
    self.cols
  }
}

/// Mouse reporting a program turned on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseMode {
  /// Moves with a button held are reported too.
  pub drags: bool,
  /// SGR encoding rather than the legacy one.
  pub sgr: bool,
}

/// One terminal's emulator state: the grid, its history, the modes and the selection.
pub struct Screen {
  // The core answers display queries itself, so nothing the emulator wants to write back is sent.
  term: Term<VoidListener>,
  parser: Processor,
  /// Where the current selection started, in grid coordinates.
  anchor: Option<Point>,
}

impl Screen {
  /// An empty screen of `cols` by `rows` cells, with `SCROLLBACK_LINES` of history.
  pub fn new(cols: u16, rows: u16) -> Self {
    let config = Config {
      scrolling_history: SCROLLBACK_LINES,
      ..Config::default()
    };
    let size = Size {
      cols: cols as usize,
      rows: rows as usize,
    };
    Self {
      term: Term::new(config, &size, VoidListener),
      parser: Processor::new(),
      anchor: None,
    }
  }

  /// Runs bytes from the program through the emulator.
  pub fn feed(&mut self, bytes: &[u8]) {
    let mut rest = bytes;
    while let Some(at) = rest
      .windows(CLEAR_SCREEN.len())
      .position(|window| window == CLEAR_SCREEN)
    {
      self.parser.advance(&mut self.term, &rest[..at]);
      // alacritty pushes at least one line into history on every full clear, even a blank one;
      // that line later slides back into view when the pane grows. A blank screen with no
      // history looks the same cleared or not, so the clear is skipped.
      if !self.is_pristine() {
        self.parser.advance(&mut self.term, CLEAR_SCREEN);
      }
      rest = &rest[at + CLEAR_SCREEN.len()..];
    }
    self.parser.advance(&mut self.term, rest);
  }

  /// Nothing drawn and no history.
  fn is_pristine(&self) -> bool {
    let (cols, rows) = self.size();
    self.term.grid().history_size() == 0
      && (0..rows as usize).all(|row| (0..cols as usize).all(|col| self.cell(row, col).is_empty()))
  }

  /// Changes the grid size, reflowing as the emulator does.
  pub fn resize(&mut self, cols: u16, rows: u16) {
    self.term.resize(Size {
      cols: cols as usize,
      rows: rows as usize,
    });
  }

  /// `(cols, rows)`.
  pub fn size(&self) -> (u16, u16) {
    (self.term.columns() as u16, self.term.screen_lines() as u16)
  }

  /// Rows and cols are in the view: row 0 is the top line shown, which may be history.
  pub(crate) fn point(&self, row: usize, col: usize) -> Point {
    Point::new(Line(row as i32 - self.display_offset() as i32), Column(col))
  }

  /// The cell at a view row and col, with the same coordinates as `point`.
  pub(crate) fn cell(&self, row: usize, col: usize) -> &Cell {
    &self.term.grid()[self.point(row, col)]
  }

  /// `(row, col)`.
  fn cursor(&self) -> (usize, usize) {
    let point = self.term.grid().cursor.point;
    (point.line.0 as usize, point.column.0)
  }

  /// Where the cursor shows in the view, if it does.
  pub fn cursor_in_view(&self) -> Option<(usize, usize)> {
    let (row, col) = self.cursor();
    let row = row + self.display_offset();
    (self.cursor_visible() && row < self.term.screen_lines()).then_some((row, col))
  }

  /// The program has not hidden the cursor.
  fn cursor_visible(&self) -> bool {
    self.term.mode().contains(TermMode::SHOW_CURSOR)
  }

  /// The program turned on application cursor keys (DECCKM).
  pub fn app_cursor(&self) -> bool {
    self.term.mode().contains(TermMode::APP_CURSOR)
  }

  /// The program is on the alternate screen, which has no history.
  fn alt_screen(&self) -> bool {
    self.term.mode().contains(TermMode::ALT_SCREEN)
  }

  /// The program asked for pasted text to be wrapped in bracketed-paste markers.
  pub fn bracketed_paste(&self) -> bool {
    self.term.mode().contains(TermMode::BRACKETED_PASTE)
  }

  /// How the program wants the mouse reported, if it does.
  pub fn mouse(&self) -> Option<MouseMode> {
    let mode = self.term.mode();
    let drags = mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION);
    mode.intersects(TermMode::MOUSE_MODE).then_some(MouseMode {
      drags,
      sgr: mode.contains(TermMode::SGR_MOUSE),
    })
  }

  /// The program expects the wheel as arrow keys rather than scrolling history.
  pub fn alternate_scroll(&self) -> bool {
    self.alt_screen() && self.term.mode().contains(TermMode::ALTERNATE_SCROLL)
  }

  /// Positive goes back into history.
  pub fn scroll(&mut self, lines: i32) {
    self.term.scroll_display(Scroll::Delta(lines));
  }

  /// Returns the view to the live screen.
  pub fn scroll_to_bottom(&mut self) {
    self.term.scroll_display(Scroll::Bottom);
  }

  /// How many lines of history sit above the live screen in the view.
  fn display_offset(&self) -> usize {
    self.term.grid().display_offset()
  }

  /// Starts a selection at the view cell, replacing any previous one.
  pub fn select_start(&mut self, row: usize, col: usize) {
    let point = self.point(row, col);
    self.anchor = Some(point);
    self.term.selection = Some(Selection::new(SelectionType::Simple, point, Side::Left));
  }

  /// Extends the selection begun by `select_start` to the view cell.
  pub fn select_to(&mut self, row: usize, col: usize) {
    let Some(anchor) = self.anchor else { return };
    let point = self.point(row, col);
    // Sides say which half of a cell the edge is on; both ends are included either way.
    let (start_side, end_side) = if point < anchor {
      (Side::Right, Side::Left)
    } else {
      (Side::Left, Side::Right)
    };
    let mut selection = Selection::new(SelectionType::Simple, anchor, start_side);
    selection.update(point, end_side);
    self.term.selection = Some(selection);
  }

  /// Drops the selection.
  pub fn clear_selection(&mut self) {
    self.anchor = None;
    self.term.selection = None;
  }

  /// The selected text, if any is selected.
  pub fn selected_text(&self) -> Option<String> {
    self
      .term
      .selection_to_string()
      .filter(|text| !text.is_empty())
  }

  /// The selection as a range of grid points, if one is active.
  pub(crate) fn selection_range(&self) -> Option<SelectionRange> {
    self
      .term
      .selection
      .as_ref()
      .and_then(|selection| selection.to_range(&self.term))
  }

  /// Colors the program changed with OSC sequences.
  pub(crate) fn colors(&self) -> &Colors {
    self.term.colors()
  }
}
