//! Mouse events for programs that ask for them (vim with `mouse=a`, htop, Claude Code), in the
//! xterm encodings: SGR (`ESC [ < b ; x ; y M`) when the program enables it, else the legacy
//! `ESC [ M` with each value as one byte.

use crate::keys::Mods;

/// The button an event is about; the wheel counts as two buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
  Left,
  Middle,
  Right,
  WheelUp,
  WheelDown,
}

/// What the button did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Press,
  Release,
  /// Moved with the button held.
  Drag,
}

/// A mouse event over the cell at `col`, `row` (zero-based, in the view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
  pub button: Button,
  pub kind: Kind,
  pub col: usize,
  pub row: usize,
  pub mods: Mods,
}

/// The legacy encoding's values are one byte each, offset by 32.
const LEGACY_MAX: usize = 255 - 32;

/// The bytes to send the program for `event`: SGR when `sgr` is set, else the legacy encoding.
pub fn encode(event: MouseEvent, sgr: bool) -> Vec<u8> {
  let mut code = match event.button {
    Button::Left => 0,
    Button::Middle => 1,
    Button::Right => 2,
    Button::WheelUp => 64,
    Button::WheelDown => 65,
  };
  // The legacy encoding cannot say which button went up.
  if !sgr && event.kind == Kind::Release {
    code = 3;
  }
  if event.kind == Kind::Drag {
    code += 32;
  }
  if event.mods.shift {
    code += 4;
  }
  if event.mods.alt {
    code += 8;
  }
  if event.mods.ctrl {
    code += 16;
  }
  let (col, row) = (event.col + 1, event.row + 1);
  if sgr {
    let end = if event.kind == Kind::Release {
      'm'
    } else {
      'M'
    };
    return format!("\x1b[<{code};{col};{row}{end}").into_bytes();
  }
  let byte = |value: usize| (value.min(LEGACY_MAX) + 32) as u8;
  vec![0x1b, b'[', b'M', byte(code), byte(col), byte(row)]
}
