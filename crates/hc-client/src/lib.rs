//! A blocking client for herdr-core over one framed connection.
//!
//! A reader thread decodes frames and routes them: channel 0 answers pending calls,
//! other channels feed attachments and event streams. Everything else uses std threads
//! and channels, so the UI can drive it from any executor.

mod chan;

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hc_proto::PROTOCOL_GENERATION;
use hc_proto::frame::{Frame, FrameDecoder, FrameKind};
use hc_proto::rpc::{ErrorKind, Incoming, RpcError, parse, request};
use hc_proto::types::{
  AttachParams, AttachResult, DocGetParams, DocGetResult, DocSetParams, DocSetResult, Event,
  HelloParams, HelloResult, SubscribeParams, SubscribeResult,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// How long a call, or the handshake, waits for the core. A core that stays silent this long is
/// taken as lost: the bridge is closed, and everything waiting on it hears a disconnect.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// How many times longer attaching may take than a cheap call.
const ATTACH_TIMEOUT_FACTOR: u32 = 6;

/// Everything a call to the core can fail with.
#[derive(Debug)]
pub enum Error {
  /// Reading or writing the connection failed.
  Io(io::Error),
  /// A message could not be encoded or decoded.
  Json(serde_json::Error),
  /// The core answered a request with an error.
  Rpc(RpcError),
  /// The core did not answer `hello` as expected, or did not switch to framed mode.
  Handshake(String),
  /// The core sent something the protocol does not allow at that point.
  Protocol(String),
  /// The connection is over, so nothing more will arrive.
  Disconnected,
  /// The core did not answer in time.
  Timeout,
}

impl Error {
  /// A write that lost to a newer version of the document.
  pub fn is_conflict(&self) -> bool {
    matches!(self, Self::Rpc(error) if error.kind == Some(ErrorKind::Conflict))
  }
}

impl From<io::Error> for Error {
  fn from(error: io::Error) -> Self {
    Self::Io(error)
  }
}

impl From<serde_json::Error> for Error {
  fn from(error: serde_json::Error) -> Self {
    Self::Json(error)
  }
}

enum ChannelMessage {
  Data(Vec<u8>),
  Closed,
}

/// Output, a close or a disconnect can come before the response that names the channel,
/// so whoever gets there first makes the slot and the receiver is claimed later.
/// A slot without a sender has ended; it stays until its receiver is claimed.
struct Slot {
  sender: Option<chan::Sender<ChannelMessage>>,
  receiver: Option<chan::Receiver<ChannelMessage>>,
}

impl Slot {
  fn new(disconnected: bool) -> Self {
    let (sender, receiver) = chan::channel();
    Self {
      sender: (!disconnected).then_some(sender),
      receiver: Some(receiver),
    }
  }
}

#[derive(Default)]
struct State {
  pending: HashMap<u64, Sender<Result<Value, RpcError>>>,
  slots: HashMap<u32, Slot>,
  disconnected: bool,
}

struct Shared {
  writer: Mutex<Box<dyn Write + Send>>,
  state: Mutex<State>,
}

impl Shared {
  fn send(&self, frame: Frame) -> Result<(), Error> {
    let mut writer = self.writer.lock().unwrap();
    writer.write_all(&frame.encode())?;
    writer.flush()?;
    Ok(())
  }

  fn claim(&self, channel: u32) -> chan::Receiver<ChannelMessage> {
    let mut state = self.state.lock().unwrap();
    let disconnected = state.disconnected;
    let slot = state
      .slots
      .entry(channel)
      .or_insert_with(|| Slot::new(disconnected));
    let receiver = slot.receiver.take().expect("channel claimed twice");
    if slot.sender.is_none() {
      state.slots.remove(&channel);
    }
    receiver
  }
}

/// The state of a connection that is over: nobody is waiting any more, and nothing new is.
fn mark_disconnected(state: &mut State) {
  // Dropping the senders wakes every waiter with a disconnect.
  state.disconnected = true;
  state.pending.clear();
  state.slots.retain(|_, slot| slot.receiver.is_some());
  for slot in state.slots.values_mut() {
    slot.sender = None;
  }
}

/// A live connection to herdr-core. Calls block until the core answers or the timeout passes.
pub struct Client {
  shared: Arc<Shared>,
  next_id: AtomicU64,
  hello: HelloResult,
  /// The `hc connect` bridge, when this client started it.
  child: Mutex<Option<Child>>,
  timeout: Duration,
}

impl Client {
  /// Starts `hc connect` and talks to the core through its stdin and stdout.
  pub fn spawn() -> Result<Self, Error> {
    let (home, local_app_data) = installer_roots();
    Self::spawn_from(hc_candidates(home.as_deref(), local_app_data.as_deref()))
  }

  /// The first of `candidates` that starts and speaks the protocol. One that is missing, or is
  /// some other tool with the same name, hands over to the next. When none works the answer is
  /// the first failure of a program that did start, so an installed core that only failed to
  /// answer is never reported as missing; `NotFound` is for when nothing started at all.
  fn spawn_from(candidates: Vec<PathBuf>) -> Result<Self, Error> {
    let mut failure: Option<Error> = None;
    let mut missing: Option<Error> = None;
    for program in candidates {
      let mut command = Command::new(&program);
      no_console(&mut command);
      let spawned = command
        .arg("connect")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn();
      let child = match spawned {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
          missing = Some(error.into());
          continue;
        }
        Err(error) => return Err(error.into()),
      };
      match Self::over_child(child) {
        Ok(client) => return Ok(client),
        Err(error) if is_foreign(&error) => {
          failure.get_or_insert(error);
        }
        // A core that answered, or took too long: a slow real core is not skipped.
        Err(error) => return Err(error),
      }
    }
    Err(
      failure
        .or(missing)
        .expect("there is always a first candidate"),
    )
  }

  /// Talks to the core through the child's stdin and stdout. When that fails, the child is
  /// killed rather than left running: a reconnect loop would otherwise leave one per try.
  fn over_child(mut child: Child) -> Result<Self, Error> {
    let stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    match Self::connect(stdout, stdin) {
      Ok(client) => {
        *client.child.lock().unwrap() = Some(child);
        Ok(client)
      }
      Err(error) => {
        let _ = child.kill();
        let _ = child.wait();
        Err(error)
      }
    }
  }

  /// Says `hello` in plain mode, switches to framed mode and starts the reader thread. The core
  /// gets `DEFAULT_TIMEOUT` to answer, here and on every later call.
  fn connect<R, W>(reader: R, mut writer: W) -> Result<Self, Error>
  where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
  {
    let timeout = DEFAULT_TIMEOUT;
    let params = HelloParams {
      generation: PROTOCOL_GENERATION,
      framed: true,
    };
    writer.write_all((request(0, "hello", &params)? + "\n").as_bytes())?;
    writer.flush()?;

    let shared = Arc::new(Shared {
      writer: Mutex::new(Box::new(writer)),
      state: Mutex::new(State::default()),
    });
    // The reader thread reads the answer too, so a core that never answers leaves nothing but
    // that one thread, which ends when the pipe does.
    let (hello_sender, hello_receiver) = channel();
    let reader_shared = Arc::clone(&shared);
    thread::spawn(move || {
      let mut reader = BufReader::new(reader);
      match read_hello(&mut reader) {
        Ok(hello) => {
          let _ = hello_sender.send(Ok(hello));
          read_frames(reader, &reader_shared);
        }
        Err(error) => {
          let _ = hello_sender.send(Err(error));
        }
      }
    });
    let hello = match hello_receiver.recv_timeout(timeout) {
      Ok(hello) => hello?,
      Err(RecvTimeoutError::Timeout) => return Err(Error::Timeout),
      Err(RecvTimeoutError::Disconnected) => return Err(Error::Disconnected),
    };

    Ok(Self {
      shared,
      next_id: AtomicU64::new(1),
      hello,
      child: Mutex::new(None),
      timeout,
    })
  }

  /// The core stopped answering: closes the bridge, which ends the reader and wakes everything
  /// waiting, and refuses new calls at once.
  fn give_up(&self) {
    if let Some(child) = self.child.lock().unwrap().as_mut() {
      let _ = child.kill();
    }
    mark_disconnected(&mut self.shared.state.lock().unwrap());
  }

  /// What the core reported in the handshake.
  pub fn hello(&self) -> &HelloResult {
    &self.hello
  }

  /// Sends one request and waits for its typed result.
  pub fn call<P: Serialize, R: DeserializeOwned>(
    &self,
    method: &str,
    params: &P,
  ) -> Result<R, Error> {
    self.call_within(method, params, self.timeout)
  }

  /// `call`, waiting `timeout` for the answer instead of the usual.
  fn call_within<P: Serialize, R: DeserializeOwned>(
    &self,
    method: &str,
    params: &P,
    timeout: Duration,
  ) -> Result<R, Error> {
    let id = self.next_id.fetch_add(1, Ordering::Relaxed);
    let payload = request(id, method, params)?.into_bytes();
    let (sender, receiver) = channel();
    {
      let mut state = self.shared.state.lock().unwrap();
      if state.disconnected {
        return Err(Error::Disconnected);
      }
      state.pending.insert(id, sender);
    }
    if let Err(error) = self.shared.send(Frame::data(0, payload)) {
      self.shared.state.lock().unwrap().pending.remove(&id);
      return Err(error);
    }
    match receiver.recv_timeout(timeout) {
      Ok(Ok(value)) => Ok(serde_json::from_value(value)?),
      Ok(Err(error)) => Err(Error::Rpc(error)),
      Err(RecvTimeoutError::Timeout) => {
        self.give_up();
        Err(Error::Timeout)
      }
      Err(RecvTimeoutError::Disconnected) => Err(Error::Disconnected),
    }
  }

  /// A document kept in the core, and its version: `None` at version 0 when never written.
  pub fn doc_get<T: DeserializeOwned>(&self, name: &str) -> Result<(Option<T>, u64), Error> {
    let params = DocGetParams { name: name.into() };
    let doc: DocGetResult = self.call("doc.get", &params)?;
    let value = match doc.value {
      Value::Null => None,
      value => Some(serde_json::from_value(value)?),
    };
    Ok((value, doc.version))
  }

  /// Writes it if `version` is still current; returns the new version.
  pub fn doc_set<T: Serialize>(&self, name: &str, value: &T, version: u64) -> Result<u64, Error> {
    let params = DocSetParams {
      name: name.into(),
      value: serde_json::to_value(value)?,
      version,
    };
    let doc: DocSetResult = self.call("doc.set", &params)?;
    Ok(doc.version)
  }

  pub fn attach(&self, params: &AttachParams) -> Result<Attachment, Error> {
    // Attaching replays the terminal's history, so on a loaded machine it is the one call that can
    // honestly be slow; timing it out would drop the connection and every pane with it.
    let info: AttachResult = self.call_within(
      "terminal.attach",
      params,
      self.timeout * ATTACH_TIMEOUT_FACTOR,
    )?;
    let receiver = self.shared.claim(info.channel);
    Ok(Attachment {
      channel: info.channel,
      info,
      receiver,
      shared: Arc::clone(&self.shared),
    })
  }

  /// The snapshot of matching terminals, then their changes in order.
  pub fn subscribe(
    &self,
    params: &SubscribeParams,
  ) -> Result<(SubscribeResult, EventStream), Error> {
    let snapshot: SubscribeResult = self.call("events.subscribe", params)?;
    let channel = snapshot
      .channel
      .ok_or_else(|| Error::Protocol("framed subscribe without a channel".into()))?;
    let receiver = self.shared.claim(channel);
    let stream = EventStream {
      channel,
      receiver,
      buffer: Vec::new(),
      shared: Arc::clone(&self.shared),
    };
    Ok((snapshot, stream))
  }
}

