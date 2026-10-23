//! The session controller (06-app-shell.md §SessionController).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use lalela_core::audio::{AudioFormat, AudioSource};
use lalela_core::limits;
use lalela_core::open::classify_open_path;
use lalela_core::ports::{
    AudioPlayer, Clock, IdSource, KeySource, PlayheadState, ProviderDescriptor, ProviderId,
    SecretStore, Settings, SettingsStore, StoreError, TranscribeError, TranscriptStore,
};
use lalela_core::registry::ProviderRegistry;
use lalela_core::save::{SavePlan, plan_save};
use lalela_core::session::{JobId as CoreJobId, SessionEvent, SessionState};
use lalela_core::units::{ApiKey, LanguagePreference, Milliseconds};

use crate::progress::{CancelToken, ProgressCell};
use crate::ui::messages::message_for_transcribe_error;
use crate::view_model::{
    KeyView, ModelView, PrimaryAction, ProviderView, SettingsView, Status, ViewModel,
};
use crate::worker::{Job, JobId, JobOutcome, OwnedTranscriptionRequest};

/// A user action that needs confirmation.
pub enum PendingAction {
    /// Open a file.
    Open(PathBuf),
    /// Transcribe.
    Transcribe,
    /// Save with a plan.
    Save(SavePlan),
}

/// The controller.
pub struct Controller {
    state: SessionState,
    settings: Settings,
    registry: Arc<ProviderRegistry>,
    store: Arc<dyn TranscriptStore>,
    player: Box<dyn AudioPlayer>,
    secret: Arc<dyn SecretStore>,
    settings_store: Arc<dyn SettingsStore>,
    ids: Box<dyn IdSource>,
    key_present: BTreeMap<ProviderId, bool>,
    next_job: u64,
    commands: mpsc::SyncSender<Job>,
    events: mpsc::Receiver<JobOutcome>,
    done_rx: mpsc::Receiver<()>,
    progress: Arc<ProgressCell>,
    cancel: CancelToken,
    playhead: PlayheadState,
    pending: Option<PendingAction>,
    last_error: Option<String>,
    transcript_font_pt: f32,
    accent: [u8; 4],
    settings_open: bool,
    confirm: Option<(String, String)>,
    quit_pending: bool,
    quit_confirmed: bool,
    replanned_save: bool,
}

