//! The `visr` binary: opens the window and wires the app together.

// Release builds on Windows open no console window next to the app.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod connection;
mod edit;
mod editors;
mod file_icons;
mod git;
mod input;
mod setup;
mod startup;
mod terminal_view;
mod text_field;
mod tree;
mod workspace;

use std::borrow::Cow;

use config::Config;
use connection::Connection;
use gpui::{
  App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, prelude::*, px,
  size,
};
use setup::{Launch, Setup};
use startup::{StartupError, startup_message};
use workspace::Workspace;

const SIDEBAR_WIDTH: f32 = 220.0;
/// Height of the strip the macOS traffic lights sit in, above the sidebar.
const TITLEBAR_HEIGHT: f32 = 40.0;

/// The default font ships inside the binary, so every machine renders the same without installing
/// anything. Licensed under the SIL OFL 1.1: see assets/fonts/LICENSE.txt, and NOTICE.txt for the
/// Nerd Fonts glyphs.
const FONTS: [&[u8]; 4] = [
  include_bytes!("../assets/fonts/LilexNerdFontMono-Regular.ttf"),
  include_bytes!("../assets/fonts/LilexNerdFontMono-Bold.ttf"),
  include_bytes!("../assets/fonts/LilexNerdFontMono-Italic.ttf"),
  include_bytes!("../assets/fonts/LilexNerdFontMono-BoldItalic.ttf"),
];

fn main() {
  ui::motion::check_reduce_motion();
  let connection = Connection::open();
  let initial = std::env::args().nth(1);
  let (config, config_error) = match Config::load(&config::config_path()) {
    Ok(config) => (config, None),
    Err(error) => (Config::defaults(), Some(error.to_string())),
  };

  Application::new()
    .with_assets(file_icons::Assets)
    .run(move |cx: &mut App| {
      let fonts = FONTS.iter().map(|font| Cow::Borrowed(*font)).collect();
      cx.text_system()
        .add_fonts(fonts)
        .expect("load the bundled fonts");
      cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
          cx.quit();
        }
      })
      .detach();
      let bounds = Bounds::centered(None, size(px(1100.0), px(700.0)), cx);
      let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
          title: Some("Visr".into()),
          // Windows keeps its native title bar: drawing our own needs the caption buttons too.
          appears_transparent: cfg!(target_os = "macos"),
          traffic_light_position: Some(point(px(14.0), px(workspace::theme::TRAFFIC_LIGHTS_TOP))),
        }),
        ..Default::default()
      };
      let opened = match connection {
        Ok(connection) => cx
          .open_window(options, |window, cx| {
            cx.new(|cx| Workspace::new(connection, initial, config, config_error, window, cx))
          })
          .map(|_| ()),
        // Missing or out of date, herdr-core is something Visr can install.
        Err(error) => match setup::reason(&error) {
          Some(reason) => {
            let launch = Launch {
              initial,
              config,
              config_error,
            };
            cx.open_window(options, |window, cx| {
              cx.new(|cx| Setup::new(reason, launch, window, cx))
            })
            .map(|_| ())
          }
          None => {
            let message = startup_message(&error);
            cx.open_window(options, |_, cx| cx.new(|_| StartupError { message }))
              .map(|_| ())
          }
        },
      };
      opened.expect("open the window");
      cx.activate(true);
    });
}
