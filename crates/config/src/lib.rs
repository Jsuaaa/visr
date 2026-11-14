//! The user's look and keys: built-in defaults with an optional TOML file on top, so the same
//! file gives the same app on every machine.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use term::palette::Palette;
use term::render::Rgb;
use toml::{Table, Value};

const COMMON_DEFAULTS: &str = include_str!("../defaults/common.toml");
#[cfg(target_os = "macos")]
const PLATFORM_DEFAULTS: &str = include_str!("../defaults/macos.toml");
#[cfg(not(target_os = "macos"))]
const PLATFORM_DEFAULTS: &str = include_str!("../defaults/other.toml");

/// Why a config could not be loaded, as a message ready to show to the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for ConfigError {}

/// The settings Visr runs with: font, terminal palette and keybindings.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
  pub font: FontConfig,
  pub palette: Palette,
  pub keys: Keymap,
}

/// The terminal font. Sizes are in logical pixels.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontConfig {
  pub family: String,
  pub size: f32,
  pub line_height: f32,
}

/// The merged TOML as read, before colors and bindings are checked.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
  font: FontConfig,
  theme: RawTheme,
  keys: BTreeMap<String, String>,
}

/// The `[theme]` table as written: colors are still `#rrggbb` text.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTheme {
  foreground: String,
  background: String,
  cursor: String,
  selection: String,
  ansi: [String; 16],
}

impl Config {
  /// The built-in defaults for this platform.
  pub fn defaults() -> Self {
    Self::parse("").expect("the built-in defaults are valid")
  }

  /// The defaults with the user's TOML on top.
  fn parse(user: &str) -> Result<Self, ConfigError> {
    let user_table = parse_table(user)?;
    let user_keys: Vec<String> = user_table
      .get("keys")
      .and_then(Value::as_table)
      .map(|keys| keys.keys().cloned().collect())
      .unwrap_or_default();
    let mut merged = parse_table(COMMON_DEFAULTS)?;
    merge(&mut merged, parse_table(PLATFORM_DEFAULTS)?);
    merge(&mut merged, user_table);
    let raw: RawConfig = Value::Table(merged)
      .try_into()
      .map_err(|error| ConfigError(error.to_string()))?;
    Ok(Self {
      font: raw.font,
      palette: palette(&raw.theme)?,
      keys: Keymap::from_names(&raw.keys, &user_keys)?,
    })
  }

  /// A missing file is fine: the defaults apply.
  pub fn load(path: &Path) -> Result<Self, ConfigError> {
    match std::fs::read_to_string(path) {
      Ok(text) => Self::parse(&text),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::defaults()),
      Err(error) => Err(ConfigError(format!("{}: {error}", path.display()))),
    }
  }
}

/// `VISR_CONFIG`, else `~/.config/visr/config.toml`, or `%APPDATA%\visr\config.toml` on Windows.
pub fn config_path() -> PathBuf {
  if let Some(path) = std::env::var_os("VISR_CONFIG") {
    return path.into();
  }
  let base = if cfg!(windows) {
    std::env::var_os("APPDATA").map(PathBuf::from)
  } else {
    std::env::home_dir().map(|home| home.join(".config"))
  };
  base.unwrap_or_default().join("visr").join("config.toml")
}

fn parse_table(text: &str) -> Result<Table, ConfigError> {
  text
    .parse::<Table>()
    .map_err(|error| ConfigError(error.to_string()))
}

/// Tables merge key by key; anything else in `over` replaces what `base` had.
fn merge(base: &mut Table, over: Table) {
  for (key, value) in over {
    match (base.get_mut(&key), value) {
      (Some(Value::Table(base_table)), Value::Table(over_table)) => merge(base_table, over_table),
      (_, value) => {
        base.insert(key, value);
      }
    }
  }
}

fn color(text: &str) -> Result<Rgb, ConfigError> {
  let invalid = || ConfigError(format!("theme: `{text}` is not a #rrggbb color"));
  let hex = text
    .strip_prefix('#')
    .filter(|hex| hex.len() == 6)
    .ok_or_else(invalid)?;
  let value = u32::from_str_radix(hex, 16).map_err(|_| invalid())?;
  Ok(Rgb {
    r: (value >> 16) as u8,
    g: (value >> 8) as u8,
    b: value as u8,
  })
}

fn palette(theme: &RawTheme) -> Result<Palette, ConfigError> {
  let mut ansi = [Rgb::default(); 16];
  for (slot, text) in ansi.iter_mut().zip(&theme.ansi) {
    *slot = color(text)?;
  }
  Ok(Palette {
    ansi,
    foreground: color(&theme.foreground)?,
    background: color(&theme.background)?,
    cursor: color(&theme.cursor)?,
    selection: color(&theme.selection)?,
  })
}

/// A key as pressed: the key's name (`t`, `]`, `enter`) and exactly which modifiers were held.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyPress {
  pub key: String,
  pub ctrl: bool,
  pub alt: bool,
  pub shift: bool,
  /// cmd on macOS, the Windows key elsewhere.
  pub platform: bool,
}

