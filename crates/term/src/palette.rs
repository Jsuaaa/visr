//! Turns terminal colors into RGB: the program's own overrides first, then the theme.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// The theme colors a terminal draws with.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
  /// The 16 ANSI colors: normal 0-7, bright 8-15.
  pub ansi: [Rgb; 16],
  pub foreground: Rgb,
  pub background: Rgb,
  pub cursor: Rgb,
  pub selection: Rgb,
}

/// Steps of the xterm 6x6x6 color cube.
const CUBE_STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];

impl Palette {
  /// The RGB for `color`: a program's override in `overrides` wins over the palette.
  pub(crate) fn resolve(&self, color: Color, overrides: &Colors) -> Rgb {
    match color {
      Color::Spec(rgb) => rgb,
      Color::Named(name) => overrides[name].unwrap_or_else(|| self.named(name)),
      Color::Indexed(index) => overrides[index as usize].unwrap_or_else(|| self.indexed(index)),
    }
  }

  fn named(&self, name: NamedColor) -> Rgb {
    let index = name as usize;
    match name {
      NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::DimForeground => {
        self.foreground
      }
      NamedColor::Background => self.background,
      NamedColor::Cursor => self.cursor,
      _ if index < 16 => self.ansi[index],
      // Dim variants: no separate theme colors yet, so they reuse the normal ones.
      _ => self.ansi[(index - NamedColor::DimBlack as usize) % 8],
    }
  }

  fn indexed(&self, index: u8) -> Rgb {
    match index {
      0..=15 => self.ansi[index as usize],
      16..=231 => {
        let cube = index - 16;
        let step = |value: u8| CUBE_STEPS[value as usize];
        Rgb {
          r: step(cube / 36),
          g: step(cube / 6 % 6),
          b: step(cube % 6),
        }
      }
      _ => {
        let level = 8 + (index - 232) * 10;
        Rgb {
          r: level,
          g: level,
          b: level,
        }
      }
    }
  }
}
