//! File and folder icons from the Symbols theme for VS Code (MIT, by Miguel Solorio), resolved the
//! way VS Code does: a folder by its exact name; a file by its whole name, then its extensions
//! longest first, as `app.spec.ts` tries `spec.ts` before `ts`; the plain icon otherwise. The
//! editors' logos (Simple Icons, CC0) are served from here too, with their own notice in
//! `assets/icons/editors`.
//!
//! The SVGs in `assets/icons/symbols` and `table.rs` are generated and committed:
//! `node scripts/build-icons.mjs [git ref]` makes them again. Node is only for whoever does; it
//! is not needed to build Visr.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

mod table;

use table::{EXTENSIONS, FILE, FILE_NAMES, FOLDER, FOLDER_NAMES, SYMBOLS};

macro_rules! svgs {
  ($dir:literal: [$($name:literal),* $(,)?]) => {
    &[$((
      concat!("icons/", $dir, "/", $name, ".svg"),
      include_bytes!(concat!("../assets/icons/", $dir, "/", $name, ".svg")) as &[u8],
    )),*]
  };
}

const EDITORS: &[(&str, &[u8])] =
  svgs!("editors": ["cursor", "sublime", "vscodium", "windsurf", "zed"]);

/// A bundled file's bytes, by its asset path.
fn bytes(path: &str) -> Option<&'static [u8]> {
  SYMBOLS
    .binary_search_by_key(&path, |(known, _)| known)
    .ok()
    .map(|index| SYMBOLS[index].1)
    .or_else(|| {
      EDITORS
        .iter()
        .find(|(known, _)| *known == path)
        .map(|(_, bytes)| *bytes)
    })
}

/// The tables are sorted by key.
fn find(table: &[(&'static str, &'static str)], key: &str) -> Option<&'static str> {
  table
    .binary_search_by_key(&key, |(known, _)| known)
    .ok()
    .map(|index| table[index].1)
}

/// The asset path of a file's icon.
pub(crate) fn file(name: &str) -> SharedString {
  let lower = name.to_ascii_lowercase();
  let icon = find(FILE_NAMES, &lower)
    .or_else(|| {
      // Each dot starts a shorter extension: `app.spec.ts`, then `spec.ts`, then `ts`.
      lower
        .match_indices('.')
        .find_map(|(dot, _)| find(EXTENSIONS, &lower[dot + 1..]))
    })
    .unwrap_or(FILE);
  path(icon)
}

/// The asset path of a folder's icon.
pub(crate) fn folder(name: &str) -> SharedString {
  path(find(FOLDER_NAMES, &name.to_ascii_lowercase()).unwrap_or(FOLDER))
}

fn path(icon: &str) -> SharedString {
  format!("icons/symbols/{icon}.svg").into()
}

/// Serves the bundled icons to the window: the file icons, and the editors' logos.
pub(crate) struct Assets;

impl AssetSource for Assets {
  fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
    Ok(bytes(path).map(Cow::Borrowed))
  }

  fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
    Ok(Vec::new())
  }
}
