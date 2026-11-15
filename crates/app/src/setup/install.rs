//! Running herdr-core's official installer: fetching what it needs, running it, and passing on
//! what it says as it says it. Everything goes through programs every supported system has
//! (`curl`, `sh`, PowerShell), so no crate is added for it.

use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::manifest::{Manifest, Plan, RELEASES_URL};
use crate::editors::no_console;

/// Where the releases are. Tests and mirrors point it elsewhere (curl reads `file://` too); it is
/// also the one way a non-https address gets in.
const RELEASES_VARIABLE: &str = "VISR_HERDR_RELEASES_URL";

pub(super) fn releases_url() -> String {
  std::env::var(RELEASES_VARIABLE)
    .ok()
    .filter(|url| !url.is_empty())
    .map(|url| url.trim_end_matches('/').to_string())
    .unwrap_or_else(|| RELEASES_URL.to_string())
}

fn manifest_url(releases: &str) -> String {
  format!("{releases}/core/latest.json")
}

fn script_name(windows: bool) -> &'static str {
  if windows { "install.ps1" } else { "install.sh" }
}

pub(super) fn script_url(releases: &str, windows: bool) -> String {
  format!("{releases}/core/{}", script_name(windows))
}

/// Whether the releases are herdr's own server, not an override.
pub(super) fn is_official_releases(releases: &str) -> bool {
  releases == RELEASES_URL
}

/// Whether the address is herdr's own release server, the only place https is insisted on for.
fn is_official(url: &str) -> bool {
  url.starts_with(&format!("{RELEASES_URL}/"))
}

/// A redirect never leaves https, and the official server is only ever spoken to in https; an
/// overridden address (a mirror, a `file://` fixture) is allowed its own scheme.
fn curl_args(url: &str) -> Vec<String> {
  let mut args: Vec<String> = ["-fsSL", "--max-time", "60", "--proto-redir", "=https"]
    .map(String::from)
    .to_vec();
  if is_official(url) {
    args.extend(["--proto".to_string(), "=https".to_string()]);
  }
  args.push(url.to_string());
  args
}

fn curl(url: &str) -> Result<Vec<u8>, String> {
  let program = if cfg!(windows) { "curl.exe" } else { "curl" };
  let mut command = Command::new(program);
  no_console(&mut command);
  let output = command
    .args(curl_args(url))
    .stdin(Stdio::null())
    .output()
    .map_err(|error| format!("couldn't run {program}: {error}"))?;
  if output.status.success() {
    return Ok(output.stdout);
  }
  let said = String::from_utf8_lossy(&output.stderr);
  let reason = said.lines().last().unwrap_or("").trim();
  Err(format!("couldn't download {url}: {reason}"))
}

pub(super) fn fetch_manifest(releases: &str) -> Result<Manifest, String> {
  let bytes = curl(&manifest_url(releases))?;
  String::from_utf8(bytes)
    .ok()
    .and_then(|text| Manifest::parse(&text))
    .ok_or_else(|| "couldn't read the list of herdr-core releases".to_string())
}

/// A folder only this person can read, to keep the installer in while it runs.
pub(super) fn private_dir() -> io::Result<PathBuf> {
  let nanos = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |since| since.as_nanos());
  let dir = std::env::temp_dir().join(format!("visr-hc-install-{}-{nanos}", std::process::id()));
  // Only the unix permission call needs it mutable.
  #[cfg_attr(not(unix), allow(unused_mut))]
  let mut builder = std::fs::DirBuilder::new();
  #[cfg(unix)]
  std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
  builder.create(&dir)?;
  Ok(dir)
}

/// Saves the installer in `dir`, to run from a file rather than piped from the network.
pub(super) fn download_script(url: &str, dir: &Path, windows: bool) -> Result<PathBuf, String> {
  let bytes = curl(url)?;
  let path = dir.join(script_name(windows));
  let mut options = std::fs::OpenOptions::new();
  options.write(true).create_new(true);
  #[cfg(unix)]
  std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
  options
    .open(&path)
    .and_then(|mut file| io::Write::write_all(&mut file, &bytes))
    .map_err(|error| format!("couldn't save the installer: {error}"))?;
  Ok(path)
}

/// The installer, run from `script`, for the version that was checked.
pub(super) fn install_command(
  plan: &Plan,
  windows: bool,
  script: &Path,
  releases_override: Option<&str>,
) -> Command {
  let mut command = if windows {
    let mut command = Command::new("powershell");
    command
      .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
      .arg(script);
    command
  } else {
    let mut command = Command::new("sh");
    command.arg(script);
    command
  };
  command.env("HC_VERSION", &plan.version);
  if let Some(releases) = releases_override {
    command.env("HERDR_RELEASES_URL", releases);
  }
  no_console(&mut command);
  command
}

/// The installer changes what it does as root, and a window should not install for the system.
pub(super) fn running_as_root() -> bool {
  if cfg!(windows) {
    return false;
  }
  Command::new("id")
    .arg("-u")
    .output()
    .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "0")
}

/// What a running program reports back.
pub(super) enum Event {
  /// A line of output, from either stream.
  Line(String),
  /// The program is over, and every line it wrote has been sent.
  Finished(io::Result<ExitStatus>),
}

/// Stops a running program and everything it started: the installer's own `curl`, `tar` and
/// installing steps would otherwise carry on after it.
pub(super) struct Cancel {
  child: Arc<Mutex<Child>>,
  pid: u32,
}

impl Cancel {
  pub(super) fn cancel(&self) {
    #[cfg(unix)]
    {
      // The program leads its own process group, so the group is all of it.
      let _ = Command::new("kill")
        .args(["-TERM", "--", &format!("-{}", self.pid)])
        .status();
    }
    #[cfg(windows)]
    {
      let mut command = Command::new("taskkill");
      no_console(&mut command);
      let _ = command
        .args(["/T", "/F", "/PID", &self.pid.to_string()])
        .status();
    }
    let _ = self.child.lock().unwrap().kill();
  }
}

/// Starts `command` and sends what it writes, line by line, as it writes it.
pub(super) fn run(mut command: Command) -> io::Result<(Receiver<Event>, Cancel)> {
  #[cfg(unix)]
  std::os::unix::process::CommandExt::process_group(&mut command, 0);
  let mut child = command
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
  let streams: [Box<dyn Read + Send>; 2] = [
    Box::new(child.stdout.take().expect("stdout is piped")),
    Box::new(child.stderr.take().expect("stderr is piped")),
  ];
  let pid = child.id();
  let child = Arc::new(Mutex::new(child));
  let (sender, receiver) = channel();
  let readers: Vec<_> = streams
    .into_iter()
    .map(|stream| {
      let sender = sender.clone();
      thread::spawn(move || {
        for line in BufReader::new(stream).split(b'\n') {
          let Ok(bytes) = line else { break };
          let text = String::from_utf8_lossy(&bytes)
            .trim_end_matches('\r')
            .to_string();
          if sender.send(Event::Line(text)).is_err() {
            break;
          }
        }
      })
    })
    .collect();
  let waited = Arc::clone(&child);
  thread::spawn(move || {
    // Polled, so that cancelling can take the child's lock in between.
    let status = loop {
      match waited.lock().unwrap().try_wait() {
        Ok(Some(status)) => break Ok(status),
        Ok(None) => {}
        Err(error) => break Err(error),
      }
      thread::sleep(Duration::from_millis(50));
    };
    for reader in readers {
      let _ = reader.join();
    }
    let _ = sender.send(Event::Finished(status));
  });
  Ok((receiver, Cancel { child, pid }))
}
