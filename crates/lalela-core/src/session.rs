//! The session state machine (01-domain-model.md §Session state machine).

use std::fmt;

use crate::audio::AudioSource;
use crate::ports::{ProviderId, StoreError, TranscribeError};
use crate::save::SavedPair;
use crate::transcript::Transcript;

/// A job identifier, unique within one app run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(u64);

impl JobId {
    /// Construct a job ID.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The raw value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// What the session looked like before a job started; restored when the job
/// ends without a successful replacement.
#[derive(Clone, Debug)]
pub enum Prior {
    /// Nothing to restore (the session was `Loaded`).
    None,
    /// An unsaved transcript.
    Unsaved(Transcript),
    /// A saved transcript and its pair.
    Saved(Transcript, SavedPair),
}

/// A coarse state name, for transition-error messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateKind {
    /// `Empty`
    Empty,
    /// `Loaded`
    Loaded,
    /// `Transcribing`
    Transcribing,
    /// `Cancelling`
    Cancelling,
    /// `Unsaved`
    Unsaved,
    /// `Saving`
    Saving,
    /// `Saved`
    Saved,
}

/// An event name, for transition-error messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// `Opened`
    Opened,
    /// `OpenedSaved`
    OpenedSaved,
    /// `TranscribeStarted`
    TranscribeStarted,
    /// `CancelRequested`
    CancelRequested,
    /// `Transcribed`
    Transcribed,
    /// `TranscribeEnded`
    TranscribeEnded,
    /// `JobEnded`
    JobEnded,
    /// `SaveStarted`
    SaveStarted,
    /// `Saved`
    Saved,
    /// `SaveFailed`
    SaveFailed,
}

/// An illegal transition: a programmer error in the controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionError {
    /// The state the transition left.
    pub from: StateKind,
    /// The event that was illegal.
    pub event: EventKind,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "illegal transition from {:?} on {:?}",
            self.from, self.event
        )
    }
}

/// The session state machine. Every transition goes through
/// `SessionState::apply`; there is no other way to mutate a session state.
#[derive(Clone, Debug)]
pub enum SessionState {
    /// No audio open.
    Empty,
    /// Audio open, no transcript.
    Loaded { source: AudioSource },
    /// A transcription job is running.
    Transcribing {
        source: AudioSource,
        job_id: JobId,
        provider: ProviderId,
        prior: Prior,
    },
    /// Cancel requested; waiting for the worker to stop.
    Cancelling {
        source: AudioSource,
        job_id: JobId,
        provider: ProviderId,
        prior: Prior,
    },
    /// Transcript present, not on disk.
    Unsaved {
        source: AudioSource,
        transcript: Transcript,
    },
    /// A save job is running.
    Saving {
        source: AudioSource,
        transcript: Transcript,
        job_id: JobId,
    },
    /// Transcript on disk, matching memory.
    Saved {
        source: AudioSource,
        transcript: Transcript,
        pair: SavedPair,
    },
}

/// A session event.
#[derive(Clone, Debug)]
pub enum SessionEvent {
    /// An audio file opened with no valid sidecar.
    Opened(AudioSource),
    /// A saved pair opened.
    OpenedSaved(AudioSource, Transcript, SavedPair),
    /// A transcription job started.
    TranscribeStarted(JobId, ProviderId),
    /// Cancel was requested for the running transcription.
    CancelRequested,
    /// A transcription produced a transcript.
    Transcribed(JobId, Transcript),
    /// A transcription ended in error.
    TranscribeEnded(JobId, TranscribeError),
    /// A cancelled job ended (outcome discarded).
    JobEnded(JobId),
    /// A save job started.
    SaveStarted(JobId),
    /// A save completed.
    Saved(JobId, SavedPair),
    /// A save failed.
    SaveFailed(JobId, StoreError),
}

impl SessionState {
    /// The state kind.
    #[must_use]
    pub const fn kind(&self) -> StateKind {
        match self {
            Self::Empty => StateKind::Empty,
            Self::Loaded { .. } => StateKind::Loaded,
            Self::Transcribing { .. } => StateKind::Transcribing,
            Self::Cancelling { .. } => StateKind::Cancelling,
            Self::Unsaved { .. } => StateKind::Unsaved,
            Self::Saving { .. } => StateKind::Saving,
            Self::Saved { .. } => StateKind::Saved,
        }
    }

    /// The audio source, when one is open.
    #[must_use]
    pub fn source(&self) -> Option<&AudioSource> {
        match self {
            Self::Empty => None,
            Self::Loaded { source } => Some(source),
            Self::Transcribing { source, .. } => Some(source),
            Self::Cancelling { source, .. } => Some(source),
            Self::Unsaved { source, .. } => Some(source),
            Self::Saving { source, .. } => Some(source),
            Self::Saved { source, .. } => Some(source),
        }
    }

