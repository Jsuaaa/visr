//! A channel one thread can wait on or one task can await, with std only.
//!
//! Terminal output used to wait on a pool thread per pane. A pane that is only awaiting costs no
//! thread, so many panes (every tab keeps its views) cannot starve the executor.

use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Poll, Waker};

/// The sender is gone and everything it sent has been read.
#[derive(Debug, PartialEq, Eq)]
pub struct Closed;

struct State<T> {
  queue: VecDeque<T>,
  /// Whoever is awaiting the next value.
  waker: Option<Waker>,
  closed: bool,
}

struct Shared<T> {
  state: Mutex<State<T>>,
  ready: Condvar,
}

/// The producing end. Dropping it closes the channel once the queue is drained.
pub struct Sender<T> {
  shared: Arc<Shared<T>>,
}

/// One consumer: a second one would take the first's wakeups.
pub struct Receiver<T> {
  shared: Arc<Shared<T>>,
}

/// Creates an unbounded channel whose receiver can be waited on or awaited.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
  let shared = Arc::new(Shared {
    state: Mutex::new(State {
      queue: VecDeque::new(),
      waker: None,
      closed: false,
    }),
    ready: Condvar::new(),
  });
  (
    Sender {
      shared: Arc::clone(&shared),
    },
    Receiver { shared },
  )
}

impl<T> Shared<T> {
  /// Changes the state, then wakes whoever waits, on a thread or in a task.
  fn update(&self, change: impl FnOnce(&mut State<T>)) {
    let waker = {
      let mut state = self.state.lock().unwrap();
      change(&mut state);
      state.waker.take()
    };
    self.ready.notify_all();
    if let Some(waker) = waker {
      waker.wake();
    }
  }
}

impl<T> Sender<T> {
  /// Queues a value and wakes the receiver.
  pub fn send(&self, value: T) {
    self.shared.update(|state| state.queue.push_back(value));
  }
}

impl<T> Drop for Sender<T> {
  fn drop(&mut self) {
    self.shared.update(|state| state.closed = true);
  }
}

impl<T> Receiver<T> {
  /// Waits on this thread for the next value.
  pub fn recv(&self) -> Result<T, Closed> {
    let mut state = self.shared.state.lock().unwrap();
    loop {
      if let Some(value) = state.queue.pop_front() {
        return Ok(value);
      }
      if state.closed {
        return Err(Closed);
      }
      state = self.shared.ready.wait(state).unwrap();
    }
  }

  /// Awaits the next value; the task sleeps without holding a thread.
  pub async fn recv_async(&self) -> Result<T, Closed> {
    poll_fn(|context| {
      let mut state = self.shared.state.lock().unwrap();
      if let Some(value) = state.queue.pop_front() {
        return Poll::Ready(Ok(value));
      }
      if state.closed {
        return Poll::Ready(Err(Closed));
      }
      state.waker = Some(context.waker().clone());
      Poll::Pending
    })
    .await
  }
}
