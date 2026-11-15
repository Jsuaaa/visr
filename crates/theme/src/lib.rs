//! The theme file format: parsing, validation, fallback and the built-in themes. It is data only:
//! colors are plain RGBA and no file is looked up here.

use std::fmt;

use toml::{Table, Value};

const DEFAULT_SOURCE: &str = include_str!("../themes/default.toml");
const DEFAULT_LIGHT_SOURCE: &str = include_str!("../themes/default_light.toml");

/// A color as written in a theme file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
  pub r: u8,
  pub g: u8,
  pub b: u8,
  pub a: u8,
}

impl Rgba {
  /// Stands in for a role that has no value, only while its problem is being reported.
  const TRANSPARENT: Self = Self {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
  };
}

/// Whether a theme is meant for a dark or a light system appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
  Dark,
  Light,
}

/// A theme: the colors of the chrome and of the terminals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
  pub appearance: Appearance,
  pub ui: UiColors,
  pub terminal: TerminalColors,
}

/// The `[ui]` roles: every color the chrome draws with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiColors {
  /// The window and the sidebar behind everything else.
  pub surface: Rgba,
  /// A row or a button under the pointer.
  pub surface_hover: Rgba,
  /// The active row, an icon's backing and a secondary button.
  pub surface_active: Rgba,
  /// A button under the pointer inside a row that is itself hovered or active, and the edge and
  /// the separators of a menu.
  pub surface_hover_strong: Rgba,
  /// Menus and other panels that float over the window.
  pub surface_raised: Rgba,
  /// A menu row under the pointer.
  pub surface_raised_hover: Rgba,
  /// A recessed area, such as the setup log box.
  pub surface_sunken: Rgba,
  /// The banner that reports an error.
  pub surface_error: Rgba,
  /// The edge of a pane and the divider lines.
  pub border: Rgba,
  /// The edge of the focused pane, the sidebar's rename field, the back layer of the stack icon,
  /// and a divider being hovered or dragged.
  pub border_focused: Rgba,
  /// A panel's edge while its width is the default one.
  pub border_snapped: Rgba,
  /// The main text of the chrome, and the cursor of a text field.
  pub text: Rgba,
  /// Labels, icons and shortcuts that stay quieter than the main text.
  pub text_muted: Rgba,
  /// Secondary text in the setup window.
  pub text_secondary: Rgba,
  /// A link.
  pub text_link: Rgba,
  /// A warning message.
  pub text_warning: Rgba,
  /// The selected text of a text field.
  pub selection: Rgba,
  /// The background of a primary button.
  pub button_primary: Rgba,
  /// A modified file in the changes panel.
  pub git_modified: Rgba,
  /// An added file in the changes panel.
  pub git_added: Rgba,
  /// A deleted file in the changes panel.
  pub git_deleted: Rgba,
  /// A renamed or untracked file in the changes panel.
  pub git_untracked: Rgba,
  /// A file with a merge conflict in the changes panel.
  pub git_conflict: Rgba,
  /// The dot of a terminal that has nothing going on.
  pub status_quiet: Rgba,
  /// The dot of a terminal that is working.
  pub status_working: Rgba,
  /// The dot of a terminal that needs a person.
  pub status_blocked: Rgba,
  /// The dot of a terminal that is done.
  pub status_done: Rgba,
  /// The dot of a terminal whose program has ended.
  pub status_exited: Rgba,
}

/// The `[terminal]` colors: what the terminal grids draw with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalColors {
  /// The text of the grid, and the chrome around it that sits on the terminal background.
  pub foreground: Rgba,
  /// The grid, the pane body and the startup screen.
  pub background: Rgba,
  /// The cursor of the grid.
  pub cursor: Rgba,
  /// The selected text of the grid.
  pub selection: Rgba,
  /// The 16 ANSI colors: normal 0-7, bright 8-15.
  pub ansi: [Rgba; 16],
}

/// What is wrong with a key of a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
  Missing,
  NotText,
  NotATable,
  UnknownKey,
  InvalidAppearance,
  InvalidColor,
  WrongAnsiLength,
}

impl fmt::Display for Problem {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Self::Missing => "is missing",
      Self::NotText => "is not text",
      Self::NotATable => "is not a table",
      Self::UnknownKey => "is not a known key",
      Self::InvalidAppearance => "is not `dark` or `light`",
      Self::InvalidColor => "is not a #rrggbb or #rrggbbaa color",
      Self::WrongAnsiLength => "is not a list of 16 colors",
    })
  }
}

/// One problem found in a theme, with the key it is about. The file is named by whoever read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeIssue {
  pub key: String,
  pub problem: Problem,
}

impl fmt::Display for ThemeIssue {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "`{}` {}", self.key, self.problem)
  }
}

/// A theme that could not be parsed at all, as a message ready to show to the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeError(toml::de::Error);

