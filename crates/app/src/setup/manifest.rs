//! The list of herdr-core releases, and whether the latest can be used here.

use serde_json::Value;

pub(super) const RELEASES_URL: &str = "https://releases.herdr.dev";
pub(super) const MANUAL_URL: &str = "https://herdr.dev";

/// The latest herdr-core release, as `latest.json` describes it.
#[derive(Debug, PartialEq)]
pub(super) struct Manifest {
  version: String,
  /// The protocol generation the release speaks.
  generation: u32,
  targets: Vec<String>,
}

impl Manifest {
  /// Reads `latest.json`; any other shape is `None`. The version ends up in the installer's
  /// environment, so only plain characters pass.
  pub(super) fn parse(text: &str) -> Option<Self> {
    let value: Value = serde_json::from_str(text).ok()?;
    let version = value.get("version")?.as_str()?;
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+');
    if version.is_empty() || !version.chars().all(plain) {
      return None;
    }
    let generation = u32::try_from(value.get("generation")?.as_u64()?).ok()?;
    let targets = value.get("targets")?.as_object()?.keys().cloned().collect();
    Some(Self {
      version: version.into(),
      generation,
      targets,
    })
  }
}

/// A machine's release target, from the names `std::env::consts` gives.
fn target(os: &str, arch: &str) -> Option<&'static str> {
  match (os, arch) {
    ("macos", "aarch64") => Some("macos-aarch64"),
    ("macos", "x86_64") => Some("macos-x86_64"),
    ("linux", "aarch64") => Some("linux-aarch64"),
    ("linux", "x86_64") => Some("linux-x86_64"),
    ("windows", "x86_64") => Some("windows-x86_64"),
    _ => None,
  }
}

/// What to install: the release's version, for this machine's target.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Plan {
  pub(super) version: String,
  pub(super) target: &'static str,
}

/// Why a release can't be installed here.
#[derive(Debug, PartialEq)]
pub(super) enum Refusal {
  /// The release speaks another protocol than this Visr does.
  Generation { theirs: u32, ours: u32 },
  /// There is no build for this machine.
  NoBuild { os: String, arch: String },
}

/// The build to install for this machine, or why there is none.
pub(super) fn check(manifest: &Manifest, ours: u32, os: &str, arch: &str) -> Result<Plan, Refusal> {
  let target = target(os, arch)
    .filter(|target| manifest.targets.iter().any(|known| known == target))
    .ok_or_else(|| Refusal::NoBuild {
      os: os.into(),
      arch: arch.into(),
    })?;
  if manifest.generation != ours {
    return Err(Refusal::Generation {
      theirs: manifest.generation,
      ours,
    });
  }
  Ok(Plan {
    version: manifest.version.clone(),
    target,
  })
}