impl KeyPress {
  /// How menus write the shortcut on this platform: `⌥⌘W` on macOS, `Alt+Win+W` elsewhere.
  pub fn label(&self) -> String {
    let mac = cfg!(target_os = "macos");
    let key = match (self.key.as_str(), mac) {
      ("up", true) => "↑".to_string(),
      ("down", true) => "↓".to_string(),
      ("left", true) => "←".to_string(),
      ("right", true) => "→".to_string(),
      (key, true) => key.to_uppercase(),
      (key, false) => {
        let mut chars = key.chars();
        chars
          .next()
          .map(|first| first.to_uppercase().chain(chars).collect())
          .unwrap_or_default()
      }
    };
    let modifiers = [
      (self.ctrl, "⌃", "Ctrl"),
      (self.alt, "⌥", "Alt"),
      (self.shift, "⇧", "Shift"),
      (self.platform, "⌘", "Win"),
    ];
    let held = modifiers
      .iter()
      .filter(|(held, _, _)| *held)
      .map(|(_, symbol, word)| if mac { *symbol } else { *word });
    if mac {
      held.chain([key.as_str()]).collect()
    } else {
      held.chain([key.as_str()]).collect::<Vec<_>>().join("+")
    }
  }
}

/// Something a keybinding can ask the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
  NewTab,
  SplitRight,
  SplitDown,
  ClosePane,
  CloseTab,
  FocusNext,
  FocusPrev,
  /// Zero-based.
  SelectTab(usize),
  MoveTabUp,
  MoveTabDown,
  ToggleSidebar,
  ZoomPane,
  Copy,
  Paste,
}

fn action_named(name: &str) -> Option<Action> {
  let action = match name {
    "new_tab" => Action::NewTab,
    "split_right" => Action::SplitRight,
    "split_down" => Action::SplitDown,
    "close_pane" => Action::ClosePane,
    "close_tab" => Action::CloseTab,
    "focus_next" => Action::FocusNext,
    "focus_prev" => Action::FocusPrev,
    "move_tab_up" => Action::MoveTabUp,
    "move_tab_down" => Action::MoveTabDown,
    "toggle_sidebar" => Action::ToggleSidebar,
    "zoom_pane" => Action::ZoomPane,
    "copy" => Action::Copy,
    "paste" => Action::Paste,
    _ => match name.strip_prefix("select_tab_")?.parse() {
      Ok(tab @ 1..=9) => Action::SelectTab(tab - 1),
      _ => return None,
    },
  };
  Some(action)
}

/// Parses `cmd-shift-d`, `ctrl--`, `alt-enter`.
fn parse_binding(text: &str) -> Result<KeyPress, ConfigError> {
  let (modifiers, key) = match text.strip_suffix("--") {
    Some(modifiers) => (modifiers, "-"),
    None => text.rsplit_once('-').unwrap_or(("", text)),
  };
  if key.is_empty() {
    return Err(ConfigError(format!("keys: `{text}` has no key")));
  }
  let mut press = KeyPress {
    key: key.to_lowercase(),
    ctrl: false,
    alt: false,
    shift: false,
    platform: false,
  };
  for modifier in modifiers.split('-').filter(|modifier| !modifier.is_empty()) {
    match modifier {
      "cmd" | "super" | "win" => press.platform = true,
      "ctrl" => press.ctrl = true,
      "alt" | "opt" | "option" => press.alt = true,
      "shift" => press.shift = true,
      _ => {
        return Err(ConfigError(format!(
          "keys: unknown modifier `{modifier}` in `{text}`"
        )));
      }
    }
  }
  Ok(press)
}

/// The bindings from keys to actions, built from the defaults and the user's `[keys]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Keymap {
  bindings: Vec<(KeyPress, Action)>,
}

impl Keymap {
  /// `user_keys` are the actions the user's file sets: their keys are theirs, so a default on the
  /// same key gives way. Two bindings left on one key is an error, not a silent winner.
  fn from_names(
    names: &BTreeMap<String, String>,
    user_keys: &[String],
  ) -> Result<Self, ConfigError> {
    let mut parsed = Vec::new();
    for (name, binding) in names {
      let action =
        action_named(name).ok_or_else(|| ConfigError(format!("keys: unknown action `{name}`")))?;
      if !binding.is_empty() {
        parsed.push((name.as_str(), parse_binding(binding)?, action));
      }
    }
    let is_user_key = |name: &str| user_keys.iter().any(|key| key == name);
    let user_presses: Vec<&KeyPress> = parsed
      .iter()
      .filter(|(name, _, _)| is_user_key(name))
      .map(|(_, press, _)| press)
      .collect();
    let kept: Vec<&(&str, KeyPress, Action)> = parsed
      .iter()
      .filter(|(name, press, _)| is_user_key(name) || !user_presses.contains(&press))
      .collect();
    for (index, (name, press, _)) in kept.iter().enumerate() {
      if let Some((other, _, _)) = kept[..index].iter().find(|(_, other, _)| other == press) {
        return Err(ConfigError(format!(
          "keys: `{other}` and `{name}` are both bound to the same key"
        )));
      }
    }
    let bindings = kept
      .into_iter()
      .map(|(_, press, action)| (press.clone(), *action))
      .collect();
    Ok(Self { bindings })
  }

  /// The key bound to `action`, if it has one.
  pub fn binding(&self, action: Action) -> Option<&KeyPress> {
    self
      .bindings
      .iter()
      .find(|(_, bound)| *bound == action)
      .map(|(press, _)| press)
  }

  /// The action bound to exactly this key press, if any.
  pub fn action(&self, press: &KeyPress) -> Option<Action> {
    self
      .bindings
      .iter()
      .find(|(bound, _)| bound == press)
      .map(|(_, action)| *action)
  }
}
