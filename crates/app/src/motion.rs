//! Values that ease from one number to another over a short time, for layout changes that should
//! move instead of jump.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long layout changes take: quick enough not to wait on, slow enough to follow.
const DURATION: Duration = Duration::from_millis(220);

/// The system asks for less motion: changes happen at once.
static REDUCED: AtomicBool = AtomicBool::new(false);

/// How long a change takes now: none when the system asks for less motion.
pub(crate) fn duration() -> Duration {
  if REDUCED.load(Ordering::Relaxed) {
    Duration::ZERO
  } else {
    DURATION
  }
}

/// Reads the system's reduce motion setting; call it again to notice a change.
pub(crate) fn check_reduce_motion() {
  REDUCED.store(system_reduces_motion(), Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn system_reduces_motion() -> bool {
  let output = std::process::Command::new("defaults")
    .args(["read", "com.apple.universalaccess", "reduceMotion"])
    .output();
  output.is_ok_and(|output| reduce_motion_from(&String::from_utf8_lossy(&output.stdout)))
}

/// Windows' "Animation effects" setting, off when the user wants less motion.
#[cfg(windows)]
fn system_reduces_motion() -> bool {
  #[link(name = "user32")]
  unsafe extern "system" {
    fn SystemParametersInfoW(
      action: u32,
      param: u32,
      value: *mut std::ffi::c_void,
      ini: u32,
    ) -> i32;
  }
  const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
  let mut animates: i32 = 1;
  // SAFETY: the action writes one BOOL through the pointer, which outlives the call.
  let read = unsafe {
    SystemParametersInfoW(
      SPI_GETCLIENTAREAANIMATION,
      0,
      (&mut animates as *mut i32).cast(),
      0,
    )
  };
  read != 0 && animates == 0
}

#[cfg(not(any(target_os = "macos", windows)))]
fn system_reduces_motion() -> bool {
  false
}

/// `defaults read` prints 1 when the setting is on, and nothing when it was never set.
#[cfg(target_os = "macos")]
fn reduce_motion_from(output: &str) -> bool {
  output.trim() == "1"
}

/// A value moving from one number to another over a duration, read at any instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tween {
  from: f32,
  to: f32,
  start: Instant,
  duration: Duration,
}

impl Tween {
  pub(crate) fn new(from: f32, to: f32, start: Instant, duration: Duration) -> Self {
    Self {
      from,
      to,
      start,
      duration,
    }
  }

  pub(crate) fn settled(value: f32) -> Self {
    Self::new(value, value, Instant::now(), Duration::ZERO)
  }

  fn progress(&self, now: Instant) -> f32 {
    if self.duration.is_zero() {
      return 1.0;
    }
    let elapsed = now.saturating_duration_since(self.start);
    (elapsed.as_secs_f32() / self.duration.as_secs_f32()).min(1.0)
  }

  /// Ease-out cubic: moves off quickly and settles gently.
  pub(crate) fn value(&self, now: Instant) -> f32 {
    let eased = 1.0 - (1.0 - self.progress(now)).powi(3);
    self.from + (self.to - self.from) * eased
  }

  pub(crate) fn done(&self, now: Instant) -> bool {
    self.progress(now) >= 1.0
  }

  /// Heads for `to` from wherever it is at `now`.
  pub(crate) fn retarget(&self, to: f32, now: Instant, duration: Duration) -> Self {
    Self::new(self.value(now), to, now, duration)
  }
}
