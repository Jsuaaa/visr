//! The assets the window serves: the file icons from `ui`, and the editors' logos (Simple Icons,
//! CC0) with their own notice in `assets/icons/editors`.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

macro_rules! editors {
  ($($name:literal),* $(,)?) => {
    &[$((
      concat!("icons/editors/", $name, ".svg"),
      include_bytes!(concat!("../assets/icons/editors/", $name, ".svg")) as &[u8],
    )),*]
  };
}

const EDITORS: &[(&str, &[u8])] = editors!["cursor", "sublime", "vscodium", "windsurf", "zed"];

/// Serves the bundled icons to the window: the file icons, then the editors' logos.
pub(crate) struct Assets;

impl AssetSource for Assets {
  fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
    let bytes = ui::file_icons::symbol(path).or_else(|| {
      EDITORS
        .iter()
        .find(|(known, _)| *known == path)
        .map(|(_, bytes)| *bytes)
    });
    Ok(bytes.map(Cow::Borrowed))
  }

  fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
    Ok(Vec::new())
  }
}