impl Controller {
    /// Build the controller and spawn the worker.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<ProviderRegistry>,
        store: Arc<dyn TranscriptStore>,
        player: Box<dyn AudioPlayer>,
        secret: Arc<dyn SecretStore>,
        settings_store: Arc<dyn SettingsStore>,
        clock: Arc<dyn Clock>,
        ids: Box<dyn IdSource>,
        appearance: Arc<dyn lalela_core::ports::Appearance>,
        ctx: egui::Context,
    ) -> Self {
        let settings_result = settings_store.load();
        let settings = match settings_result {
            Ok(settings) => settings,
            Err(error) => {
                log::warn!("settings load failed, using defaults: {error:?}");
                Settings::defaults()
            }
        };
        let (commands_tx, commands_rx) =
            mpsc::sync_channel(lalela_core::limits::WORKER_COMMANDS_QUEUE_MAX);
        let (events_tx, events_rx) =
            mpsc::sync_channel(lalela_core::limits::WORKER_EVENTS_QUEUE_MAX);
        let (done_tx, done_rx) = mpsc::channel();
        {
            let registry = registry.clone();
            let store = store.clone();
            let clock = clock.clone();
            std::thread::Builder::new()
                .name("lalela-worker".to_owned())
                .spawn(move || {
                    crate::worker::run_worker(
                        registry,
                        store,
                        clock,
                        commands_rx,
                        events_tx,
                        done_tx,
                        ctx,
                    )
                })
                .unwrap_or_else(|error| panic!("spawn worker thread failed: {error}"));
        }
        let mut key_present = BTreeMap::new();
        for provider in [
            ProviderId::AppleSpeech,
            ProviderId::MistralVoxtral,
            ProviderId::Deepgram,
        ] {
            let present = matches!(secret.get(provider), Ok(Some(_)));
            key_present.insert(provider, present);
        }
        let accent = appearance.accent_rgba().unwrap_or([0x0a, 0x7a, 0xff, 0xff]);
        Self {
            state: SessionState::Empty,
            settings,
            registry,
            store,
            player,
            secret,
            settings_store,
            ids,
            key_present,
            next_job: 0,
            commands: commands_tx,
            events: events_rx,
            done_rx,
            progress: Arc::new(ProgressCell::new()),
            cancel: CancelToken::new(),
            playhead: PlayheadState {
                position: Milliseconds::try_new(0)
                    .unwrap_or_else(|| panic!("zero is a valid media time")),
                is_playing: false,
            },
            pending: None,
            last_error: None,
            transcript_font_pt: limits::TRANSCRIPT_FONT_PT_DEFAULT,
            accent,
            settings_open: false,
            confirm: None,
            quit_pending: false,
            quit_confirmed: false,
            replanned_save: false,
        }
    }

    /// Refresh the cached key presence.
    fn refresh_key_present(&mut self) {
        for provider in [
            ProviderId::AppleSpeech,
            ProviderId::MistralVoxtral,
            ProviderId::Deepgram,
        ] {
            let present = matches!(self.secret.get(provider), Ok(Some(_)));
            self.key_present.insert(provider, present);
        }
    }

    /// Apply one intent.
    pub fn apply(&mut self, intent: crate::ui::UiIntent) {
        match intent {
            crate::ui::UiIntent::OpenDialog => {
                if self.job_in_flight() {
                    return;
                }
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "Audio",
                        &["mp3", "m4a", "mp4", "aac", "wav", "aiff", "caf", "flac"],
                    )
                    .add_filter("Transcript", &["transcript.json"])
                    .pick_file()
                {
                    self.request_open(path);
                }
            }
            crate::ui::UiIntent::OpenPath(path) => self.request_open(path),
            crate::ui::UiIntent::SelectProvider(provider) => {
                self.settings.provider = provider;
                self.settings_store.save(&self.settings).ok();
                self.refresh_key_present();
            }
            crate::ui::UiIntent::SelectModel(model) => {
                self.settings.set_model(self.settings.provider, model);
                self.settings_store.save(&self.settings).ok();
            }
            crate::ui::UiIntent::SelectLanguage(language) => {
                self.settings.language = language;
                self.settings_store.save(&self.settings).ok();
            }
            crate::ui::UiIntent::Transcribe => self.request_transcribe(),
            crate::ui::UiIntent::Cancel => {
                if let SessionState::Transcribing { .. } = self.state {
                    self.cancel.cancel();
                    self.move_state(SessionEvent::CancelRequested);
                }
            }
            crate::ui::UiIntent::Save => self.request_save(),
            crate::ui::UiIntent::ShowInFinder => {
                if let Some(pair) = self.state.pair() {
                    let _ = std::process::Command::new("open")
                        .arg("-R")
                        .arg(pair.folder.join(&pair.sidecar_file_name))
                        .spawn();
                }
            }
            crate::ui::UiIntent::PlayPause => self.toggle_playback(),
            crate::ui::UiIntent::Seek(position) => {
                let _ = self.player.seek(position);
            }
            crate::ui::UiIntent::SeekBy(delta) => {
                if let Some(duration) = self.state.source().map(|s| s.duration_ms) {
                    let raw = self.playhead.position.as_u64() as i64 + delta;
                    let clamped = raw.clamp(0, duration.as_u64() as i64) as u64;
                    let _ = self.player.seek(
                        Milliseconds::try_new(clamped)
                            .unwrap_or_else(|| panic!("clamped position is valid")),
                    );
                }
            }
            crate::ui::UiIntent::SegmentNext => self.segment_skip(1),
            crate::ui::UiIntent::SegmentPrevious => self.segment_skip(-1),
            crate::ui::UiIntent::SegmentActivate(index) => {
                if let Some(segments) = self.state.transcript().map(|t| &t.segments)
                    && let Some(segment) = segments.get(index as usize)
                {
                    let _ = self.player.seek(segment.start_ms);
                }
            }
            crate::ui::UiIntent::TranscriptTextLarger => {
                self.transcript_font_pt = (self.transcript_font_pt
                    + limits::TRANSCRIPT_FONT_PT_STEP)
                    .min(limits::TRANSCRIPT_FONT_PT_MAX);
            }
            crate::ui::UiIntent::TranscriptTextSmaller => {
                self.transcript_font_pt = (self.transcript_font_pt
                    - limits::TRANSCRIPT_FONT_PT_STEP)
                    .max(limits::TRANSCRIPT_FONT_PT_MIN);
            }
            crate::ui::UiIntent::TranscriptTextReset => {
                self.transcript_font_pt = limits::TRANSCRIPT_FONT_PT_DEFAULT;
            }
            crate::ui::UiIntent::OpenSettings => {
                self.settings_open = true;
            }
            crate::ui::UiIntent::CloseSettings => {
                self.settings_open = false;
            }
            crate::ui::UiIntent::ChooseOutputFolder => {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.settings.output_folder = Some(folder);
                    self.settings_store.save(&self.settings).ok();
                }
            }
            crate::ui::UiIntent::SetApiKey(provider, key) => {
                if let Some(key) = ApiKey::try_new(key) {
                    if self.secret.set(provider, &key).is_ok() {
                        self.refresh_key_present();
                    } else {
                        self.last_error = Some("The Keychain refused to save the key.".to_owned());
                    }
                } else {
                    self.last_error = Some("That key is not a valid API key.".to_owned());
                }
            }
            crate::ui::UiIntent::RemoveApiKey(provider) => {
                let _ = self.secret.delete(provider);
                self.refresh_key_present();
            }
            crate::ui::UiIntent::Confirm(choice) => {
                let was_quit = self.quit_pending;
                let pending = self.pending.take();
                self.confirm = None;
                self.quit_pending = false;
                if choice == crate::ui::ConfirmChoice::Proceed {
                    if was_quit {
                        // The user confirmed quitting; the app layer observes
                        // this and issues the close command.
                        self.quit_confirmed = true;
                    } else if let Some(action) = pending {
                        match action {
                            PendingAction::Open(path) => self.finish_open(path),
                            PendingAction::Transcribe => self.finish_transcribe(),
                            PendingAction::Save(plan) => self.finish_save(plan),
                        }
                    }
                }
            }
        }
    }

    /// Poll the worker outcomes and the playhead.
    pub fn poll(&mut self) {
        self.poll_playhead();
        while let Ok(outcome) = self.events.try_recv() {
            self.handle_outcome(outcome);
        }
    }

    fn poll_playhead(&mut self) {
        if self.state.source().is_some()
            && let Ok(state) = self.player.state()
        {
            self.playhead = state;
        }
    }

    fn job_in_flight(&self) -> bool {
        self.state.job_in_flight()
    }

    fn move_state(&mut self, event: SessionEvent) {
        let next = std::mem::replace(&mut self.state, SessionState::Empty).apply(event);
        match next {
            Ok(state) => self.state = state,
            Err(error) => panic!("illegal session transition: {error}"),
        }
    }

    /// Request an open; asks for confirmation when an unsaved transcript exists.
    fn request_open(&mut self, path: PathBuf) {
        if self.job_in_flight() {
            return;
        }
        if let SessionState::Unsaved { .. } = self.state {
            self.pending = Some(PendingAction::Open(path));
            self.confirm = Some((
                "Discard the unsaved transcript and open another file?".to_owned(),
                "Discard transcript".to_owned(),
            ));
            return;
        }
        self.finish_open(path);
    }

    fn finish_open(&mut self, path: PathBuf) {
        let sibling_exists = path.with_extension("transcript.json").exists();
        let target = match classify_open_path(&path, sibling_exists) {
            Ok(target) => target,
            Err(_) => {
                self.last_error = Some("Lalela can't open this file type.".to_owned());
                return;
            }
        };
        match target {
            lalela_core::open::OpenTarget::Sidecar(sidecar) => {
                let result = self.store.read_sidecar(&sidecar);
                let (transcript, metadata) = match result {
                    Ok(pair) => pair,
                    Err(error) => {
                        self.last_error = Some(store_error_message(&error));
                        return;
                    }
                };
                let audio_path = sidecar
                    .parent()
                    .map(|folder| folder.join(&metadata.file_name));
                let Some(audio_path) = audio_path else {
                    self.last_error = Some("The sidecar is not inside a folder.".to_owned());
                    return;
                };
                if !audio_path.exists() {
                    self.last_error =
                        Some("The audio file for this saved transcript is missing.".to_owned());
                    return;
                }
                self.load_saved_pair(audio_path, transcript, metadata);
            }
            lalela_core::open::OpenTarget::AudioWithSidecar { audio, sidecar } => {
                let audio_source = match self.load_audio_only(&audio) {
                    Ok(source) => source,
                    Err(message) => {
                        self.last_error = Some(message);
                        return;
                    }
                };
                match self.store.read_sidecar(&sidecar) {
                    Ok((transcript, metadata)) => {
                        if metadata.size_bytes != audio_source.size_bytes
                            || metadata
                                .duration_ms
                                .as_u64()
                                .abs_diff(audio_source.duration_ms.as_u64())
                                > limits::SEGMENT_END_TOLERANCE_MS
                        {
                            self.last_error =
                                Some("Saved transcript does not match this audio.".to_owned());
                            self.move_state(SessionEvent::Opened(audio_source));
                            return;
                        }
                        let folder = audio.parent().map(PathBuf::from).unwrap_or_default();
                        let stem = audio
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("transcript");
                        let pair = lalela_core::save::SavedPair {
                            folder,
                            stem: lalela_core::save::FileStem::try_new(stem.to_owned())
                                .unwrap_or_else(|| {
                                    lalela_core::save::FileStem::try_new("transcript".to_owned())
                                        .unwrap_or_else(|| panic!("fallback stem is valid"))
                                }),
                            audio_file_name: audio_source.file_name.clone(),
                            text_file_name: format!("{stem}.txt"),
                            sidecar_file_name: format!("{stem}.transcript.json"),
                        };
                        self.move_state(SessionEvent::OpenedSaved(audio_source, transcript, pair));
                    }
                    Err(error) => {
                        self.move_state(SessionEvent::Opened(audio_source));
                        self.last_error = Some(store_error_message(&error));
                    }
                }
            }
            lalela_core::open::OpenTarget::AudioOnly(audio) => match self.load_audio_only(&audio) {
                Ok(source) => self.move_state(SessionEvent::Opened(source)),
                Err(message) => self.last_error = Some(message),
            },
        }
    }

    /// Load audio through the player and build an `AudioSource`.
    fn load_audio_only(&mut self, path: &std::path::Path) -> Result<AudioSource, String> {
        let Some(format) = AudioFormat::from_path(path) else {
            return Err("Lalela can't open this file type.".to_owned());
        };
        let metadata =
            std::fs::metadata(path).map_err(|_| "The audio file could not be read.".to_owned())?;
        let size = lalela_core::units::Bytes::new(metadata.len());
        if size.as_u64() > limits::AUDIO_BYTES_MAX {
            return Err("The audio file is too large.".to_owned());
        }
        let duration = self
            .player
            .load(path)
            .map_err(|_| "The audio file could not be decoded.".to_owned())?;
        AudioSource::try_new(path.to_path_buf(), format, size, duration)
            .map_err(|_| "The audio file could not be opened.".to_owned())
    }

    fn load_saved_pair(
        &mut self,
        audio_path: PathBuf,
        transcript: lalela_core::transcript::Transcript,
        metadata: lalela_core::ports::AudioMetadata,
    ) {
        let source = match self.load_audio_only(&audio_path) {
            Ok(source) => source,
            Err(message) => {
                self.last_error = Some(message);
                return;
            }
        };
        if metadata.size_bytes != source.size_bytes
            || metadata
                .duration_ms
                .as_u64()
                .abs_diff(source.duration_ms.as_u64())
                > limits::SEGMENT_END_TOLERANCE_MS
        {
            self.last_error = Some("Saved transcript does not match this audio.".to_owned());
            self.move_state(SessionEvent::Opened(source));
            return;
        }
        let folder = audio_path.parent().map(PathBuf::from).unwrap_or_default();
        let stem = audio_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("transcript");
        let pair = lalela_core::save::SavedPair {
            folder,
            stem: lalela_core::save::FileStem::try_new(stem.to_owned()).unwrap_or_else(|| {
                lalela_core::save::FileStem::try_new("transcript".to_owned())
                    .unwrap_or_else(|| panic!("fallback stem is valid"))
            }),
            audio_file_name: source.file_name.clone(),
            text_file_name: format!("{stem}.txt"),
            sidecar_file_name: format!("{stem}.transcript.json"),
        };
        self.move_state(SessionEvent::OpenedSaved(source, transcript, pair));
    }

    fn request_transcribe(&mut self) {
        if self.job_in_flight() {
            return;
        }
        if let SessionState::Unsaved { .. } = self.state {
            self.pending = Some(PendingAction::Transcribe);
            self.confirm = Some((
                "Transcribing again will replace the unsaved transcript.".to_owned(),
                "Transcribe".to_owned(),
            ));
            return;
        }
        self.finish_transcribe();
    }

    fn finish_transcribe(&mut self) {
        let Some(source) = self.state.source() else {
            self.last_error = Some("Open an audio file first.".to_owned());
            return;
        };
        let provider = self.settings.provider;
        let Some(descriptor) = self.registry.descriptor(provider) else {
            self.last_error = Some("The selected provider is not registered.".to_owned());
            return;
        };
        let key_present = self.key_present.get(&provider).copied().unwrap_or(false);
        if let Some(transcriber) = self.registry.get(provider) {
            match transcriber.readiness(key_present) {
                lalela_core::ports::Readiness::NeedsApiKey => {
                    self.last_error = Some(format!(
                        "{} needs an API key. Add one in Settings.",
                        descriptor.display_name
                    ));
                    return;
                }
                lalela_core::ports::Readiness::Unavailable { reason } => {
                    self.last_error = Some(format!(
                        "{} is not available: {reason}.",
                        descriptor.display_name
                    ));
                    return;
                }
                lalela_core::ports::Readiness::Ready => {}
            }
        }
        if source.size_bytes.as_u64() > descriptor.audio_bytes_max {
            self.last_error = Some(format!(
                "File is too large for {}.",
                descriptor.display_name
            ));
            return;
        }
        if source.duration_ms.as_u64() > descriptor.audio_duration_ms_max {
            self.last_error = Some(format!(
                "Audio is too long for {}.",
                descriptor.display_name
            ));
            return;
        }
        let Some(model) = self.resolved_model(provider, descriptor) else {
            self.last_error = Some("The selected model is not available.".to_owned());
            return;
        };
        let api_key = match self.secret.get(provider) {
            Ok(Some(stored)) => Some(stored.key),
            Ok(None) => None,
            Err(_) => {
                self.last_error = Some("The Keychain could not be read.".to_owned());
                return;
            }
        };
        let request = OwnedTranscriptionRequest {
            transcript_id: self.ids.next_transcript_id(),
            audio: source.clone(),
            model,
            language: effective_language(model, &self.settings.language),
            api_key,
        };
        let job_id = self.next_job_id();
        self.cancel = CancelToken::new();
        self.progress = Arc::new(ProgressCell::new());
        self.replanned_save = false;
        let job = Job::Transcribe {
            job_id,
            provider,
            request,
            progress: self.progress.clone(),
            cancel: self.cancel.clone(),
        };
        if self.commands.try_send(job).is_err() {
            self.last_error = Some("The worker is busy.".to_owned());
            return;
        }
        self.move_state(SessionEvent::TranscribeStarted(
            CoreJobId::new(job_id.as_u64()),
            provider,
        ));
    }

    fn request_save(&mut self) {
        if self.job_in_flight() {
            return;
        }
        let Some(source) = self.state.source() else {
            return;
        };
        let Some(_) = self.state.transcript() else {
            return;
        };
        let output_folder = match &self.settings.output_folder {
            Some(folder) => folder.clone(),
            None => match rfd::FileDialog::new().pick_folder() {
                Some(folder) => {
                    self.settings.output_folder = Some(folder.clone());
                    self.settings_store.save(&self.settings).ok();
                    folder
                }
                None => return,
            },
        };
        let source_folder = source.path.parent().map(PathBuf::from).unwrap_or_default();
        let names = match self.store.list_names(&output_folder) {
            Ok(names) => names,
            Err(error) => {
                self.last_error = Some(store_error_message(&error));
                return;
            }
        };
        let plan = match plan_save(source, &source_folder, &output_folder, &names) {
            Ok(plan) => plan,
            Err(lalela_core::save::SaveError::NoFreeName) => {
                self.last_error = Some("No free file name remains for this transcript.".to_owned());
                return;
            }
            Err(lalela_core::save::SaveError::OutputFolderNotAbsolute) => {
                self.last_error = Some("The output folder is not absolute.".to_owned());
                return;
            }
        };
        if plan.replaces_existing {
            self.pending = Some(PendingAction::Save(plan));
            self.confirm = Some((
                "Replace the existing transcript files in this folder?".to_owned(),
                "Replace files".to_owned(),
            ));
            return;
        }
        self.finish_save(plan);
    }

    fn finish_save(&mut self, plan: SavePlan) {
        let Some(source) = self.state.source().cloned() else {
            return;
        };
        let Some(transcript) = self.state.transcript().cloned() else {
            return;
        };
        let job_id = self.next_job_id();
        self.cancel = CancelToken::new();
        self.progress = Arc::new(ProgressCell::new());
        let job = Job::Save {
            job_id,
            plan,
            audio: source.clone(),
            transcript: transcript.clone(),
            progress: self.progress.clone(),
            cancel: self.cancel.clone(),
        };
        if self.commands.try_send(job).is_err() {
            self.last_error = Some("The worker is busy.".to_owned());
            return;
        }
        self.move_state(SessionEvent::SaveStarted(CoreJobId::new(job_id.as_u64())));
    }

    fn next_job_id(&mut self) -> JobId {
        self.next_job += 1;
        JobId::new(self.next_job)
    }

    fn handle_outcome(&mut self, outcome: JobOutcome) {
        match outcome {
            JobOutcome::Transcribed { job_id, result } => {
                let job_id = CoreJobId::new(job_id.as_u64());
                let event = transcribe_outcome_event(&self.state, job_id, result);
                match &event {
                    SessionEvent::JobEnded(_) => self.last_error = None,
                    SessionEvent::Transcribed(_, _) => self.last_error = None,
                    SessionEvent::TranscribeEnded(_, error) => {
                        self.last_error = Some(message_for_transcribe_error(error));
                    }
                    _ => {}
                }
                self.move_state(event);
            }
            JobOutcome::Saved { job_id, result } => {
                let job_id = CoreJobId::new(job_id.as_u64());
                match result {
                    Ok(pair) => {
                        self.last_error = None;
                        self.replanned_save = false;
                        self.move_state(SessionEvent::Saved(job_id, pair));
                    }
                    Err(StoreError::Conflict) if !self.replanned_save => {
                        self.replanned_save = true;
                        // Re-plan once and retry.
                        let current = self.state.clone();
                        if let SessionState::Saving {
                            source, transcript, ..
                        } = current
                        {
                            let names = self.store.list_names(
                                &self.settings.output_folder.clone().unwrap_or_default(),
                            );
                            if let Ok(names) = names {
                                let source_folder =
                                    source.path.parent().map(PathBuf::from).unwrap_or_default();
                                let output_folder =
                                    self.settings.output_folder.clone().unwrap_or_default();
                                if let Ok(plan) =
                                    plan_save(&source, &source_folder, &output_folder, &names)
                                    && !plan.replaces_existing
                                {
                                    let new_job = Job::Save {
                                        job_id: JobId::new(job_id.as_u64()),
                                        plan,
                                        audio: source,
                                        transcript,
                                        progress: self.progress.clone(),
                                        cancel: self.cancel.clone(),
                                    };
                                    if self.commands.try_send(new_job).is_ok() {
                                        return;
                                    }
                                }
                            }
                        }
                        self.move_state(SessionEvent::SaveFailed(job_id, StoreError::Conflict));
                        self.last_error =
                            Some("The output folder changed while saving.".to_owned());
                    }
                    Err(error) => {
                        self.move_state(SessionEvent::SaveFailed(job_id, error.clone()));
                        self.last_error = Some(store_error_message(&error));
                    }
                }
            }
        }
    }

    /// The model option for the current provider.
    fn resolved_model(
        &self,
        provider: ProviderId,
        descriptor: &ProviderDescriptor,
    ) -> Option<&'static lalela_core::ports::ModelOption> {
        let selected = self.settings.model(provider);
        descriptor
            .models
            .iter()
            .find(|model| Some(model.id) == selected || model.id == descriptor.model_default)
    }

    fn toggle_playback(&mut self) {
        if self.state.source().is_none() {
            return;
        }
        if self.playhead.is_playing {
            let _ = self.player.pause();
        } else {
            let _ = self.player.play();
        }
    }

    fn segment_skip(&mut self, direction: i32) {
        let Some(segments) = self.state.transcript().map(|t| &t.segments) else {
            return;
        };
        if segments.is_empty() {
            return;
        }
        let current = lalela_core::playback::segment_at_position(segments, self.playhead.position);
        let target = match (direction, current) {
            (1, Some(index)) => lalela_core::playback::segment_next_index(index, segments.len()),
            (1, None) => Some(0),
            (-1, Some(index)) => lalela_core::playback::segment_previous_index(
                index,
                self.playhead.position,
                segments,
            ),
            (-1, None) => Some(0),
            _ => None,
        };
        if let Some(index) = target
            && let Some(segment) = segments.get(index as usize)
        {
            let _ = self.player.seek(segment.start_ms);
        }
    }

    /// Build the view model.
    pub fn view_model(&self) -> ViewModel {
        let source = self.state.source();
        let transcript = self.state.transcript();
        let current_segment = transcript.and_then(|t| {
            lalela_core::playback::segment_at_position(&t.segments, self.playhead.position)
        });

        let providers = [
            ProviderId::AppleSpeech,
            ProviderId::MistralVoxtral,
            ProviderId::Deepgram,
        ]
        .into_iter()
        .filter_map(|id| {
            let descriptor = self.registry.descriptor(id)?;
            let readiness = self
                .registry
                .get(id)?
                .readiness(self.key_present.get(&id).copied().unwrap_or(false));
            let selected_model = self
                .settings
                .model(id)
                .unwrap_or(descriptor.model_default)
                .to_owned();
            Some(ProviderView {
                id,
                display_name: descriptor.display_name,
                readiness,
                models: descriptor
                    .models
                    .iter()
                    .map(|model| ModelView {
                        id: model.id,
                        label: model.label,
                        supports_language_hint: model.supports_language_hint,
                    })
                    .collect(),
                selected_model,
            })
        })
        .collect();

        let selected_model_supports_language = self
            .registry
            .descriptor(self.settings.provider)
            .and_then(|descriptor| self.resolved_model(self.settings.provider, descriptor))
            .map(|model| model.supports_language_hint)
            .unwrap_or(false);

        let language = if selected_model_supports_language {
            match &self.settings.language {
                LanguagePreference::Auto => "Auto".to_owned(),
                LanguagePreference::Tag(tag) => tag.as_str().to_owned(),
            }
        } else {
            // The model cannot take a language hint; show Auto (the request
            // always uses Auto for such models).
            "Auto".to_owned()
        };

        let primary_action = match &self.state {
            SessionState::Empty => PrimaryAction::Open,
            SessionState::Loaded { .. } => PrimaryAction::Transcribe,
            SessionState::Transcribing { .. } => PrimaryAction::Cancel,
            SessionState::Cancelling { .. } => PrimaryAction::None,
            SessionState::Unsaved { .. } => PrimaryAction::Save,
            SessionState::Saving { .. } => PrimaryAction::None,
            SessionState::Saved { .. } => PrimaryAction::Transcribe,
        };

        let status = if let Some(error) = &self.last_error {
            Status::Error {
                message: error.clone(),
            }
        } else {
            match &self.state {
                SessionState::Transcribing { provider, .. } => Status::Progress {
                    phase: self.progress.phase(),
                    fraction: self.progress.fraction(),
                    provider: provider.display_name(),
                },
                SessionState::Cancelling { .. } => Status::Cancelling,
                SessionState::Unsaved { .. } => Status::Unsaved,
                SessionState::Saving { .. } => Status::Progress {
                    phase: self.progress.phase(),
                    fraction: self.progress.fraction(),
                    provider: "Save",
                },
                SessionState::Saved { pair, .. } => Status::Saved {
                    folder: pair.folder.clone(),
                },
                _ => Status::Idle,
            }
        };

        let keys = [ProviderId::MistralVoxtral, ProviderId::Deepgram]
            .into_iter()
            .map(|provider| {
                let source = match self.secret.get(provider) {
                    Ok(Some(stored)) => match stored.source {
                        KeySource::Environment => "Using environment key".to_owned(),
                        KeySource::Keychain => "Key saved".to_owned(),
                    },
                    Ok(None) => "No key".to_owned(),
                    Err(_) => "Keychain unavailable".to_owned(),
                };
                KeyView { provider, source }
            })
            .collect();

        ViewModel {
            file_name: source.map(|s| s.file_name.clone()),
            duration: source.map(|s| s.duration_ms),
            providers,
            provider: self.settings.provider,
            language,
            language_enabled: selected_model_supports_language,
            primary_action,
            segments: transcript.map(|t| t.segments.clone()),
            current_segment,
            playhead: self.playhead.position,
            is_playing: self.playhead.is_playing,
            status,
            accent: self.accent,
            transcript_font_pt: self.transcript_font_pt,
            settings_open: self.settings_open,
            confirm_open: self.confirm.is_some() || self.quit_pending,
            confirm_prompt: self
                .confirm
                .as_ref()
                .map(|(p, _)| p.clone())
                .unwrap_or_else(|| {
                    if self.quit_pending {
                        "Quit and discard the unsaved transcript?".to_owned()
                    } else {
                        String::new()
                    }
                }),
            confirm_accept: self
                .confirm
                .as_ref()
                .map(|(_, a)| a.clone())
                .unwrap_or_else(|| {
                    if self.quit_pending {
                        "Quit".to_owned()
                    } else {
                        String::new()
                    }
                }),
            settings: SettingsView {
                output_folder: self.settings.output_folder.clone(),
                keys,
            },
        }
    }

    /// Whether a quit should be requested (after confirmation).
    pub fn quit_pending(&self) -> bool {
        self.quit_pending
    }

    /// Whether the user confirmed quitting through the confirmation sheet.
    /// Consumed by the app layer, which then issues the close command.
    pub fn consume_quit_confirmed(&mut self) -> bool {
        let confirmed = self.quit_confirmed;
        self.quit_confirmed = false;
        confirmed
    }

    /// Request quit; confirms when an unsaved transcript would be lost.
    pub fn request_quit(&mut self) {
        let unsaved = matches!(
            self.state,
            SessionState::Unsaved { .. }
                | SessionState::Saving { .. }
                | SessionState::Transcribing { .. }
                | SessionState::Cancelling { .. }
        );
        self.quit_pending = unsaved;
    }

    /// Shut the worker down, waiting at most
    /// `WORKER_SHUTDOWN_WAIT_MS` for it to stop.
    pub fn shutdown(&mut self) {
        let _ = self.commands.send(Job::Shutdown);
        let wait = std::time::Duration::from_millis(lalela_core::limits::WORKER_SHUTDOWN_WAIT_MS);
        let _ = self.done_rx.recv_timeout(wait);
    }
}