    /// The current transcript, when one exists.
    #[must_use]
    pub fn transcript(&self) -> Option<&Transcript> {
        match self {
            Self::Unsaved { transcript, .. }
            | Self::Saving { transcript, .. }
            | Self::Saved { transcript, .. } => Some(transcript),
            _ => None,
        }
    }

    /// The saved pair, when the session is saved.
    #[must_use]
    pub fn pair(&self) -> Option<&SavedPair> {
        match self {
            Self::Saved { pair, .. } => Some(pair),
            _ => None,
        }
    }

    /// The running job's ID, when a job is in flight.
    #[must_use]
    pub fn job_id(&self) -> Option<JobId> {
        match self {
            Self::Transcribing { job_id, .. }
            | Self::Cancelling { job_id, .. }
            | Self::Saving { job_id, .. } => Some(*job_id),
            _ => None,
        }
    }

    /// The provider of the running transcription, when in transcription.
    #[must_use]
    pub fn transcription_provider(&self) -> Option<ProviderId> {
        match self {
            Self::Transcribing { provider, .. } | Self::Cancelling { provider, .. } => {
                Some(*provider)
            }
            _ => None,
        }
    }

    /// The prior state, when a transcription job is in flight.
    #[must_use]
    pub fn prior(&self) -> Option<&Prior> {
        match self {
            Self::Transcribing { prior, .. } | Self::Cancelling { prior, .. } => Some(prior),
            _ => None,
        }
    }

    /// Whether a job is running or cancelling.
    #[must_use]
    pub fn job_in_flight(&self) -> bool {
        matches!(
            self,
            Self::Transcribing { .. } | Self::Cancelling { .. } | Self::Saving { .. }
        )
    }

    /// Apply an event, producing the next state.
    pub fn apply(self, event: SessionEvent) -> Result<Self, TransitionError> {
        let from = self.kind();
        let error = |event: EventKind| TransitionError { from, event };
        match (self, event) {
            (
                Self::Empty | Self::Loaded { .. } | Self::Unsaved { .. } | Self::Saved { .. },
                SessionEvent::Opened(source),
            ) => Ok(Self::Loaded { source }),
            (
                Self::Empty | Self::Loaded { .. } | Self::Unsaved { .. } | Self::Saved { .. },
                SessionEvent::OpenedSaved(source, transcript, pair),
            ) => Ok(Self::Saved {
                source,
                transcript,
                pair,
            }),
            (Self::Loaded { source }, SessionEvent::TranscribeStarted(job_id, provider)) => {
                Ok(Self::Transcribing {
                    source,
                    job_id,
                    provider,
                    prior: Prior::None,
                })
            }
            (
                Self::Unsaved { source, transcript },
                SessionEvent::TranscribeStarted(job_id, provider),
            ) => Ok(Self::Transcribing {
                source,
                job_id,
                provider,
                prior: Prior::Unsaved(transcript),
            }),
            (
                Self::Saved {
                    source,
                    transcript,
                    pair,
                },
                SessionEvent::TranscribeStarted(job_id, provider),
            ) => Ok(Self::Transcribing {
                source,
                job_id,
                provider,
                prior: Prior::Saved(transcript, pair),
            }),
            (
                Self::Transcribing {
                    source,
                    job_id,
                    provider,
                    prior,
                },
                SessionEvent::CancelRequested,
            ) => Ok(Self::Cancelling {
                source,
                job_id,
                provider,
                prior,
            }),
            (
                Self::Transcribing { source, job_id, .. },
                SessionEvent::Transcribed(event_job_id, transcript),
            ) => {
                if event_job_id != job_id {
                    return Err(error(EventKind::Transcribed));
                }
                Ok(Self::Unsaved { source, transcript })
            }
            (
                Self::Transcribing {
                    source,
                    job_id,
                    prior,
                    ..
                },
                SessionEvent::TranscribeEnded(event_job_id, _),
            ) => {
                if event_job_id != job_id {
                    return Err(error(EventKind::TranscribeEnded));
                }
                Ok(match prior {
                    Prior::None => Self::Loaded { source },
                    Prior::Unsaved(transcript) => Self::Unsaved { source, transcript },
                    Prior::Saved(transcript, pair) => Self::Saved {
                        source,
                        transcript,
                        pair,
                    },
                })
            }
            (
                Self::Cancelling {
                    source,
                    job_id,
                    prior,
                    ..
                },
                SessionEvent::JobEnded(event_job_id),
            ) => {
                if event_job_id != job_id {
                    return Err(error(EventKind::JobEnded));
                }
                Ok(match prior {
                    Prior::None => Self::Loaded { source },
                    Prior::Unsaved(transcript) => Self::Unsaved { source, transcript },
                    Prior::Saved(transcript, pair) => Self::Saved {
                        source,
                        transcript,
                        pair,
                    },
                })
            }
            (Self::Unsaved { source, transcript }, SessionEvent::SaveStarted(job_id)) => {
                Ok(Self::Saving {
                    source,
                    transcript,
                    job_id,
                })
            }
            (
                Self::Saving {
                    source,
                    transcript,
                    job_id,
                },
                SessionEvent::Saved(event_job_id, pair),
            ) => {
                if event_job_id != job_id {
                    return Err(error(EventKind::Saved));
                }
                Ok(Self::Saved {
                    source,
                    transcript,
                    pair,
                })
            }
            (
                Self::Saving {
                    source,
                    transcript,
                    job_id,
                },
                SessionEvent::SaveFailed(event_job_id, _),
            ) => {
                if event_job_id != job_id {
                    return Err(error(EventKind::SaveFailed));
                }
                Ok(Self::Unsaved { source, transcript })
            }
            (state, event) => Err(TransitionError {
                from: state.kind(),
                event: event_kind(&event),
            }),
        }
    }
}