impl Drop for Client {
  fn drop(&mut self) {
    // Only ends the `hc connect` bridge; terminals live in the core.
    if let Some(child) = self.child.lock().unwrap().as_mut() {
      let _ = child.kill();
      let _ = child.wait();
    }
  }
}

#[cfg(windows)]
const HC_FILE_NAME: &str = "hc.exe";
#[cfg(not(windows))]
const HC_FILE_NAME: &str = "hc";
#[cfg(windows)]
const CORE_FILE_NAME: &str = "herdr-core.exe";
#[cfg(not(windows))]
const CORE_FILE_NAME: &str = "herdr-core";

/// Where to look for `hc`: the PATH, then where the installers put it, which apps opened from the
/// Finder or the Start menu don't have on their PATH, and which a running app that has just
/// installed it never will: `~/.local/bin`, and `%LOCALAPPDATA%\Programs\herdr-core` on Windows.
/// The installers name the program `herdr-core` and link `hc` to it only when no `hc` exists.
fn hc_candidates(home: Option<&Path>, local_app_data: Option<&Path>) -> Vec<PathBuf> {
  let mut candidates = vec![PathBuf::from("hc")];
  for folder in install_folders(home, local_app_data) {
    candidates.push(folder.join(HC_FILE_NAME));
    candidates.push(folder.join(CORE_FILE_NAME));
  }
  candidates
}

