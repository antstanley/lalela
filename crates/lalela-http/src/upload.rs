//! A counting, cancellable upload body that also reports progress.

use std::io::{self, Read};

/// A reader that wraps `inner`, counts the bytes it delivers, calls
/// `on_progress` as it goes, and aborts with `io::ErrorKind::Interrupted`
/// when `is_cancelled` turns true.
pub struct CountingCancelReader<'a, R> {
    inner: R,
    sent: u64,
    total: u64,
    is_cancelled: &'a (dyn Fn() -> bool + Sync),
    on_progress: &'a (dyn Fn(u64, u64) + Sync),
}

impl<'a, R: Read> CountingCancelReader<'a, R> {
    /// Wrap `inner` with a total byte count.
    #[must_use]
    pub fn new(
        inner: R,
        total: u64,
        is_cancelled: &'a (dyn Fn() -> bool + Sync),
        on_progress: &'a (dyn Fn(u64, u64) + Sync),
    ) -> Self {
        Self {
            inner,
            sent: 0,
            total,
            is_cancelled,
            on_progress,
        }
    }

    /// Bytes delivered so far.
    #[must_use]
    pub const fn sent(&self) -> u64 {
        self.sent
    }
}

impl<R: Read> Read for CountingCancelReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if (self.is_cancelled)() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "upload cancelled",
            ));
        }
        let read = self.inner.read(buf)?;
        if read > 0 {
            self.sent += read as u64;
            (self.on_progress)(self.sent, self.total);
        }
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::io::Cursor;

    #[test]
    fn counts_and_reports() {
        let cancels = std::sync::atomic::AtomicBool::new(false);
        let cancel = || cancels.load(std::sync::atomic::Ordering::Relaxed);
        let reported = std::sync::Mutex::new(Vec::new());
        let report = |sent, total| reported.lock().unwrap().push((sent, total));
        let data = vec![1u8, 2, 3, 4, 5];
        let mut reader = CountingCancelReader::new(Cursor::new(data), 5, &cancel, &report);
        let mut buf = [0u8; 2];
        let mut total = 0usize;
        loop {
            let read = reader.read(&mut buf).unwrap();
            if read == 0 {
                break;
            }
            total += read;
        }
        assert_eq!(total, 5);
        assert_eq!(reader.sent(), 5);
        assert_eq!(*reported.lock().unwrap(), vec![(2, 5), (4, 5), (5, 5)]);
    }

    #[test]
    fn aborts_when_cancelled() {
        let cancels = std::sync::atomic::AtomicBool::new(true);
        let cancel = || cancels.load(std::sync::atomic::Ordering::Relaxed);
        let noop = |_, _| {};
        let mut reader = CountingCancelReader::new(Cursor::new(vec![1u8]), 1, &cancel, &noop);
        let mut buf = [0u8; 4];
        let err = reader.read(&mut buf).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    }
}
