//! Code editors found on this machine, and what opens a folder: the toolbar's "Open with".

use std::path::{Path, PathBuf};
use std::process::Command;

/// What to look for, in the order the toolbar prefers: the first one found is offered.
struct Candidate {
  name: &'static str,
  /// The commands it may install on the PATH: Zed is `zeditor` on some Linux distributions.
  commands: &'static [&'static str],
  mac_app: &'static str,
  /// Its executable under the Windows program folders.
  windows_exe: &'static str,
  /// Its logo in `assets/icons/editors`, when it has one, and its color.
  logo: Option<&'static str>,
  color: u32,
}

const CANDIDATES: &[Candidate] = &[
  Candidate {
    name: "Zed",
    commands: &["zed", "zeditor"],
    mac_app: "Zed.app",
    windows_exe: "Zed/Zed.exe",
    logo: Some("zed"),
    color: 0x3b82f6,
  },
  Candidate {
    name: "VS Code",
    commands: &["code"],
    mac_app: "Visual Studio Code.app",
    windows_exe: "Microsoft VS Code/Code.exe",
    logo: None,
    color: 0x23a9f2,
  },
  Candidate {
    name: "Cursor",
    commands: &["cursor"],
    mac_app: "Cursor.app",
    windows_exe: "cursor/Cursor.exe",
    logo: Some("cursor"),
    color: 0xe6e6e6,
  },
  Candidate {
    name: "Windsurf",
    commands: &["windsurf"],
    mac_app: "Windsurf.app",
    windows_exe: "Windsurf/Windsurf.exe",
    logo: Some("windsurf"),
    color: 0x2ed1b9,
  },
  Candidate {
    name: "VSCodium",
    commands: &["codium"],
    mac_app: "VSCodium.app",
    windows_exe: "VSCodium/VSCodium.exe",
    logo: Some("vscodium"),
    color: 0x2f80ed,
  },
  Candidate {
    name: "Sublime Text",
    commands: &["subl"],
    mac_app: "Sublime Text.app",
    windows_exe: "Sublime Text/sublime_text.exe",
    logo: Some("sublime"),
    color: 0xff9800,
  },
];

/// Where to look for editors, so the lookup does not depend on the real system.
#[derive(Debug, Default)]
struct Search {
  path: Vec<PathBuf>,
  /// Folders holding `.app` bundles (macOS).
  app_dirs: Vec<PathBuf>,
  /// Folders holding one folder per program (Windows).
  program_dirs: Vec<PathBuf>,
  /// Extensions a command may have on this system, in order of preference.
  extensions: Vec<&'static str>,
}

impl Search {
  fn system() -> Self {
    let path = std::env::var_os("PATH")
      .map(|path| std::env::split_paths(&path).collect())
      .unwrap_or_default();
    let mut app_dirs = Vec::new();
    let mut program_dirs = Vec::new();
    if cfg!(target_os = "macos") {
      app_dirs.push(PathBuf::from("/Applications"));
      app_dirs.extend(std::env::home_dir().map(|home| home.join("Applications")));
    }
    if cfg!(windows) {
      for (variable, subfolder) in [("LOCALAPPDATA", Some("Programs")), ("ProgramFiles", None)] {
        if let Some(root) = std::env::var_os(variable) {
          let root = PathBuf::from(root);
          program_dirs.push(match subfolder {
            Some(subfolder) => root.join(subfolder),
            None => root,
          });
        }
      }
    }
    // Windows editors ship an extensionless `bin/code` script (for WSL) beside `code.cmd`, which
    // is not something Windows can run, so only the runnable extensions count there.
    let extensions = if cfg!(windows) {
      vec![".exe", ".cmd"]
    } else {
      vec![""]
    };
    Self {
      path,
      app_dirs,
      program_dirs,
      extensions,
    }
  }
}

/// An editor found on this machine that can open a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Editor {
  pub(crate) name: &'static str,
  /// Its logo's path among the window's assets, if it has one.
  pub(crate) logo: Option<String>,
  pub(crate) color: u32,
  launch: Launch,
}

/// How an editor is started.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Launch {
  /// A macOS app, opened through `open -a`.
  App(PathBuf),
  Program(PathBuf),
}

/// The editors found, best first. An app or program folder wins over the PATH, which may only hold
/// a shell alias or another tool's wrapper.
fn find(search: &Search, exists: impl Fn(&Path) -> bool) -> Vec<Editor> {
  CANDIDATES
    .iter()
    .filter_map(|candidate| {
      let launch = search
        .app_dirs
        .iter()
        .map(|dir| dir.join(candidate.mac_app))
        .find(|path| exists(path))
        .map(Launch::App)
        .or_else(|| {
          search
            .program_dirs
            .iter()
            .map(|dir| dir.join(candidate.windows_exe))
            .find(|path| exists(path))
            .map(Launch::Program)
        })
        .or_else(|| {
          search
            .path
            .iter()
            .flat_map(|dir| {
              candidate.commands.iter().flat_map(move |command| {
                search
                  .extensions
                  .iter()
                  .map(move |extension| dir.join(format!("{command}{extension}")))
              })
            })
            .find(|path| exists(path))
            .map(Launch::Program)
        })?;
      Some(Editor {
        name: candidate.name,
        logo: candidate
          .logo
          .map(|logo| format!("icons/editors/{logo}.svg")),
        color: candidate.color,
        launch,
      })
    })
    .collect()
}

pub(crate) fn installed() -> Vec<Editor> {
  find(&Search::system(), |path| path.exists())
}

impl Editor {
  pub(crate) fn open(&self, folder: &Path) {
    let mut command = match &self.launch {
      Launch::App(app) => {
        let mut command = Command::new("open");
        command.arg("-a").arg(app);
        command
      }
      Launch::Program(program) => Command::new(program),
    };
    command.arg(folder);
    spawn(command);
  }
}

/// The system's file manager on `folder`.
pub(crate) fn open_folder(folder: &Path) {
  let program = if cfg!(target_os = "macos") {
    "open"
  } else if cfg!(windows) {
    "explorer"
  } else {
    "xdg-open"
  };
  let mut command = Command::new(program);
  command.arg(folder);
  spawn(command);
}

/// Windows opens a console window for a command-line program started by a GUI app, unless asked
/// not to.
pub(crate) fn no_console(command: &mut Command) {
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
  }
  #[cfg(not(windows))]
  let _ = command;
}

/// Starts it and lets it go: the app doesn't wait on an editor or a file manager.
fn spawn(mut command: Command) {
  no_console(&mut command);
  command
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
  if let Ok(mut child) = command.spawn() {
    std::thread::spawn(move || {
      let _ = child.wait();
    });
  }
}