/// Where the installers put the program: `~/.local/bin`, and on Windows
/// `%LOCALAPPDATA%\Programs\herdr-core`.
fn install_folders(home: Option<&Path>, local_app_data: Option<&Path>) -> Vec<PathBuf> {
  let mut folders = Vec::new();
  if let Some(home) = home {
    folders.push(home.join(".local").join("bin"));
  }
  if let Some(local_app_data) = local_app_data {
    folders.push(local_app_data.join("Programs").join("herdr-core"));
  }
  folders
}

/// The home folder and `%LOCALAPPDATA%` of this process, where the installers' folders hang from.
fn installer_roots() -> (Option<PathBuf>, Option<PathBuf>) {
  (
    std::env::home_dir(),
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
  )
}

/// Runs herdr-core with `args` and waits for it. It is looked for as `herdr-core`, never as `hc`:
/// that name may be some other tool's, and nothing here checks what answers to it.
pub fn run_hc(args: &[&str]) -> io::Result<std::process::ExitStatus> {
  let (home, local_app_data) = installer_roots();
  run_from(
    core_candidates(home.as_deref(), local_app_data.as_deref()),
    args,
  )
}

/// Where `herdr-core` may be: the installers' folders, where one just installed is, then the PATH.
fn core_candidates(home: Option<&Path>, local_app_data: Option<&Path>) -> Vec<PathBuf> {
  let mut candidates: Vec<PathBuf> = install_folders(home, local_app_data)
    .into_iter()
    .map(|folder| folder.join(CORE_FILE_NAME))
    .collect();
  candidates.push(PathBuf::from(CORE_FILE_NAME));
  candidates
}

