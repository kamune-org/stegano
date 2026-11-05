//! Map core stage fractions onto one overall fraction.
//!
//! Events stay monotonic. They are throttled to about 20 per second, except
//! when the stage name changes. `1.0` is sent only from [`Sink::finish`].

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use stegano_core::Stage;
use tauri::ipc::Channel;

#[derive(Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub fraction: f32,
}

struct Gate {
    last: f32,
    stage: String,
    sent: Instant,
}

pub struct Sink {
    channel: Channel<Progress>,
    gate: Mutex<Gate>,
}

impl Sink {
    pub fn new(channel: Channel<Progress>) -> Self {
        Self {
            channel,
            gate: Mutex::new(Gate {
                last: 0.0,
                stage: String::new(),
                sent: Instant::now() - Duration::from_secs(1),
            }),
        }
    }

    pub fn emit(&self, stage: &str, fraction: f32) {
        self.send(stage, fraction.clamp(0.0, 0.99), false);
    }

    /// Overall `1.0`, after the command has succeeded.
    pub fn finish(&self, stage: &str) {
        self.send(stage, 1.0, true);
    }

    pub fn hide(&self, stage: Stage, fraction: f32) {
        let (name, overall) = match stage {
            Stage::Analyzing => ("analyzing", 0.05 + 0.15 * fraction),
            Stage::Keying => ("keying", 0.20 + 0.10 * fraction),
            Stage::Embedding => ("embedding", 0.30 + 0.50 * fraction),
            Stage::Encoding => ("encoding", 0.80 + 0.10 * fraction),
            Stage::Verifying => ("verifying", 0.90 + 0.09 * fraction),
            Stage::Extracting | Stage::Decrypting | Stage::Resizing => return,
        };
        self.emit(name, overall);
    }

    pub fn reveal(&self, stage: Stage, fraction: f32) {
        let (name, overall) = match stage {
            Stage::Keying => ("keying", 0.40 + 0.10 * fraction),
            Stage::Extracting => ("extracting", 0.50 + 0.40 * fraction),
            Stage::Decrypting => ("decrypting", 0.90 + 0.09 * fraction),
            Stage::Analyzing
            | Stage::Embedding
            | Stage::Encoding
            | Stage::Verifying
            | Stage::Resizing => return,
        };
        self.emit(name, overall);
    }

    pub fn resize(&self, stage: Stage, fraction: f32) {
        if stage == Stage::Resizing {
            self.emit("resizing", 0.05 + 0.75 * fraction);
        }
    }

    fn send(&self, stage: &str, fraction: f32, force: bool) {
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if fraction < gate.last {
            return;
        }
        let changed = gate.stage != stage;
        let due = gate.sent.elapsed() >= Duration::from_millis(50);
        gate.last = fraction;
        if !force && !changed && !due {
            return;
        }
        gate.stage = stage.to_string();
        gate.sent = Instant::now();
        drop(gate);
        let _ = self.channel.send(Progress {
            stage: stage.to_string(),
            fraction,
        });
    }
}
