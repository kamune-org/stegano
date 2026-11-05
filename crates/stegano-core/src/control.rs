//! Cancellation and per-stage progress for long operations.
//!
//! Progress callbacks run on worker threads, so they are `Sync`. A stage
//! fraction is in `0.0..=1.0` and, for one stage, is only reported forwards.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Error, Result};

/// Coarse step of an embed, extract, or resize.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    Analyzing,
    Keying,
    Embedding,
    Encoding,
    Verifying,
    Extracting,
    Decrypting,
    Resizing,
}

/// Optional cancel flag and progress callback.
///
/// [`Control::none`] leaves both unset. `cancel` is polled between stages and
/// inside the cost and STC loops. Argon2 itself cannot be interrupted; the
/// flag is checked immediately before and after it.
pub struct Control<'a> {
    pub cancel: Option<&'a AtomicBool>,
    pub progress: Option<&'a (dyn Fn(Stage, f32) + Sync)>,
}

impl Control<'static> {
    /// No cancellation and no progress reports.
    pub fn none() -> Self {
        Self {
            cancel: None,
            progress: None,
        }
    }
}

impl Default for Control<'static> {
    fn default() -> Self {
        Self::none()
    }
}

impl Control<'_> {
    pub(crate) fn check(&self) -> Result<()> {
        if self.cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(crate) fn report(&self, stage: Stage, fraction: f32) {
        if let Some(callback) = self.progress {
            callback(stage, fraction.clamp(0.0, 1.0));
        }
    }
}
