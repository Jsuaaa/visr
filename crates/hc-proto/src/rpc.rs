//! JSON-RPC 2.0 messages, one per line in plain mode or one per channel-0 frame in framed mode.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The JSON-RPC envelope of an outgoing request.
#[derive(Serialize)]
struct Request<'a, P> {
  jsonrpc: &'static str,
  id: u64,
  method: &'a str,
  params: &'a P,
}

/// One request as JSON, without a trailing newline.
pub fn request<P: Serialize>(id: u64, method: &str, params: &P) -> serde_json::Result<String> {
  serde_json::to_string(&Request {
    jsonrpc: "2.0",
    id,
    method,
    params,
  })
}

/// A message from the core: a reply to one of our requests, or a notification.
#[derive(Debug, PartialEq)]
pub enum Incoming {
  Response {
    id: u64,
    result: Result<Value, RpcError>,
  },
  Notification {
    method: String,
    params: Value,
  },
}

/// An error reply from the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcError {
  pub code: i64,
  pub message: String,
  /// herdr-core's stable `data.kind`; absent on plain JSON-RPC errors.
  pub kind: Option<ErrorKind>,
  pub hint: Option<String>,
}

/// The stable error kinds herdr-core reports in `data.kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
  NotFound,
  Finished,
  Unavailable,
  NeedsFramed,
  MachineUnknown,
  UnsupportedGeneration,
  NameTaken,
  InvalidParams,
  /// A document changed since the version the write was based on.
  Conflict,
  Other(String),
}

impl From<String> for ErrorKind {
  fn from(kind: String) -> Self {
    match kind.as_str() {
      "not_found" => Self::NotFound,
      "finished" => Self::Finished,
      "unavailable" => Self::Unavailable,
      "needs_framed" => Self::NeedsFramed,
      "machine_unknown" => Self::MachineUnknown,
      "unsupported_generation" => Self::UnsupportedGeneration,
      "name_taken" => Self::NameTaken,
      "invalid_params" => Self::InvalidParams,
      "conflict" => Self::Conflict,
      _ => Self::Other(kind),
    }
  }
}

/// An incoming message before it is told apart as response or notification.
#[derive(Deserialize)]
struct RawMessage {
  id: Option<u64>,
  method: Option<String>,
  #[serde(default)]
  params: Value,
  result: Option<Value>,
  error: Option<RawError>,
}

/// The `error` object of a response as it arrives.
#[derive(Deserialize)]
struct RawError {
  code: i64,
  message: String,
  #[serde(default)]
  data: RawErrorData,
}

/// The `data` object inside a response error.
#[derive(Deserialize, Default)]
struct RawErrorData {
  kind: Option<String>,
  hint: Option<String>,
}

/// Classifies one incoming message as a response or a notification.
pub fn parse(bytes: &[u8]) -> serde_json::Result<Incoming> {
  let raw: RawMessage = serde_json::from_slice(bytes)?;
  match (raw.id, raw.method) {
    (Some(id), None) => {
      let result = match raw.error {
        Some(error) => Err(RpcError {
          code: error.code,
          message: error.message,
          kind: error.data.kind.map(ErrorKind::from),
          hint: error.data.hint,
        }),
        None => Ok(raw.result.unwrap_or(Value::Null)),
      };
      Ok(Incoming::Response { id, result })
    }
    (_, Some(method)) => Ok(Incoming::Notification {
      method,
      params: raw.params,
    }),
    (None, None) => Err(serde::de::Error::custom(
      "neither a response nor a notification",
    )),
  }
}