/// Windows opens a console window for a console program started by an app without one, and for
/// `hc connect` it would stay open for the whole session.
fn no_console(command: &mut Command) {
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
  }
  #[cfg(not(windows))]
  let _ = command;
}

fn run_from(candidates: Vec<PathBuf>, args: &[&str]) -> io::Result<std::process::ExitStatus> {
  let mut missing = None;
  for program in candidates {
    let mut command = Command::new(&program);
    no_console(&mut command);
    let ran = command
      .args(args)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status();
    match ran {
      Err(error) if error.kind() == io::ErrorKind::NotFound => missing = Some(error),
      other => return other,
    }
  }
  Err(missing.expect("there is always a first candidate"))
}

/// Whether the failure says the program is not a core: another tool called `hc` closes the pipe
/// or answers with something that isn't the protocol. A timeout is not that: a core can be slow.
fn is_foreign(error: &Error) -> bool {
  matches!(
    error,
    Error::Io(_) | Error::Json(_) | Error::Handshake(_) | Error::Protocol(_) | Error::Disconnected
  )
}

/// Reads the plain-mode answer to `hello`.
fn read_hello(reader: &mut impl BufRead) -> Result<HelloResult, Error> {
  let mut line = String::new();
  if reader.read_line(&mut line)? == 0 {
    return Err(Error::Disconnected);
  }
  let hello: HelloResult = match parse(line.as_bytes())? {
    Incoming::Response {
      id: 0,
      result: Ok(value),
    } => serde_json::from_value(value)?,
    Incoming::Response {
      id: 0,
      result: Err(error),
    } => return Err(Error::Rpc(error)),
    other => {
      return Err(Error::Handshake(format!(
        "unexpected reply to hello: {other:?}"
      )));
    }
  };
  if !hello.framed {
    return Err(Error::Handshake(
      "core did not switch to framed mode".into(),
    ));
  }
  Ok(hello)
}

fn read_frames(mut reader: impl Read, shared: &Shared) {
  let mut decoder = FrameDecoder::default();
  let mut buffer = [0u8; 64 * 1024];
  loop {
    let read = match reader.read(&mut buffer) {
      Ok(0) | Err(_) => break,
      Ok(read) => read,
    };
    decoder.push(&buffer[..read]);
    while let Some(frame) = decoder.next_frame() {
      route(frame, shared);
    }
  }
  mark_disconnected(&mut shared.state.lock().unwrap());
}

fn route(frame: Frame, shared: &Shared) {
  let mut state = shared.state.lock().unwrap();
  if frame.channel == 0 {
    if let Ok(Incoming::Response { id, result }) = parse(&frame.payload)
      && let Some(sender) = state.pending.remove(&id)
    {
      let _ = sender.send(result);
    }
    return;
  }
  let message = match frame.kind {
    FrameKind::Data => ChannelMessage::Data(frame.payload),
    FrameKind::Close => ChannelMessage::Closed,
    _ => return,
  };
  let closing = matches!(message, ChannelMessage::Closed);
  let slot = state
    .slots
    .entry(frame.channel)
    .or_insert_with(|| Slot::new(false));
  if let Some(sender) = &slot.sender {
    sender.send(message);
  }
  if closing {
    slot.sender = None;
    if slot.receiver.is_none() {
      state.slots.remove(&frame.channel);
    }
  }
}

/// Best effort: bytes already received are still handed over if the core is gone,
/// and the disconnect shows up on the next read.
fn give_credit(shared: &Shared, channel: u32, bytes: usize) {
  let _ = shared.send(Frame::credit(channel, bytes as u32));
}

