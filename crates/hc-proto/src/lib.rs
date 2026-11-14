//! The herdr-core wire format: how frames are cut, and the requests and replies they carry.

pub mod frame;
pub mod rpc;
pub mod types;

/// The herdr-core protocol generation this client speaks, sent in `hello`.
pub const PROTOCOL_GENERATION: u32 = 1;
