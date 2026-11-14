//! Framed mode of the herdr-core protocol, after `hello { framed: true }`.
//!
//! Every frame is `channel: u32 LE | kind: u8 | len: u32 LE | payload`.
//! Channel 0 carries JSON-RPC messages; `terminal.attach` opens the others.
//! Reverse-engineered from a captured `hc attach` session.

const HEADER_LEN: usize = 9;

/// What a frame's payload means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
  Data = 0,
  /// Hands back flow-control credit: the number of DATA bytes consumed.
  Credit = 1,
  Close = 2,
  Resize = 3,
  Focus = 4,
  /// Seen in the schema, payload not observed yet.
  Facts = 5,
  /// Input typed by a program rather than a person; see `automatic_input` in attach.
  AutomaticInput = 6,
}

impl FrameKind {
  fn from_byte(byte: u8) -> Option<Self> {
    match byte {
      0 => Some(Self::Data),
      1 => Some(Self::Credit),
      2 => Some(Self::Close),
      3 => Some(Self::Resize),
      4 => Some(Self::Focus),
      5 => Some(Self::Facts),
      6 => Some(Self::AutomaticInput),
      _ => None,
    }
  }
}

/// One frame on the wire: a channel, a kind and its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
  pub channel: u32,
  pub kind: FrameKind,
  pub payload: Vec<u8>,
}

impl Frame {
  /// A frame carrying bytes for the terminal or from it.
  pub fn data(channel: u32, payload: Vec<u8>) -> Self {
    Self {
      channel,
      kind: FrameKind::Data,
      payload,
    }
  }

  /// Gives the sender `bytes` more flow-control credit; send it after consuming DATA.
  pub fn credit(channel: u32, bytes: u32) -> Self {
    Self {
      channel,
      kind: FrameKind::Credit,
      payload: bytes.to_le_bytes().to_vec(),
    }
  }

  /// Ends the stream on `channel`.
  pub fn close(channel: u32) -> Self {
    Self {
      channel,
      kind: FrameKind::Close,
      payload: Vec::new(),
    }
  }

  /// Reports a new size in cells; the payload is `cols` then `rows`, both little-endian.
  pub fn resize(channel: u32, cols: u16, rows: u16) -> Self {
    let mut payload = cols.to_le_bytes().to_vec();
    payload.extend(rows.to_le_bytes());
    Self {
      channel,
      kind: FrameKind::Resize,
      payload,
    }
  }

  /// Tells the core whether the view on `channel` has focus.
  pub fn focus(channel: u32, focused: bool) -> Self {
    Self {
      channel,
      kind: FrameKind::Focus,
      payload: vec![focused as u8],
    }
  }

  /// The frame as bytes ready to write: header, then payload.
  pub fn encode(&self) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_LEN + self.payload.len());
    bytes.extend(self.channel.to_le_bytes());
    bytes.push(self.kind as u8);
    bytes.extend((self.payload.len() as u32).to_le_bytes());
    bytes.extend(&self.payload);
    bytes
  }
}

/// Buffers bytes as they arrive and yields whole frames.
#[derive(Debug, Default)]
pub struct FrameDecoder {
  buffer: Vec<u8>,
}

impl FrameDecoder {
  /// Appends bytes read from the connection; call `next_frame` to take whole frames out.
  pub fn push(&mut self, bytes: &[u8]) {
    self.buffer.extend_from_slice(bytes);
  }

  /// True when nothing is buffered, not even part of a frame.
  pub fn is_empty(&self) -> bool {
    self.buffer.is_empty()
  }

  /// The next whole frame. One of a kind this version does not know is skipped by its length,
  /// so a newer core adding kinds does not break the stream.
  pub fn next_frame(&mut self) -> Option<Frame> {
    loop {
      if self.buffer.len() < HEADER_LEN {
        return None;
      }
      let channel = u32::from_le_bytes(self.buffer[0..4].try_into().expect("a 4-byte slice"));
      let len = u32::from_le_bytes(self.buffer[5..9].try_into().expect("a 4-byte slice")) as usize;
      let frame_len = HEADER_LEN + len;
      if self.buffer.len() < frame_len {
        return None;
      }
      let kind = FrameKind::from_byte(self.buffer[4]);
      let payload = self.buffer[HEADER_LEN..frame_len].to_vec();
      self.buffer.drain(..frame_len);
      if let Some(kind) = kind {
        return Some(Frame {
          channel,
          kind,
          payload,
        });
      }
    }
  }
}
