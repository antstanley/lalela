//! Shared progress and cancellation (06-app-shell.md §Worker).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};

/// Progress phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// No job running.
    None,
    /// Preparing the model (Apple asset download).
    PreparingModel,
    /// Uploading.
    Uploading,
    /// Processing.
    Processing,
}

/// A cell of atomics that the worker writes and the UI reads.
#[derive(Debug)]
pub struct ProgressCell {
    phase: AtomicU16,
    numerator: AtomicU64,
    denominator: AtomicU64,
}

impl ProgressCell {
    /// A fresh cell.
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: AtomicU16::new(Phase::None as u16),
            numerator: AtomicU64::new(0),
            denominator: AtomicU64::new(1),
        }
    }

    /// Report a phase and, for determinate progress, a fraction.
    pub fn set(&self, phase: Phase, numerator: u64, denominator: u64) {
        self.phase.store(phase as u16, Ordering::Relaxed);
        if phase == Phase::Uploading && denominator == 0 {
            self.denominator.store(1, Ordering::Relaxed);
        } else {
            self.denominator.store(denominator, Ordering::Relaxed);
        }
        self.numerator.store(numerator, Ordering::Relaxed);
    }

    /// The current phase.
    #[must_use]
    pub fn phase(&self) -> Phase {
        match self.phase.load(Ordering::Relaxed) {
            1 => Phase::PreparingModel,
            2 => Phase::Uploading,
            3 => Phase::Processing,
            _ => Phase::None,
        }
    }

    /// The current fraction (0..=1), when determinate.
    #[must_use]
    pub fn fraction(&self) -> Option<f32> {
        match self.phase() {
            Phase::Uploading | Phase::PreparingModel => {
                let num = self.numerator.load(Ordering::Relaxed);
                let den = self.denominator.load(Ordering::Relaxed);
                if den == 0 {
                    None
                } else {
                    Some(num as f32 / den as f32)
                }
            }
            Phase::Processing => None,
            Phase::None => None,
        }
    }
}

impl Default for ProgressCell {
    fn default() -> Self {
        Self::new()
    }
}

/// A shared cancellation token.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that is not cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
