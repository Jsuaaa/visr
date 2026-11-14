//! Keys to the bytes a program in a terminal expects (xterm encoding).

/// A key a terminal program can receive, independent of any UI framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
  Char(char),
  Enter,
  Backspace,
  Tab,
  Escape,
  Up,
  Down,
  Left,
  Right,
  Home,
  End,
  PageUp,
  PageDown,
  Insert,
  Delete,
  F(u8),
}

/// The modifier keys held with a key or mouse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
  pub ctrl: bool,
  pub alt: bool,
  pub shift: bool,
}

impl Mods {
  fn any(self) -> bool {
    self.ctrl || self.alt || self.shift
  }

  /// The xterm modifier parameter: 1 + shift + 2·alt + 4·ctrl.
  fn param(self) -> u8 {
    1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
  }
}

/// `app_cursor` is the terminal's application cursor mode (DECCKM).
pub fn encode(key: Key, mods: Mods, app_cursor: bool) -> Vec<u8> {
  match key {
    Key::Char(c) => encode_char(c, mods),
    Key::Enter => b"\r".to_vec(),
    Key::Backspace if mods.ctrl => b"\x08".to_vec(),
    Key::Backspace => b"\x7f".to_vec(),
    Key::Tab if mods.shift => b"\x1b[Z".to_vec(),
    Key::Tab => b"\t".to_vec(),
    Key::Escape => b"\x1b".to_vec(),
    Key::Up => cursor_key(b'A', mods, app_cursor),
    Key::Down => cursor_key(b'B', mods, app_cursor),
    Key::Right => cursor_key(b'C', mods, app_cursor),
    Key::Left => cursor_key(b'D', mods, app_cursor),
    Key::Home => cursor_key(b'H', mods, app_cursor),
    Key::End => cursor_key(b'F', mods, app_cursor),
    Key::Insert => tilde_key(2, mods),
    Key::Delete => tilde_key(3, mods),
    Key::PageUp => tilde_key(5, mods),
    Key::PageDown => tilde_key(6, mods),
    Key::F(n @ 1..=4) => {
      let letter = b'P' + (n - 1);
      if mods.any() {
        format!("\x1b[1;{}{}", mods.param(), letter as char).into_bytes()
      } else {
        vec![0x1b, b'O', letter]
      }
    }
    Key::F(n) => match function_code(n) {
      Some(code) => tilde_key(code, mods),
      None => Vec::new(),
    },
  }
}

/// Pasted text as the program should get it. In bracketed mode escapes are dropped,
/// so the text can't close the bracket early and run as typed commands.
pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
  let text = text.replace("\r\n", "\r").replace('\n', "\r");
  if !bracketed {
    return text.into_bytes();
  }
  let mut bytes = b"\x1b[200~".to_vec();
  bytes.extend(text.replace('\x1b', "").as_bytes());
  bytes.extend(b"\x1b[201~");
  bytes
}

fn encode_char(c: char, mods: Mods) -> Vec<u8> {
  let mut bytes = Vec::new();
  if mods.alt {
    bytes.push(0x1b);
  }
  match (mods.ctrl, c) {
    (true, ' ' | '@' | '2') => bytes.push(0x00),
    (true, 'a'..='z' | 'A'..='Z') => bytes.push(c.to_ascii_lowercase() as u8 & 0x1f),
    (true, '[' | '\\' | ']' | '^' | '_') => bytes.push(c as u8 & 0x1f),
    _ => bytes.extend(c.to_string().as_bytes()),
  }
  bytes
}

fn cursor_key(letter: u8, mods: Mods, app_cursor: bool) -> Vec<u8> {
  if mods.any() {
    format!("\x1b[1;{}{}", mods.param(), letter as char).into_bytes()
  } else if app_cursor {
    vec![0x1b, b'O', letter]
  } else {
    vec![0x1b, b'[', letter]
  }
}

fn tilde_key(code: u8, mods: Mods) -> Vec<u8> {
  if mods.any() {
    format!("\x1b[{code};{}~", mods.param())
  } else {
    format!("\x1b[{code}~")
  }
  .into_bytes()
}

fn function_code(n: u8) -> Option<u8> {
  match n {
    5 => Some(15),
    6 => Some(17),
    7 => Some(18),
    8 => Some(19),
    9 => Some(20),
    10 => Some(21),
    11 => Some(23),
    12 => Some(24),
    _ => None,
  }
}
