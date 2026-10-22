//! Shared HTTP behaviour for the remote providers (02-transcription-providers.md
//! §Shared HTTP behaviour).

/// Connect (TCP + TLS) timeout, in milliseconds.
pub const HTTP_TIMEOUT_CONNECT_MS: u64 = 10_000;
/// Upload (send body) timeout, in milliseconds (1 GiB at about 5 Mbit/s).
pub const HTTP_TIMEOUT_SEND_BODY_MS: u64 = 1_800_000;
/// Response-receive timeout after the upload, in milliseconds (above Deepgram's
/// 10-minute processing window).
pub const HTTP_TIMEOUT_RECEIVE_MS: u64 = 900_000;
/// Whole-request timeout, in milliseconds.
pub const HTTP_TIMEOUT_GLOBAL_MS: u64 = 2_820_000;
/// Maximum response body, in bytes.
pub const HTTP_RESPONSE_BYTES_MAX: u64 = 32 * 1024 * 1024;
/// Extra retry attempts after the first try.
pub const HTTP_RETRY_ATTEMPTS_MAX: usize = 2;
/// Backoff before retry `n`, in milliseconds. Length must equal
/// `HTTP_RETRY_ATTEMPTS_MAX`.
pub const HTTP_RETRY_BACKOFF_MS: [u64; HTTP_RETRY_ATTEMPTS_MAX] = [1_000, 4_000];
/// Longest honoured `Retry-After` value, in milliseconds.
pub const HTTP_RETRY_AFTER_MS_MAX: u64 = 30_000;
/// Cancellation check interval while sleeping between retries, in milliseconds.
pub const HTTP_CANCEL_POLL_MS: u64 = 100;

const _: () = assert!(HTTP_RETRY_BACKOFF_MS.len() == HTTP_RETRY_ATTEMPTS_MAX);
const _: () = assert!(HTTP_RETRY_BACKOFF_MS.len() == 2);

/// A request to the provider API.
pub struct HttpRequest {
    /// Absolute URL.
    pub url: String,
    /// The `Authorization` header value, without the scheme word.
    pub auth: Option<String>,
    /// Extra header lines (`name: value`).
    pub headers: Vec<String>,
    /// The body to send.
    pub body: HttpBody,
}

/// A request body.
pub enum HttpBody {
    /// Raw bytes with a content type.
    Bytes {
        content_type: String,
        bytes: Vec<u8>,
    },
    /// A multipart body with a known boundary.
    Multipart {
        boundary: String,
        /// The serialised parts; the provider builds this list.
        parts: Vec<MultipartPart>,
    },
}

/// One multipart part, pre-serialised by the provider's encoder.
pub struct MultipartPart {
    /// The part's `Content-Disposition` data: `name` and optional `filename`.
    pub disposition: String,
    /// Extra headers (for example `Content-Type`).
    pub headers: Vec<String>,
    /// The part body bytes.
    pub body: Vec<u8>,
}

/// Why an HTTP request failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpError {
    /// The request failed to build (bad URL, invalid header).
    InvalidUrl { detail: String },
    /// A transport-level failure after retries.
    Network { detail: String },
    /// The request timed out.
    Timeout,
    /// The provider returned an HTTP status outside 2xx.
    Status {
        status: u16,
        /// A `Retry-After` value, when present on a rate-limited response.
        retry_after_ms: Option<u64>,
        /// A short body excerpt for diagnostics (never headers).
        excerpt: Option<String>,
    },
    /// The response body exceeded the cap.
    ResponseTooLarge,
    /// The operation was cancelled while waiting or uploading.
    Cancelled,
}