/// A live stream to one terminal.
pub struct Attachment {
  /// What the core said about the terminal when attaching.
  pub info: AttachResult,
  channel: u32,
  receiver: chan::Receiver<ChannelMessage>,
  shared: Arc<Shared>,
}

impl Attachment {
  /// Blocks for the next output; `None` once the core closes the stream.
  /// Credit goes back only for what was taken, so a slow reader slows the core down.
  pub fn recv(&self) -> Result<Option<Vec<u8>>, Error> {
    self.taken(self.receiver.recv())
  }

  /// `recv`, awaited: the task sleeps without holding a thread.
  pub async fn recv_async(&self) -> Result<Option<Vec<u8>>, Error> {
    self.taken(self.receiver.recv_async().await)
  }

  fn taken(&self, message: Result<ChannelMessage, chan::Closed>) -> Result<Option<Vec<u8>>, Error> {
    match message {
      Ok(ChannelMessage::Data(bytes)) => {
        give_credit(&self.shared, self.channel, bytes.len());
        Ok(Some(bytes))
      }
      Ok(ChannelMessage::Closed) => Ok(None),
      Err(_) => Err(Error::Disconnected),
    }
  }

  /// Sends keystrokes or pasted bytes to the terminal.
  pub fn send_input(&self, bytes: &[u8]) -> Result<(), Error> {
    self.shared.send(Frame::data(self.channel, bytes.to_vec()))
  }

  /// Tells the terminal its new size in cells.
  pub fn resize(&self, cols: u16, rows: u16) -> Result<(), Error> {
    self.shared.send(Frame::resize(self.channel, cols, rows))
  }

  /// Tells the core whether this view has focus.
  pub fn focus(&self, focused: bool) -> Result<(), Error> {
    self.shared.send(Frame::focus(self.channel, focused))
  }

  /// Stops this stream; the terminal itself keeps running in the core.
  pub fn close(&self) -> Result<(), Error> {
    self.shared.send(Frame::close(self.channel))
  }
}

/// One message from an event stream.
#[derive(Debug)]
pub enum Notice {
  Event(Event),
  Heartbeat,
  /// The stream fell behind; params carry a fresh snapshot.
  Missed(Value),
  Other {
    method: String,
    params: Value,
  },
}

/// Event notifications arriving as JSON lines on their own channel.
pub struct EventStream {
  channel: u32,
  receiver: chan::Receiver<ChannelMessage>,
  buffer: Vec<u8>,
  shared: Arc<Shared>,
}

impl EventStream {
  /// Blocks for the next notice; `None` once the core closes the stream.
  pub fn recv(&mut self) -> Result<Option<Notice>, Error> {
    loop {
      if let Some(notice) = self.buffered() {
        return notice.map(Some);
      }
      let message = self.receiver.recv();
      if let Some(end) = self.fed(message) {
        return end;
      }
    }
  }

  /// `recv`, awaited: the task sleeps without holding a thread.
  pub async fn recv_async(&mut self) -> Result<Option<Notice>, Error> {
    loop {
      if let Some(notice) = self.buffered() {
        return notice.map(Some);
      }
      let message = self.receiver.recv_async().await;
      if let Some(end) = self.fed(message) {
        return end;
      }
    }
  }

  /// The next whole line already received, as a notice.
  fn buffered(&mut self) -> Option<Result<Notice, Error>> {
    let end = self.buffer.iter().position(|&byte| byte == b'\n')?;
    let line: Vec<u8> = self.buffer.drain(..=end).collect();
    Some(parse_notice(&line))
  }

  /// Takes in what arrived; `Some` when the stream ended.
  fn fed(
    &mut self,
    message: Result<ChannelMessage, chan::Closed>,
  ) -> Option<Result<Option<Notice>, Error>> {
    match message {
      Ok(ChannelMessage::Data(bytes)) => {
        give_credit(&self.shared, self.channel, bytes.len());
        self.buffer.extend(bytes);
        None
      }
      Ok(ChannelMessage::Closed) => Some(Ok(None)),
      Err(_) => Some(Err(Error::Disconnected)),
    }
  }
}

fn parse_notice(line: &[u8]) -> Result<Notice, Error> {
  match parse(line)? {
    Incoming::Notification { method, params } => Ok(match method.as_str() {
      "event" => Notice::Event(serde_json::from_value(params)?),
      "heartbeat" => Notice::Heartbeat,
      "missed" => Notice::Missed(params),
      _ => Notice::Other { method, params },
    }),
    other => Err(Error::Protocol(format!(
      "expected a notification, got {other:?}"
    ))),
  }
}
