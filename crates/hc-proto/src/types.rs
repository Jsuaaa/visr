//! Params and results of the herdr-core methods this client uses.
//! Only the fields we read are declared; serde skips the rest, so new core fields don't break us.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Free-form key-value tags on a terminal.
pub type Labels = BTreeMap<String, String>;

/// Params of `hello`.
#[derive(Debug, Clone, Serialize)]
pub struct HelloParams {
  pub generation: u32,
  pub framed: bool,
}

/// Result of `hello`.
#[derive(Debug, Clone, Deserialize)]
pub struct HelloResult {
  pub generation: u32,
  pub version: String,
  pub core: CoreInfo,
  pub framed: bool,
  pub methods: Vec<String>,
  pub features: Vec<String>,
}

/// Identity of the core.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CoreInfo {
  pub id: String,
  pub name: String,
}

/// Whether a terminal's program is still running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
  Live,
  Finished,
}

/// A terminal as the core describes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Terminal {
  pub address: String,
  pub name: Option<String>,
  pub argv: Vec<String>,
  pub cwd: String,
  pub state: TerminalState,
  pub labels: Labels,
  pub cols: u16,
  pub rows: u16,
  #[serde(default)]
  pub title: String,
  pub status: Option<Status>,
  pub foreground: Option<Foreground>,
  pub template: Option<String>,
  pub exit_code: Option<i32>,
  pub reason: Option<String>,
}

/// The program currently in the foreground of a terminal.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Foreground {
  pub pid: u32,
  pub name: String,
  pub argv: Vec<String>,
  pub cwd: String,
}

/// The status a terminal reports; `Unknown` covers states we don't know yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusState {
  Idle,
  Working,
  Done,
  Blocked,
  Error,
  #[serde(other)]
  Unknown,
}

/// A terminal's reported status.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Status {
  pub state: StatusState,
  #[serde(default)]
  pub title: String,
  #[serde(default)]
  pub msg: String,
  pub progress: Option<u8>,
}

/// Params for methods that take just a terminal address.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TerminalParams {
  pub terminal: String,
}

/// Params of `terminal.list`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ListParams {
  #[serde(skip_serializing_if = "std::ops::Not::not")]
  pub all: bool,
  #[serde(skip_serializing_if = "Labels::is_empty")]
  pub labels: Labels,
}

/// Result of `terminal.list`.
#[derive(Debug, Clone, Deserialize)]
pub struct ListResult {
  pub terminals: Vec<Terminal>,
  pub hidden_finished: u32,
}

/// A terminal size in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Size {
  pub cols: u16,
  pub rows: u16,
}

/// Params of `terminal.run`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RunParams {
  pub argv: Vec<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cwd: Option<String>,
  #[serde(skip_serializing_if = "Labels::is_empty")]
  pub labels: Labels,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub size: Option<Size>,
}

/// Result of removing a terminal.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoveResult {
  pub removed: String,
}

/// Params for changing a terminal's labels.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LabelParams {
  pub terminal: String,
  pub labels: Labels,
}

/// Params of `terminal.attach`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AttachParams {
  pub terminal: String,
  pub cols: u16,
  pub rows: u16,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub history: Option<u32>,
  #[serde(skip_serializing_if = "std::ops::Not::not")]
  pub observe: bool,
}

/// Result of `terminal.attach`.
#[derive(Debug, Clone, Deserialize)]
pub struct AttachResult {
  pub channel: u32,
  pub terminal: Terminal,
  pub automatic_input: bool,
  pub resize_notifications: bool,
}

/// Params of `events.subscribe`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SubscribeParams {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub terminal: Option<String>,
  #[serde(skip_serializing_if = "Labels::is_empty")]
  pub labels: Labels,
}

/// Result of `events.subscribe`.
#[derive(Debug, Clone, Deserialize)]
pub struct SubscribeResult {
  pub seq: u64,
  /// Framed mode only; in plain mode the whole connection becomes the event stream.
  pub channel: Option<u32>,
  pub terminals: Vec<Terminal>,
}

/// Params of an `event` notification.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "RawEvent")]
pub struct Event {
  pub seq: u64,
  pub terminal: String,
  pub change: Change,
}

/// An `event` notification before its change type is resolved.
#[derive(Deserialize)]
struct RawEvent {
  seq: u64,
  /// Absent on events about the core itself, like `doc`.
  #[serde(default)]
  terminal: String,
  #[serde(rename = "type")]
  kind: String,
  #[serde(default)]
  data: Value,
}

const KNOWN_CHANGES: [&str; 7] = [
  "created",
  "foreground",
  "labels",
  "finished",
  "title",
  "status",
  "doc",
];

impl TryFrom<RawEvent> for Event {
  type Error = serde_json::Error;

  // serde's `other` can't swallow a payload on adjacently tagged enums, so unknown types are
  // caught here.
  fn try_from(raw: RawEvent) -> Result<Self, Self::Error> {
    let change = if raw.kind == "removed" {
      // Arrives with `data: {}`, which a unit variant rejects.
      Change::Removed
    } else if raw.kind == "restarted" {
      Change::Restarted
    } else if KNOWN_CHANGES.contains(&raw.kind.as_str()) {
      serde_json::from_value(serde_json::json!({ "type": raw.kind, "data": raw.data }))?
    } else {
      Change::Other
    };
    Ok(Self {
      seq: raw.seq,
      terminal: raw.terminal,
      change,
    })
  }
}

/// What happened to a terminal, or to the core.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Change {
  Created(Box<Terminal>),
  Foreground(Foreground),
  Labels {
    labels: Labels,
  },
  Finished {
    exit_code: Option<i32>,
    reason: Option<String>,
    signal: Option<i32>,
  },
  Title {
    title: String,
  },
  /// `None` once the program exits or never said.
  Status {
    status: Option<Status>,
  },
  Removed,
  /// The core replaced a lost program; streams to it ended and must attach again.
  #[serde(skip)]
  Restarted,
  /// A document was written; the event has no terminal.
  Doc {
    name: String,
    version: u64,
  },
  /// A change type whose payload we haven't captured yet.
  #[serde(skip)]
  Other,
}

/// One template the core can start a terminal from.
#[derive(Debug, Clone, Deserialize)]
pub struct TemplateInfo {
  pub name: String,
  pub description: String,
  pub command: String,
}

/// Result of `templates.list`.
#[derive(Debug, Clone, Deserialize)]
pub struct TemplatesResult {
  pub templates: Vec<TemplateInfo>,
}

/// Params of `doc.get`.
#[derive(Debug, Clone, Serialize)]
pub struct DocGetParams {
  pub name: String,
}

/// Result of `doc.get`.
#[derive(Debug, Clone, Deserialize)]
pub struct DocGetResult {
  pub value: Value,
  pub version: u64,
}

/// Params of `doc.set`.
#[derive(Debug, Clone, Serialize)]
pub struct DocSetParams {
  pub name: String,
  pub value: Value,
  pub version: u64,
}

/// Result of `doc.set`.
#[derive(Debug, Clone, Deserialize)]
pub struct DocSetResult {
  pub version: u64,
}
