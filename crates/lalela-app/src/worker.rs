//! The worker thread (06-app-shell.md §Worker).

use std::sync::Arc;

use lalela_core::audio::AudioSource;
use lalela_core::id::TranscriptId;
use lalela_core::ports::{
    Clock, ModelOption, Progress, ProviderId, TranscribeError, TranscriptStore,
    TranscriptionObserver,
};
use lalela_core::save::{SavePlan, SavedPair};
use lalela_core::transcript::Transcript;
use lalela_core::units::{ApiKey, LanguagePreference};

use crate::progress::{CancelToken, Phase, ProgressCell};

/// A job identifier, unique within one app run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JobId(pub u64);

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

/// The owned form of a transcription request, sent across threads.
pub struct OwnedTranscriptionRequest {
    /// The transcript id.
    pub transcript_id: TranscriptId,
    /// The audio.
    pub audio: AudioSource,
    /// The model.
    pub model: &'static ModelOption,
    /// The language preference.
    pub language: LanguagePreference,
    /// The API key, when required.
    pub api_key: Option<ApiKey>,
}

/// A job for the worker.
pub enum Job {
    /// Transcribe.
    Transcribe {
        /// The job id.
        job_id: JobId,
        /// The provider.
        provider: ProviderId,
        /// The request.
        request: OwnedTranscriptionRequest,
        /// Progress.
        progress: Arc<ProgressCell>,
        /// Cancellation.
        cancel: CancelToken,
    },
    /// Save.
    Save {
        /// The job id.
        job_id: JobId,
        /// The plan.
        plan: SavePlan,
        /// The audio.
        audio: AudioSource,
        /// The transcript.
        transcript: Transcript,
        /// Progress.
        progress: Arc<ProgressCell>,
        /// Cancellation.
        cancel: CancelToken,
    },
    /// Stop the worker.
    Shutdown,
}

/// The outcome of a job.
pub enum JobOutcome {
    /// A transcription finished.
    Transcribed {
        /// The job id.
        job_id: JobId,
        /// The result.
        result: Result<Transcript, TranscribeError>,
    },
    /// A save finished.
    Saved {
        /// The job id.
        job_id: JobId,
        /// The result.
        result: Result<SavedPair, lalela_core::ports::StoreError>,
    },
}

/// The observer the adapters see during a job.
struct JobObserver {
    cell: Arc<ProgressCell>,
    cancel: CancelToken,
}

impl TranscriptionObserver for JobObserver {
    fn on_progress(&self, progress: Progress) {
        match progress {
            Progress::PreparingModel { permille } => {
                self.cell
                    .set(Phase::PreparingModel, u64::from(permille), 1000);
            }
            Progress::Uploading { sent, total } => {
                self.cell
                    .set(Phase::Uploading, sent.as_u64(), total.as_u64());
            }
            Progress::Processing { permille } => {
                if let Some(permille) = permille {
                    self.cell.set(Phase::Processing, u64::from(permille), 1000);
                } else {
                    self.cell.set(Phase::Processing, 0, 1);
                }
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

/// Run the worker loop until `Shutdown`, then signal `done`.
pub fn run_worker(
    registry: Arc<lalela_core::registry::ProviderRegistry>,
    store: Arc<dyn TranscriptStore>,
    clock: Arc<dyn Clock>,
    commands: std::sync::mpsc::Receiver<Job>,
    events: std::sync::mpsc::SyncSender<JobOutcome>,
    done: std::sync::mpsc::Sender<()>,
    ctx: egui::Context,
) {
    log::info!("worker thread started");
    while let Ok(job) = commands.recv() {
        match job {
            Job::Shutdown => break,
            Job::Transcribe {
                job_id,
                provider,
                request,
                progress,
                cancel,
            } => {
                let observer = JobObserver {
                    cell: progress,
                    cancel,
                };
                let result = run_transcribe(
                    &registry,
                    store.as_ref(),
                    clock.as_ref(),
                    provider,
                    request,
                    &observer,
                );
                let outcome = JobOutcome::Transcribed { job_id, result };
                let _ = events.send(outcome);
                ctx.request_repaint();
            }
            Job::Save {
                job_id,
                plan,
                audio,
                transcript,
                progress,
                cancel,
            } => {
                let observer = JobObserver {
                    cell: progress,
                    cancel,
                };
                let result = store.execute(&plan, &audio, &transcript, &observer);
                let outcome = JobOutcome::Saved { job_id, result };
                let _ = events.send(outcome);
                ctx.request_repaint();
            }
        }
    }
    log::info!("worker thread stopped");
    let _ = done.send(());
}

fn run_transcribe(
    registry: &lalela_core::registry::ProviderRegistry,
    _store: &dyn TranscriptStore,
    clock: &dyn Clock,
    provider: ProviderId,
    request: OwnedTranscriptionRequest,
    observer: &JobObserver,
) -> Result<Transcript, TranscribeError> {
    let trans = registry
        .get(provider)
        .ok_or(TranscribeError::ProviderUnavailable {
            reason: "provider is not registered".to_owned(),
        })?;
    let transcription_request = lalela_core::ports::TranscriptionRequest {
        transcript_id: &request.transcript_id,
        audio: &request.audio,
        model: request.model,
        language: &request.language,
        api_key: request.api_key.as_ref(),
    };
    let draft = trans.transcribe(&transcription_request, observer)?;
    Transcript::from_draft(
        draft,
        &request.audio,
        provider,
        lalela_core::units::ModelName::try_new(request.model.id.to_owned())
            .unwrap_or_else(|| panic!("model ids are valid model names")),
        request.transcript_id,
        clock.now_unix_ms(),
    )
}