impl fmt::Display for ThemeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.0.fmt(f)
  }
}

impl std::error::Error for ThemeError {}

/// A usable theme and the problems that were found while reading it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTheme {
  pub theme: Theme,
  pub issues: Vec<ThemeIssue>,
}

/// Reads a theme file. Missing roles come from the built-in theme of the same appearance, and
/// bad values are reported while the rest of the theme still applies.
pub fn parse(text: &str) -> Result<ParsedTheme, ThemeError> {
  read(text, Base::Builtin)
}

/// The built-in theme called `name`, if there is one.
pub fn builtin(name: &str) -> Option<ParsedTheme> {
  match name {
    "default" => Some(default_theme(Appearance::Dark)),
    "default_light" => Some(default_theme(Appearance::Light)),
    _ => None,
  }
}

/// The built-in theme for an appearance: `default` for dark, `default_light` for light. Its issues
/// are about the built-in file itself, so they should show up the first time the app runs.
pub fn default_theme(appearance: Appearance) -> ParsedTheme {
  let source = match appearance {
    Appearance::Dark => DEFAULT_SOURCE,
    Appearance::Light => DEFAULT_LIGHT_SOURCE,
  };
  read(source, Base::None).expect("the built-in themes are valid TOML")
}

/// Where the roles a file lacks come from.
enum Base {
  /// The built-in theme of the file's appearance.
  Builtin,
  /// Nowhere: a missing role is reported. Used for the built-in themes themselves.
  None,
}

/// Reads `text`, filling the roles it lacks from `base`.
fn read(text: &str, base: Base) -> Result<ParsedTheme, ThemeError> {
  let table = text.parse::<Table>().map_err(ThemeError)?;
  let mut issues = Vec::new();
  let mut root = Section::root(&table, &mut issues);
  let appearance = read_meta(root.child("meta"));
  let base = match base {
    Base::Builtin => Some(default_theme(appearance).theme),
    Base::None => None,
  };
  let ui = UiColors::read(root.child("ui"), base.as_ref().map(|base| &base.ui));
  let terminal = TerminalColors::read(
    root.child("terminal"),
    base.as_ref().map(|base| &base.terminal),
  );
  root.finish();
  Ok(ParsedTheme {
    theme: Theme {
      appearance,
      ui,
      terminal,
    },
    issues,
  })
}

fn read_meta(mut section: Section<'_>) -> Appearance {
  section.text("name");
  let appearance = match section.text("appearance") {
    Some("light") => Appearance::Light,
    Some("dark") | None => Appearance::Dark,
    Some(_) => {
      section.report("appearance", Problem::InvalidAppearance);
      Appearance::Dark
    }
  };
  section.finish();
  appearance
}

impl UiColors {
  fn read(mut section: Section<'_>, base: Option<&Self>) -> Self {
    let colors = Self {
      surface: section.color("surface", base.map(|base| base.surface)),
      surface_hover: section.color("surface_hover", base.map(|base| base.surface_hover)),
      surface_active: section.color("surface_active", base.map(|base| base.surface_active)),
      surface_hover_strong: section.color(
        "surface_hover_strong",
        base.map(|base| base.surface_hover_strong),
      ),
      surface_raised: section.color("surface_raised", base.map(|base| base.surface_raised)),
      surface_raised_hover: section.color(
        "surface_raised_hover",
        base.map(|base| base.surface_raised_hover),
      ),
      surface_sunken: section.color("surface_sunken", base.map(|base| base.surface_sunken)),
      surface_error: section.color("surface_error", base.map(|base| base.surface_error)),
      border: section.color("border", base.map(|base| base.border)),
      border_focused: section.color("border_focused", base.map(|base| base.border_focused)),
      border_snapped: section.color("border_snapped", base.map(|base| base.border_snapped)),
      text: section.color("text", base.map(|base| base.text)),
      text_muted: section.color("text_muted", base.map(|base| base.text_muted)),
      text_secondary: section.color("text_secondary", base.map(|base| base.text_secondary)),
      text_link: section.color("text_link", base.map(|base| base.text_link)),
      text_warning: section.color("text_warning", base.map(|base| base.text_warning)),
      selection: section.color("selection", base.map(|base| base.selection)),
      button_primary: section.color("button_primary", base.map(|base| base.button_primary)),
      git_modified: section.color("git_modified", base.map(|base| base.git_modified)),
      git_added: section.color("git_added", base.map(|base| base.git_added)),
      git_deleted: section.color("git_deleted", base.map(|base| base.git_deleted)),
      git_untracked: section.color("git_untracked", base.map(|base| base.git_untracked)),
      git_conflict: section.color("git_conflict", base.map(|base| base.git_conflict)),
      status_quiet: section.color("status_quiet", base.map(|base| base.status_quiet)),
      status_working: section.color("status_working", base.map(|base| base.status_working)),
      status_blocked: section.color("status_blocked", base.map(|base| base.status_blocked)),
      status_done: section.color("status_done", base.map(|base| base.status_done)),
      status_exited: section.color("status_exited", base.map(|base| base.status_exited)),
    };
    section.finish();
    colors
  }
}

