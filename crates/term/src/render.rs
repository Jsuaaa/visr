//! One screen row as text plus runs of the same style, ready for any text renderer.

pub use alacritty_terminal::vte::ansi::Rgb;

use alacritty_terminal::term::cell::Flags;

use crate::palette::Palette;
use crate::screen::Screen;

/// A stretch of a row's text drawn in one style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
  /// Length in bytes of the row text.
  pub len: usize,
  pub fg: Rgb,
  /// `None` where the terminal's default background shows through.
  pub bg: Option<Rgb>,
  pub bold: bool,
  pub italic: bool,
  pub underline: bool,
}

impl Run {
  fn same_style(&self, other: &Run) -> bool {
    (self.fg, self.bg, self.bold, self.italic, self.underline)
      == (
        other.fg,
        other.bg,
        other.bold,
        other.italic,
        other.underline,
      )
  }
}

const SPACERS: Flags = Flags::WIDE_CHAR_SPACER.union(Flags::LEADING_WIDE_CHAR_SPACER);

/// Part of a cell, in fractions of its width and height from its top left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
  pub x: f32,
  pub y: f32,
  pub w: f32,
  pub h: f32,
}

const fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect { x, y, w, h }
}

const TOP_LEFT: Rect = rect(0.0, 0.0, 0.5, 0.5);
const TOP_RIGHT: Rect = rect(0.5, 0.0, 0.5, 0.5);
const BOTTOM_LEFT: Rect = rect(0.0, 0.5, 0.5, 0.5);
const BOTTOM_RIGHT: Rect = rect(0.5, 0.5, 0.5, 0.5);

/// Block elements (U+2580..U+259F, shades aside: see `shade`) as rectangles. Drawn as glyphs they
/// leave gaps whenever the line is taller than the font, which breaks pictures made of them.
const BLOCKS: &[(char, &[Rect])] = &[
  ('█', &[rect(0.0, 0.0, 1.0, 1.0)]),
  ('▀', &[rect(0.0, 0.0, 1.0, 0.5)]),
  ('▔', &[rect(0.0, 0.0, 1.0, 0.125)]),
  ('▁', &[rect(0.0, 0.875, 1.0, 0.125)]),
  ('▂', &[rect(0.0, 0.75, 1.0, 0.25)]),
  ('▃', &[rect(0.0, 0.625, 1.0, 0.375)]),
  ('▄', &[rect(0.0, 0.5, 1.0, 0.5)]),
  ('▅', &[rect(0.0, 0.375, 1.0, 0.625)]),
  ('▆', &[rect(0.0, 0.25, 1.0, 0.75)]),
  ('▇', &[rect(0.0, 0.125, 1.0, 0.875)]),
  ('▏', &[rect(0.0, 0.0, 0.125, 1.0)]),
  ('▎', &[rect(0.0, 0.0, 0.25, 1.0)]),
  ('▍', &[rect(0.0, 0.0, 0.375, 1.0)]),
  ('▌', &[rect(0.0, 0.0, 0.5, 1.0)]),
  ('▋', &[rect(0.0, 0.0, 0.625, 1.0)]),
  ('▊', &[rect(0.0, 0.0, 0.75, 1.0)]),
  ('▉', &[rect(0.0, 0.0, 0.875, 1.0)]),
  ('▐', &[rect(0.5, 0.0, 0.5, 1.0)]),
  ('▕', &[rect(0.875, 0.0, 0.125, 1.0)]),
  ('▖', &[BOTTOM_LEFT]),
  ('▗', &[BOTTOM_RIGHT]),
  ('▘', &[TOP_LEFT]),
  ('▝', &[TOP_RIGHT]),
  ('▙', &[TOP_LEFT, BOTTOM_LEFT, BOTTOM_RIGHT]),
  ('▚', &[TOP_LEFT, BOTTOM_RIGHT]),
  ('▛', &[TOP_LEFT, TOP_RIGHT, BOTTOM_LEFT]),
  ('▜', &[TOP_LEFT, TOP_RIGHT, BOTTOM_RIGHT]),
  ('▞', &[TOP_RIGHT, BOTTOM_LEFT]),
  ('▟', &[TOP_RIGHT, BOTTOM_LEFT, BOTTOM_RIGHT]),
];

fn block_rects(c: char) -> Option<&'static [Rect]> {
  // Every cell of every row asks, and almost none is a block.
  if !('\u{2580}'..='\u{259F}').contains(&c) {
    return None;
  }
  BLOCKS
    .iter()
    .find(|(block, _)| *block == c)
    .map(|(_, rects)| *rects)
}

