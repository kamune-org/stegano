//! Cancel flags for in-flight hide, reveal, and prepare jobs.
//!
//! `cancel_job` on an id that has not registered yet leaves a tombstone. The
//! job picks that up if it starts within 60 seconds.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TOMBSTONE_TTL: Duration = Duration::from_secs(60);

enum Slot {
    Live(Arc<AtomicBool>),
    Tombstone(Instant),
}

#[derive(Default)]
pub struct Jobs {
    slots: Mutex<HashMap<String, Slot>>,
}

pub struct Guard {
    jobs: Arc<Jobs>,
    id: String,
    pub flag: Arc<AtomicBool>,
}

impl Jobs {
    pub fn cancel(&self, id: &str) {
        let mut slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        expire(&mut slots);
        match slots.get_mut(id) {
            Some(Slot::Live(flag)) => flag.store(true, Ordering::Relaxed),
            Some(Slot::Tombstone(_)) => {}
            None => {
                slots.insert(id.to_string(), Slot::Tombstone(Instant::now()));
            }
        }
    }

    pub fn begin(self: &Arc<Self>, id: &str) -> Guard {
        let mut slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        expire(&mut slots);
        let flag = Arc::new(AtomicBool::new(false));
        if let Some(Slot::Tombstone(_)) = slots.remove(id) {
            flag.store(true, Ordering::Relaxed);
        }
        slots.insert(id.to_string(), Slot::Live(Arc::clone(&flag)));
        Guard {
            jobs: Arc::clone(self),
            id: id.to_string(),
            flag,
        }
    }

    fn finish(&self, id: &str) {
        let mut slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        if matches!(slots.get(id), Some(Slot::Live(_))) {
            slots.remove(id);
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.jobs.finish(&self.id);
    }
}

fn expire(slots: &mut HashMap<String, Slot>) {
    let now = Instant::now();
    slots.retain(|_, slot| match slot {
        Slot::Live(_) => true,
        Slot::Tombstone(at) => now.saturating_duration_since(*at) < TOMBSTONE_TTL,
    });
}
