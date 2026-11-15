//! The link to herdr-core and what comes over it.

use app_state::layout::Layout;
use hc_client::{Client, EventStream};
use hc_proto::types::{SubscribeParams, SubscribeResult, TemplateInfo, TemplatesResult};
use serde_json::Value;

/// The core's document holding the layout of tabs and splits.
pub(crate) const LAYOUT_DOC: &str = "visr.layout";

/// A live link to the core: the client, what it had when we subscribed, and its changes since.
/// The templates and the saved layout are fetched with the connection, so they are ready before
/// the window is built; callers open it off the UI thread, or before the window exists.
pub(crate) struct Connection {
  pub(crate) client: Client,
  pub(crate) snapshot: SubscribeResult,
  pub(crate) events: EventStream,
  pub(crate) templates: Vec<TemplateInfo>,
  /// The saved layout, `None` when never written or unreadable.
  pub(crate) saved_layout: Option<Layout>,
  /// The version of the layout document, for writing it back.
  pub(crate) layout_version: u64,
}

impl Connection {
  /// The link, and the startup data the window needs on top of it.
  pub(crate) fn open() -> Result<Self, hc_client::Error> {
    let (client, snapshot, events) = subscribe()?;
    let templates = client
      .call::<_, TemplatesResult>("templates.list", &serde_json::json!({}))
      .map(|result| result.templates)
      .unwrap_or_default();
    let (saved_layout, layout_version) = client.doc_get::<Layout>(LAYOUT_DOC).unwrap_or((None, 0));
    Ok(Self {
      client,
      snapshot,
      events,
      templates,
      saved_layout,
      layout_version,
    })
  }
}

/// Starts the client and subscribes to the core's snapshot and events. A reconnect needs no more
/// than this.
pub(crate) fn subscribe() -> Result<(Client, SubscribeResult, EventStream), hc_client::Error> {
  let client = Client::spawn()?;
  let (snapshot, events) = client.subscribe(&SubscribeParams::default())?;
  Ok((client, snapshot, events))
}

/// The fresh snapshot a `missed` notice carries, when it is in a shape we understand.
pub(crate) fn missed_snapshot(params: Value) -> Option<SubscribeResult> {
  serde_json::from_value(params).ok()
}
