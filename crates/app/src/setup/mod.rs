//! First-run setup: when herdr-core is missing or out of date, offer to install it with its own
//! official installer.

mod install;
mod manifest;
mod view;

pub(crate) use view::{Launch, Setup, reason};