impl TerminalColors {
  fn read(mut section: Section<'_>, base: Option<&Self>) -> Self {
    let colors = Self {
      foreground: section.color("foreground", base.map(|base| base.foreground)),
      background: section.color("background", base.map(|base| base.background)),
      cursor: section.color("cursor", base.map(|base| base.cursor)),
      selection: section.color("selection", base.map(|base| base.selection)),
      ansi: section.ansi("ansi", base.map(|base| base.ansi)),
    };
    section.finish();
    colors
  }
}

/// One table of a theme file while it is read: it remembers which keys were asked for, so the
/// rest can be reported as unknown, and it collects the problems under their full key.
struct Section<'a> {
  path: &'static str,
  table: Option<&'a Table>,
  seen: Vec<&'static str>,
  issues: &'a mut Vec<ThemeIssue>,
}

impl<'a> Section<'a> {
  fn root(table: &'a Table, issues: &'a mut Vec<ThemeIssue>) -> Self {
    Self {
      path: "",
      table: Some(table),
      seen: Vec::new(),
      issues,
    }
  }

  fn child(&mut self, key: &'static str) -> Section<'_> {
    self.seen.push(key);
    let table = match self.table.and_then(|table| table.get(key)) {
      Some(Value::Table(table)) => Some(table),
      Some(_) => {
        self.report(key, Problem::NotATable);
        None
      }
      None => None,
    };
    Section {
      path: key,
      table,
      seen: Vec::new(),
      issues: &mut *self.issues,
    }
  }

  fn report(&mut self, key: &str, problem: Problem) {
    let key = if self.path.is_empty() {
      key.to_string()
    } else {
      format!("{}.{key}", self.path)
    };
    self.issues.push(ThemeIssue { key, problem });
  }

  /// A text value. Reports it when it is missing or is not text.
  fn text(&mut self, key: &'static str) -> Option<&'a str> {
    self.seen.push(key);
    match self.table.and_then(|table| table.get(key)) {
      Some(Value::String(text)) => Some(text.as_str()),
      Some(_) => {
        self.report(key, Problem::NotText);
        None
      }
      None => {
        self.report(key, Problem::Missing);
        None
      }
    }
  }

  /// A color role. A role that is missing or invalid takes `fallback`; without one, it is only
  /// reported.
  fn color(&mut self, key: &'static str, fallback: Option<Rgba>) -> Rgba {
    self.seen.push(key);
    match self.table.and_then(|table| table.get(key)) {
      Some(value) => match value.as_str().and_then(parse_hex) {
        Some(color) => return color,
        None => self.report(key, Problem::InvalidColor),
      },
      None if fallback.is_none() => self.report(key, Problem::Missing),
      None => {}
    }
    fallback.unwrap_or(Rgba::TRANSPARENT)
  }

  /// The 16 ANSI colors. A bad color falls back on its own; a list of the wrong size falls back
  /// whole.
  fn ansi(&mut self, key: &'static str, fallback: Option<[Rgba; 16]>) -> [Rgba; 16] {
    self.seen.push(key);
    let fallback_colors = fallback.unwrap_or([Rgba::TRANSPARENT; 16]);
    let Some(value) = self.table.and_then(|table| table.get(key)) else {
      if fallback.is_none() {
        self.report(key, Problem::Missing);
      }
      return fallback_colors;
    };
    let Some(items) = value.as_array().filter(|items| items.len() == 16) else {
      self.report(key, Problem::WrongAnsiLength);
      return fallback_colors;
    };
    std::array::from_fn(|index| {
      items[index]
        .as_str()
        .and_then(parse_hex)
        .unwrap_or_else(|| {
          self.report(&format!("{key}[{index}]"), Problem::InvalidColor);
          fallback_colors[index]
        })
    })
  }

  /// Reports every key nobody asked for.
  fn finish(mut self) {
    let Some(table) = self.table else { return };
    for key in table.keys() {
      if !self.seen.contains(&key.as_str()) {
        self.report(key, Problem::UnknownKey);
      }
    }
  }
}

/// `#rrggbb` or `#rrggbbaa`.
fn parse_hex(text: &str) -> Option<Rgba> {
  let hex = text.strip_prefix('#')?;
  if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return None;
  }
  let value = u32::from_str_radix(hex, 16).ok()?;
  let [r, g, b, a] = if hex.len() == 6 {
    let [_, r, g, b] = value.to_be_bytes();
    [r, g, b, u8::MAX]
  } else {
    value.to_be_bytes()
  };
  Some(Rgba { r, g, b, a })
}
