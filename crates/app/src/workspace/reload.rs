//! Applying `config.toml` while the app runs. There is no timer: the file is looked at when it
//! changes on disk, when the window becomes active, and when a terminal's program finishes (an
//! editor run in a pane ends), each look reading the file's modification time off the UI thread.

use std::path::Path;

use super::*;

impl Workspace {
  /// Looks at `config.toml` off the UI thread, and applies it if it changed since the last look.
  /// The system's reduce motion setting is read along with it, as it can change any time and
  /// reading it runs a process.
  pub(super) fn check_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let seen = self.config_seen;
    self._config_check = Some(cx.spawn_in(window, async move |this, cx| {
      let (now, loaded) = cx
        .background_executor()
        .spawn(async move {
          motion::check_reduce_motion();
          let path = config::config_path();
          let now = modified(&path).map_or(ConfigSeen::Missing, ConfigSeen::At);
          let changed = seen != ConfigSeen::NotYet && seen != now;
          (now, changed.then(|| Config::load(&path)))
        })
        .await;
      let _ = this.update_in(cx, |workspace, window, cx| {
        workspace.config_seen = now;
        if let Some(loaded) = loaded {
          workspace.apply_config(loaded, window, cx);
        }
      });
    }));
  }

  fn apply_config(
    &mut self,
    loaded: Result<Config, config::ConfigError>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    match loaded {
      Ok(config) => {
        self.config = config;
        self.config_error = None;
        for pane in self.panes.values() {
          pane
            .view
            .update(cx, |view, cx| view.set_config(&self.config, window, cx));
        }
        self.reload_theme(window, cx);
      }
      Err(error) => self.config_error = Some(error.to_string()),
    }
    cx.notify();
  }
}

/// When the file was last changed; `None` when it can't be read, as when it does not exist.
fn modified(path: &Path) -> Option<SystemTime> {
  std::fs::metadata(path)
    .and_then(|metadata| metadata.modified())
    .ok()
}