/// The effective language for a transcription: when the selected model does
/// not support a language hint, the request must use `Auto` (the adapter
/// asserts this; see 02-transcription-providers.md).
#[must_use]
fn effective_language(
    model: &lalela_core::ports::ModelOption,
    settings: &LanguagePreference,
) -> LanguagePreference {
    if model.supports_language_hint {
        settings.clone()
    } else {
        LanguagePreference::Auto
    }
}

/// The session event for a finished transcription job. While the session is
/// `Cancelling`, the outcome is discarded and `JobEnded` is raised (the state
/// machine accepts only `JobEnded` from `Cancelling`); otherwise the result is
/// carried as `Transcribed` or `TranscribeEnded`.
#[must_use]
fn transcribe_outcome_event(
    state: &SessionState,
    job_id: CoreJobId,
    result: Result<lalela_core::transcript::Transcript, TranscribeError>,
) -> SessionEvent {
    if matches!(state, SessionState::Cancelling { .. }) {
        SessionEvent::JobEnded(job_id)
    } else {
        match result {
            Ok(transcript) => SessionEvent::Transcribed(job_id, transcript),
            Err(error) => SessionEvent::TranscribeEnded(job_id, error),
        }
    }
}
#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::items_after_test_module
    )]
    use super::*;
    use lalela_core::audio::{AudioFormat, AudioSource};
    use lalela_core::id::TranscriptId;
    use lalela_core::transcript::Transcript;
    use lalela_core::units::{Bytes, Milliseconds, ModelName};

    fn audio_source() -> AudioSource {
        AudioSource::try_new(
            std::path::PathBuf::from("/tmp/a.m4a"),
            AudioFormat::M4a,
            Bytes::new(100),
            Milliseconds::try_new(60_000).unwrap(),
        )
        .unwrap()
    }

    fn transcript() -> Transcript {
        Transcript::from_parts(
            TranscriptId::try_new("trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f".to_owned()).unwrap(),
            ProviderId::Deepgram,
            ModelName::try_new("nova-3".to_owned()).unwrap(),
            None,
            1,
            Milliseconds::try_new(60_000).unwrap(),
            vec![lalela_core::transcript::Segment {
                start_ms: Milliseconds::try_new(0).unwrap(),
                end_ms: Milliseconds::try_new(1_000).unwrap(),
                text: "hello".to_owned(),
            }],
        )
        .unwrap()
    }

    #[test]
    fn cancelled_job_outcome_maps_to_job_ended() {
        let state = SessionState::Empty
            .apply(SessionEvent::Opened(audio_source()))
            .unwrap()
            .apply(SessionEvent::TranscribeStarted(
                CoreJobId::new(1),
                ProviderId::Deepgram,
            ))
            .unwrap()
            .apply(SessionEvent::CancelRequested)
            .unwrap();
        // Even a *successful* result is discarded while cancelling.
        let event = transcribe_outcome_event(&state, CoreJobId::new(1), Ok(transcript()));
        assert!(matches!(event, SessionEvent::JobEnded(id) if id == CoreJobId::new(1)));
        // And the state machine accepts it, restoring the prior (Loaded) state.
        let next = state.apply(event).unwrap();
        assert_eq!(next.kind(), lalela_core::session::StateKind::Loaded);
    }

    #[test]
    fn transcribing_job_outcome_maps_to_transcribed_or_ended() {
        let state = SessionState::Empty
            .apply(SessionEvent::Opened(audio_source()))
            .unwrap()
            .apply(SessionEvent::TranscribeStarted(
                CoreJobId::new(1),
                ProviderId::Deepgram,
            ))
            .unwrap();
        let event = transcribe_outcome_event(&state, CoreJobId::new(1), Ok(transcript()));
        assert!(matches!(event, SessionEvent::Transcribed(id, _) if id == CoreJobId::new(1)));
        let event =
            transcribe_outcome_event(&state, CoreJobId::new(1), Err(TranscribeError::Timeout));
        assert!(matches!(event, SessionEvent::TranscribeEnded(id, _) if id == CoreJobId::new(1)));
    }

    #[test]
    fn effective_language_coerces_to_auto_without_hint() {
        let hint_model = lalela_core::ports::ModelOption {
            id: "m",
            label: "m",
            supports_language_hint: true,
        };
        let no_hint_model = lalela_core::ports::ModelOption {
            id: "m",
            label: "m",
            supports_language_hint: false,
        };
        let tagged = LanguagePreference::Tag(
            lalela_core::units::LanguageTag::try_new("fr".to_owned()).unwrap(),
        );
        assert_eq!(effective_language(&hint_model, &tagged), tagged);
        assert_eq!(
            effective_language(&no_hint_model, &tagged),
            LanguagePreference::Auto
        );
    }
}

/// A status message for a store error.
pub fn store_error_message(error: &StoreError) -> String {
    match error {
        StoreError::FolderMissing => "The output folder does not exist.".to_owned(),
        StoreError::FolderNotWritable => "The output folder is not writable.".to_owned(),
        StoreError::FolderTooLarge => "The output folder has too many files.".to_owned(),
        StoreError::Conflict => "The output folder changed while saving.".to_owned(),
        StoreError::SidecarTooLarge => "The transcript is too large to save.".to_owned(),
        StoreError::UnsupportedVersion => "This saved transcript uses a newer format.".to_owned(),
        StoreError::Corrupt { detail } => format!("This saved transcript is invalid: {detail}"),
        StoreError::Cancelled => "Save cancelled.".to_owned(),
        StoreError::Io { detail } => format!("Saving failed: {detail}"),
    }
}
