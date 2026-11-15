//! What to tell the person when the core can't be reached at launch.

use gpui::{Context, Window, div, prelude::*, px};
use hc_client::Error;
use hc_proto::rpc::ErrorKind;

use crate::terminal_view::{TerminalColors, hsla};

/// The whole window when there's no core to talk to.
pub(crate) struct StartupError {
  pub(crate) message: String,
}

impl Render for StartupError {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = TerminalColors::palette(cx);
    div()
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .p_8()
      .bg(hsla(palette.background))
      .text_color(hsla(palette.foreground))
      .text_size(px(15.0))
      .child(self.message.clone())
  }
}

/// The sentence shown when the core can't be reached, saying what to do next.
pub(crate) fn startup_message(error: &Error) -> String {
  match error {
    Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
      "herdr-core (`hc`) isn't installed or isn't on your PATH. Install it from https://herdr.dev, \
       then open Visr again."
        .into()
    }
    Error::Rpc(rpc) if rpc.kind == Some(ErrorKind::UnsupportedGeneration) => {
      "This herdr-core speaks a different protocol generation. Update it with `hc update`, \
       then open Visr again."
        .into()
    }
    other => format!("Couldn't reach herdr-core ({other:?}). Check it with `hc server status`."),
  }
}
