//! A blocking HTTP helper over `ureq` with fixed timeouts, retries, a
//! response-size cap, and cancellable uploads (architecture-principles.md
//! §Crate roles). Shared by the Voxtral and Deepgram adapters.

pub mod client;
pub mod limits;
pub mod upload;

pub use client::{HttpClient, HttpResponse};
pub use limits::*;
pub use upload::CountingCancelReader;
pub use {HttpBody, HttpError, MultipartPart};
