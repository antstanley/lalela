//! `HttpClient`: a blocking `ureq` wrapper with fixed timeouts, a bounded
//! retry loop, a response-size cap, and cancellable uploads.

use std::io::{Cursor, Read};
use std::time::Duration;

use crate::limits::{
    HTTP_CANCEL_POLL_MS, HTTP_RESPONSE_BYTES_MAX, HTTP_RETRY_AFTER_MS_MAX, HTTP_RETRY_ATTEMPTS_MAX,
    HTTP_RETRY_BACKOFF_MS, HTTP_TIMEOUT_CONNECT_MS, HTTP_TIMEOUT_GLOBAL_MS,
    HTTP_TIMEOUT_RECEIVE_MS, HTTP_TIMEOUT_SEND_BODY_MS,
};
use crate::upload::CountingCancelReader;
use crate::{HttpBody, HttpError, HttpRequest};
use ureq::SendBody;

/// A response to an HTTP request.
#[derive(Debug)]
pub struct HttpResponse {
    /// The HTTP status code.
    pub status: u16,
    /// The response body (already capped).
    pub body: Vec<u8>,
}

/// A blocking HTTP client.
#[derive(Clone)]
pub struct HttpClient {
    agent: ureq::Agent,
}

impl HttpClient {
    /// Build a client with the documented timeouts and `http_status_as_error`
    /// disabled so that status codes are reported as data.
    #[must_use]
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_millis(HTTP_TIMEOUT_CONNECT_MS)))
            .timeout_send_body(Some(Duration::from_millis(HTTP_TIMEOUT_SEND_BODY_MS)))
            .timeout_recv_body(Some(Duration::from_millis(HTTP_TIMEOUT_RECEIVE_MS)))
            .timeout_global(Some(Duration::from_millis(HTTP_TIMEOUT_GLOBAL_MS)))
            .http_status_as_error(false)
            .build()
            .new_agent();
        Self { agent }
    }

    /// Send `request`, watching `is_cancelled` during the upload and between
    /// retries, and reporting upload progress through `on_progress`. Returns
    /// the response body, capped at `HTTP_RESPONSE_BYTES_MAX`.
    #[allow(clippy::needless_pass_by_value)]
    pub fn send(
        &self,
        request: &HttpRequest,
        is_cancelled: &(dyn Fn() -> bool + Sync),
        on_progress: &(dyn Fn(u64, u64) + Sync),
    ) -> Result<HttpResponse, HttpError> {
        let mut attempt = 0usize;
        loop {
            match self.send_once(request, is_cancelled, on_progress) {
                Ok(response) => return Ok(response),
                Err(HttpError::Status {
                    status,
                    retry_after_ms,
                    ..
                }) if retryable_status(status) => {
                    if attempt < HTTP_RETRY_ATTEMPTS_MAX {
                        let backoff = retry_after_ms
                            .filter(|ms| *ms <= HTTP_RETRY_AFTER_MS_MAX)
                            .unwrap_or_else(|| backoff_for(attempt));
                        if sleep_pollable(backoff, is_cancelled)? {
                            return Err(HttpError::Cancelled);
                        }
                        attempt += 1;
                        continue;
                    }
                    return Err(HttpError::Status {
                        status,
                        retry_after_ms,
                        excerpt: None,
                    });
                }
                Err(error @ HttpError::Network { .. }) => {
                    if attempt < HTTP_RETRY_ATTEMPTS_MAX {
                        if sleep_pollable(backoff_for(attempt), is_cancelled)? {
                            return Err(HttpError::Cancelled);
                        }
                        attempt += 1;
                        continue;
                    }
                    return Err(error);
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// One attempt. Status codes and bodies are returned as data so the
    /// caller can inspect them (for example `Retry-After`).
    fn send_once(
        &self,
        request: &HttpRequest,
        is_cancelled: &(dyn Fn() -> bool + Sync),
        on_progress: &(dyn Fn(u64, u64) + Sync),
    ) -> Result<HttpResponse, HttpError> {
        let mut builder = self.agent.post(&request.url);
        if let Some(auth) = &request.auth {
            builder = builder.header("Authorization", auth);
        }
        for header in &request.headers {
            let Some((name, value)) = header.split_once(':') else {
                return Err(HttpError::InvalidUrl {
                    detail: format!("malformed request header: {header}"),
                });
            };
            builder = builder.header(name.trim(), value.trim());
        }

        let result = match &request.body {
            HttpBody::Bytes {
                content_type,
                bytes,
            } => {
                builder = builder.header("Content-Type", content_type);
                let mut reader = CountingCancelReader::new(
                    Cursor::new(bytes),
                    bytes.len() as u64,
                    is_cancelled,
                    on_progress,
                );
                builder.send(SendBody::from_reader(&mut reader))
            }
            HttpBody::Multipart { boundary, parts } => {
                let content_type = format!("multipart/form-data; boundary={boundary}");
                builder = builder.header("Content-Type", content_type);
                let mut reader = multipart_reader(boundary, parts, is_cancelled, on_progress)?;
                builder.send(SendBody::from_reader(&mut reader))
            }
        };

        let response = match result {
            Ok(response) => response,
            Err(error) => return Err(map_transport_error(error)),
        };

        let status = response.status().as_u16();
        let retry_after_ms = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|text| text.trim().parse::<u64>().ok())
            .map(|seconds| seconds.saturating_mul(1000));

        let body = match read_capped(response.into_body(), HTTP_RESPONSE_BYTES_MAX) {
            Ok(body) => body,
            Err(HttpError::ResponseTooLarge) if !(200..300).contains(&status) => {
                return Err(HttpError::Status {
                    status,
                    retry_after_ms,
                    excerpt: Some("response body too large".to_owned()),
                });
            }
            Err(error) => return Err(error),
        };

        if (200..300).contains(&status) {
            Ok(HttpResponse { status, body })
        } else {
            let excerpt = String::from_utf8_lossy(&body);
            let excerpt = excerpt.chars().take(512).collect::<String>();
            Err(HttpError::Status {
                status,
                retry_after_ms,
                excerpt: Some(excerpt),
            })
        }
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `status` should be retried (504 from Deepgram is deliberately not
/// retried: the job exceeded the processing window).
#[must_use]
pub fn retryable_status(status: u16) -> bool {
    status == 429 || status == 502 || status == 503
}

/// The backoff for attempt `attempt` (0-based).
#[must_use]
pub fn backoff_for(attempt: usize) -> u64 {
    assert!(attempt < HTTP_RETRY_BACKOFF_MS.len());
    HTTP_RETRY_BACKOFF_MS[attempt]
}

/// Sleep `ms` milliseconds, checking `is_cancelled` every
/// `HTTP_CANCEL_POLL_MS`. Returns `Ok(true)` when cancelled.
fn sleep_pollable(ms: u64, is_cancelled: &(dyn Fn() -> bool + Sync)) -> Result<bool, HttpError> {
    let mut remaining = ms;
    while remaining > 0 {
        if is_cancelled() {
            return Ok(true);
        }
        let step = remaining.min(HTTP_CANCEL_POLL_MS);
        std::thread::sleep(Duration::from_millis(step));
        remaining -= step;
    }
    Ok(false)
}

/// Read a body capped at `cap` bytes.
fn read_capped(body: ureq::Body, cap: u64) -> Result<Vec<u8>, HttpError> {
    let reader = body.into_reader();
    let mut limited = reader.take(cap + 1);
    let mut out = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut total: u64 = 0;
    loop {
        let read = limited
            .read(&mut chunk)
            .map_err(|error| HttpError::Network {
                detail: format!("{error}"),
            })?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > cap {
            return Err(HttpError::ResponseTooLarge);
        }
        out.extend_from_slice(&chunk[..read]);
    }
    Ok(out)
}

/// Build a streaming multipart reader from pre-serialised parts.
fn multipart_reader<'a>(
    boundary: &'a str,
    parts: &'a [crate::MultipartPart],
    is_cancelled: &'a (dyn Fn() -> bool + Sync),
    on_progress: &'a (dyn Fn(u64, u64) + Sync),
) -> Result<Box<dyn Read + 'a>, HttpError> {
    if parts.is_empty() {
        return Err(HttpError::InvalidUrl {
            detail: "multipart body has no parts".to_owned(),
        });
    }
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(format!("Content-Disposition: {}\r\n", part.disposition).as_bytes());
        for header in &part.headers {
            body.extend_from_slice(header.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&part.body);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let total = body.len() as u64;
    Ok(Box::new(CountingCancelReader::new(
        Cursor::new(body),
        total,
        is_cancelled,
        on_progress,
    )))
}

/// Translate a `ureq` transport error into an `HttpError`.
fn map_transport_error(error: ureq::Error) -> HttpError {
    match error {
        ureq::Error::Timeout(_) => HttpError::Timeout,
        ureq::Error::Io(io_error) if io_error.kind() == std::io::ErrorKind::Interrupted => {
            HttpError::Cancelled
        }
        other => HttpError::Network {
            detail: format!("{other}"),
        },
    }
}
