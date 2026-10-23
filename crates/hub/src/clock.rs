//! The hub's clock, which tests can set.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The time in Unix milliseconds: the system's, or one a test controls.
#[derive(Debug, Clone, Default)]
pub struct Clock {
    fixed: Option<Arc<AtomicI64>>,
}

impl Clock {
    /// A clock that reads `start` until it is moved with [`Clock::set`].
    #[must_use]
    pub fn fixed(start: i64) -> Self {
        Self {
            fixed: Some(Arc::new(AtomicI64::new(start))),
        }
    }

    /// Moves a fixed clock. A system clock ignores it.
    pub fn set(&self, now: i64) {
        if let Some(fixed) = &self.fixed {
            fixed.store(now, Ordering::SeqCst);
        }
    }

    /// Now, in Unix milliseconds.
    #[must_use]
    pub fn now(&self) -> i64 {
        match &self.fixed {
            Some(fixed) => fixed.load(Ordering::SeqCst),
            None => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|since| i64::try_from(since.as_millis()).ok())
                .unwrap_or_default(),
        }
    }
}