/// How much of the foreground a shade (░▒▓) shows over the background.
fn shade(c: char) -> Option<f32> {
  match c {
    '░' => Some(0.25),
    '▒' => Some(0.5),
    '▓' => Some(0.75),
    _ => None,
  }
}

fn mix(fg: Rgb, bg: Rgb, amount: f32) -> Rgb {
  let channel = |f: u8, b: u8| (b as f32 + (f as f32 - b as f32) * amount).round() as u8;
  Rgb {
    r: channel(fg.r, bg.r),
    g: channel(fg.g, bg.g),
    b: channel(fg.b, bg.b),
  }
}

/// A block element to draw over the row, in its cell's text color (blended, for a shade).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
  pub col: usize,
  pub color: Rgb,
  pub rects: &'static [Rect],
}

/// What a row needs to color its cells: where the cursor is drawn and what's selected.
struct RowStyle {
  cursor_col: Option<usize>,
  selection: Option<alacritty_terminal::selection::SelectionRange>,
}

impl RowStyle {
  /// `show_cursor` is the view's say (focus, blink); the program can still hide it.
  fn new(screen: &Screen, row: usize, show_cursor: bool) -> Self {
    let cursor_col = match screen.cursor_in_view() {
      Some((cursor_row, cursor_col)) if show_cursor && cursor_row == row => Some(cursor_col),
      _ => None,
    };
    Self {
      cursor_col,
      selection: screen.selection_range(),
    }
  }

  /// `(fg, bg)` of a cell after inverse, selection and the cursor.
  fn colors(&self, screen: &Screen, row: usize, col: usize, palette: &Palette) -> (Rgb, Rgb) {
    let cell = screen.cell(row, col);
    let mut fg = palette.resolve(cell.fg, screen.colors());
    let mut bg = palette.resolve(cell.bg, screen.colors());
    if cell.flags.contains(Flags::INVERSE) {
      (fg, bg) = (bg, fg);
    }
    if self
      .selection
      .is_some_and(|range| range.contains(screen.point(row, col)))
    {
      bg = palette.selection;
    }
    if self.cursor_col == Some(col) {
      (fg, bg) = (palette.background, palette.cursor);
    }
    (fg, bg)
  }
}

/// The row's text and its styled runs. Block elements become spaces here; see `row_blocks`.
pub fn row_runs(
  screen: &Screen,
  row: usize,
  palette: &Palette,
  show_cursor: bool,
) -> (String, Vec<Run>) {
  let (cols, _) = screen.size();
  let style = RowStyle::new(screen, row, show_cursor);
  let mut text = String::new();
  let mut runs: Vec<Run> = Vec::new();

  for col in 0..cols as usize {
    let cell = screen.cell(row, col);
    if cell.flags.intersects(SPACERS) {
      continue;
    }
    let (fg, bg) = style.colors(screen, row, col, palette);
    let c = if block_rects(cell.c).is_some() || shade(cell.c).is_some() {
      ' '
    } else {
      cell.c
    };
    let run = Run {
      len: c.len_utf8(),
      fg,
      bg: (bg != palette.background).then_some(bg),
      bold: cell.flags.contains(Flags::BOLD),
      italic: cell.flags.contains(Flags::ITALIC),
      underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
    };
    text.push(c);
    match runs.last_mut() {
      Some(last) if last.same_style(&run) => last.len += run.len,
      _ => runs.push(run),
    }
  }
  (text, runs)
}

/// The row's block elements, to draw as rectangles over the text.
pub fn row_blocks(screen: &Screen, row: usize, palette: &Palette, show_cursor: bool) -> Vec<Block> {
  let (cols, _) = screen.size();
  let style = RowStyle::new(screen, row, show_cursor);
  (0..cols as usize)
    .filter_map(|col| {
      // Colors are resolved only for the few cells that are blocks, not for every cell of the row.
      let c = screen.cell(row, col).c;
      let (rects, amount) = match shade(c) {
        Some(amount) => (block_rects('█')?, Some(amount)),
        None => (block_rects(c)?, None),
      };
      let (fg, bg) = style.colors(screen, row, col, palette);
      let color = amount.map_or(fg, |amount| mix(fg, bg, amount));
      Some(Block { col, color, rects })
    })
    .collect()
}
