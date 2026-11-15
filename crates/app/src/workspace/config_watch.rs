//! Watches the folder of `config.toml` so edits apply as they are saved, with no polling.

use std::future::poll_fn;
use std::sync::{Arc, Mutex};
use std::task::{Poll, Waker};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::*;

/// Whether the file changed since the task last looked, and the task waiting to be told. A burst
/// of events (an editor writing, renaming) sets the flag once.
#[derive(Default)]
struct Signal {
  changed: bool,
  waker: Option<Waker>,
}

/// The running watch. Dropping it stops the watcher and cancels the task that reacts to it.
pub(super) struct ConfigWatch {
  _watcher: RecommendedWatcher,
  _task: Task<()>,
}

impl Workspace {
  /// Starts watching `config.toml`; each change looks at the file the way a window activation does.
  /// `None` when the system can't watch the folder: the window activation check still applies edits.
  pub(super) fn watch_config(window: &Window, cx: &mut Context<Self>) -> Option<ConfigWatch> {
    let path = config::config_path();
    let folder = path.parent()?;
    let signal = Arc::new(Mutex::new(Signal::default()));
    let mut watcher = notify::recommended_watcher({
      let signal = Arc::clone(&signal);
      let file_name = path.file_name()?.to_owned();
      move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        let touches_config = event
          .paths
          .iter()
          .any(|path| path.file_name() == Some(&file_name));
        if event.kind.is_access() || !touches_config {
          return;
        }
        let waker = {
          let mut signal = signal.lock().unwrap();
          signal.changed = true;
          signal.waker.take()
        };
        if let Some(waker) = waker {
          waker.wake();
        }
      }
    })
    .ok()?;
    // The folder, not the file: editors save by replacing the file, which ends a watch on it.
    watcher.watch(folder, RecursiveMode::NonRecursive).ok()?;

    let task = cx.spawn_in(window, async move |this, cx| {
      loop {
        poll_fn(|context| {
          let mut signal = signal.lock().unwrap();
          if std::mem::take(&mut signal.changed) {
            return Poll::Ready(());
          }
          signal.waker = Some(context.waker().clone());
          Poll::Pending
        })
        .await;
        let updated = this.update_in(cx, |workspace, window, cx| {
          workspace.check_config(window, cx)
        });
        if updated.is_err() {
          break;
        }
      }
    });
    Some(ConfigWatch {
      _watcher: watcher,
      _task: task,
    })
  }
}