/// The kind of an event, for transition-error messages.
fn event_kind(event: &SessionEvent) -> EventKind {
    match event {
        SessionEvent::Opened(_) => EventKind::Opened,
        SessionEvent::OpenedSaved(_, _, _) => EventKind::OpenedSaved,
        SessionEvent::TranscribeStarted(_, _) => EventKind::TranscribeStarted,
        SessionEvent::CancelRequested => EventKind::CancelRequested,
        SessionEvent::Transcribed(_, _) => EventKind::Transcribed,
        SessionEvent::TranscribeEnded(_, _) => EventKind::TranscribeEnded,
        SessionEvent::JobEnded(_) => EventKind::JobEnded,
        SessionEvent::SaveStarted(_) => EventKind::SaveStarted,
        SessionEvent::Saved(_, _) => EventKind::Saved,
        SessionEvent::SaveFailed(_, _) => EventKind::SaveFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioFormat, AudioSource};
    use crate::units::{Bytes, Milliseconds};

    fn source() -> AudioSource {
        AudioSource::try_new(
            std::path::PathBuf::from("/tmp/a.m4a"),
            AudioFormat::M4a,
            Bytes::new(100),
            Milliseconds::try_new(60_000).unwrap(),
        )
        .unwrap()
    }

    fn transcript() -> Transcript {
        crate::transcript::Transcript::from_parts(
            crate::id::TranscriptId::try_new("trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f".to_owned())
                .unwrap(),
            ProviderId::Deepgram,
            crate::units::ModelName::try_new("nova-3".to_owned()).unwrap(),
            None,
            1_000,
            Milliseconds::try_new(60_000).unwrap(),
            vec![crate::transcript::Segment {
                start_ms: Milliseconds::try_new(0).unwrap(),
                end_ms: Milliseconds::try_new(1_000).unwrap(),
                text: "Hello".to_owned(),
            }],
        )
        .unwrap()
    }

    fn pair() -> SavedPair {
        SavedPair {
            folder: std::path::PathBuf::from("/out"),
            stem: crate::save::FileStem::try_new("a".to_owned()).unwrap(),
            audio_file_name: "a.m4a".to_owned(),
            text_file_name: "a.txt".to_owned(),
            sidecar_file_name: "a.transcript.json".to_owned(),
        }
    }

    #[test]
    fn open_flows() {
        let s = SessionState::Empty
            .apply(SessionEvent::Opened(source()))
            .unwrap();
        assert_eq!(s.kind(), StateKind::Loaded);
        let s = s.apply(SessionEvent::Opened(source())).unwrap();
        assert_eq!(s.kind(), StateKind::Loaded);
        assert!(
            s.apply(SessionEvent::OpenedSaved(source(), transcript(), pair()))
                .unwrap()
                .kind()
                == StateKind::Saved
        );
    }

    #[test]
    fn transcribe_round_trip_from_loaded() {
        let state = SessionState::Empty
            .apply(SessionEvent::Opened(source()))
            .unwrap();
        let job = JobId::new(1);
        let state = state
            .apply(SessionEvent::TranscribeStarted(job, ProviderId::Deepgram))
            .unwrap();
        assert_eq!(state.kind(), StateKind::Transcribing);
        let state = state.apply(SessionEvent::CancelRequested).unwrap();
        assert_eq!(state.kind(), StateKind::Cancelling);
        let state = state.apply(SessionEvent::JobEnded(job)).unwrap();
        assert_eq!(state.kind(), StateKind::Loaded);
    }

    #[test]
    fn transcribe_failure_restores_prior() {
        let state = SessionState::Empty
            .apply(SessionEvent::Opened(source()))
            .unwrap();
        let job = JobId::new(1);
        let state = state
            .apply(SessionEvent::TranscribeStarted(job, ProviderId::Deepgram))
            .unwrap();
        let state = state
            .apply(SessionEvent::TranscribeEnded(job, TranscribeError::Timeout))
            .unwrap();
        assert_eq!(state.kind(), StateKind::Loaded);

        // From Saved, failure restores the saved transcript and pair.
        let state = SessionState::Empty
            .apply(SessionEvent::OpenedSaved(source(), transcript(), pair()))
            .unwrap();
        let job = JobId::new(2);
        let state = state
            .apply(SessionEvent::TranscribeStarted(job, ProviderId::Deepgram))
            .unwrap();
        let state = state
            .apply(SessionEvent::TranscribeEnded(job, TranscribeError::Timeout))
            .unwrap();
        assert_eq!(state.kind(), StateKind::Saved);
        assert!(state.pair().is_some());
        assert!(state.transcript().is_some());
    }

    #[test]
    fn save_flows() {
        let state = SessionState::Empty
            .apply(SessionEvent::OpenedSaved(source(), transcript(), pair()))
            .unwrap();
        let job = JobId::new(1);
        let state = state
            .apply(SessionEvent::TranscribeStarted(job, ProviderId::Deepgram))
            .unwrap();
        let state = state
            .apply(SessionEvent::Transcribed(job, transcript()))
            .unwrap();
        assert_eq!(state.kind(), StateKind::Unsaved);
        let save_job = JobId::new(2);
        let state = state.apply(SessionEvent::SaveStarted(save_job)).unwrap();
        assert_eq!(state.kind(), StateKind::Saving);
        let state = state.apply(SessionEvent::Saved(save_job, pair())).unwrap();
        assert_eq!(state.kind(), StateKind::Saved);
    }

    #[test]
    fn save_failure_returns_to_unsaved() {
        let state = SessionState::Empty
            .apply(SessionEvent::OpenedSaved(source(), transcript(), pair()))
            .unwrap();
        let job = JobId::new(1);
        let state = state
            .apply(SessionEvent::TranscribeStarted(job, ProviderId::Deepgram))
            .unwrap();
        let state = state
            .apply(SessionEvent::Transcribed(job, transcript()))
            .unwrap();
        let save_job = JobId::new(2);
        let state = state.apply(SessionEvent::SaveStarted(save_job)).unwrap();
        let state = state
            .apply(SessionEvent::SaveFailed(
                save_job,
                StoreError::FolderMissing,
            ))
            .unwrap();
        assert_eq!(state.kind(), StateKind::Unsaved);
        assert!(state.transcript().is_some());
    }

    #[test]
    fn mismatch_job_id_is_illegal() {
        let state = SessionState::Empty
            .apply(SessionEvent::Opened(source()))
            .unwrap();
        let state = state
            .apply(SessionEvent::TranscribeStarted(
                JobId::new(1),
                ProviderId::Deepgram,
            ))
            .unwrap();
        let err = state.apply(SessionEvent::Transcribed(JobId::new(99), transcript()));
        assert!(err.is_err());
        let err = err.unwrap_err();
        assert_eq!(err.from, StateKind::Transcribing);
        assert_eq!(err.event, EventKind::Transcribed);
    }

    #[test]
    fn illegal_events_are_rejected() {
        let state = SessionState::Empty;
        assert!(
            state
                .clone()
                .apply(SessionEvent::TranscribeStarted(
                    JobId::new(1),
                    ProviderId::Deepgram
                ))
                .is_err()
        );
        assert!(
            state
                .clone()
                .apply(SessionEvent::SaveStarted(JobId::new(1)))
                .is_err()
        );
        assert!(state.apply(SessionEvent::CancelRequested).is_err());
        let state = SessionState::Empty
            .apply(SessionEvent::Opened(source()))
            .unwrap();
        assert!(
            state
                .clone()
                .apply(SessionEvent::SaveStarted(JobId::new(1)))
                .is_err()
        );
        assert!(
            state
                .apply(SessionEvent::Saved(JobId::new(1), pair()))
                .is_err()
        );
    }
}
