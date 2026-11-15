//! File and folder icons from the Symbols theme for VS Code (MIT, by Miguel Solorio), resolved the
//! way VS Code does: a folder by its exact name; a file by its whole name, then its extensions
//! longest first, as `app.spec.ts` tries `spec.ts` before `ts`; the plain icon otherwise. The
//! theme's license is in `assets/icons/symbols`.
//!
//! The SVGs in `assets/icons/symbols` and `table.rs` are generated and committed:
//! `node scripts/build-icons.mjs [git ref]` makes them again. Node is only for whoever does; it
//! is not needed to build Visr.

use gpui::SharedString;

mod table;

use table::{EXTENSIONS, FILE, FILE_NAMES, FOLDER, FOLDER_NAMES, SYMBOLS};

/// A bundled symbol's bytes, by its asset path, as `file` and `folder` return them.
pub fn symbol(path: &str) -> Option<&'static [u8]> {
  SYMBOLS
    .binary_search_by_key(&path, |(known, _)| known)
    .ok()
    .map(|index| SYMBOLS[index].1)
}

/// The tables are sorted by key.
fn find(table: &[(&'static str, &'static str)], key: &str) -> Option<&'static str> {
  table
    .binary_search_by_key(&key, |(known, _)| known)
    .ok()
    .map(|index| table[index].1)
}

/// The asset path of a file's icon.
pub fn file(name: &str) -> SharedString {
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
pub fn folder(name: &str) -> SharedString {
  path(find(FOLDER_NAMES, &name.to_ascii_lowercase()).unwrap_or(FOLDER))
}

fn path(icon: &str) -> SharedString {
  format!("icons/symbols/{icon}.svg").into()
}
