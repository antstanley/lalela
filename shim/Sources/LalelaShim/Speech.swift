import AVFoundation
import Foundation
import Speech
import os

/// Shared state between the export and the Swift tasks that work for it.
/// The `abandoned` flag, guarded by an NSLock, is the hard boundary after
/// which no Rust pointer is touched.
final class CallState: @unchecked Sendable {
  private let lock = NSLock()
  private var abandoned = false
  private var outLength: UInt32 = 0
  private var segments: [(CMTimeRange, String)] = []

  let context: UnsafeMutableRawPointer?
  let onProgress: LalelaProgressFn?
  let isCancelled: LalelaCancelledFn?
  let outJson: UnsafeMutablePointer<UInt8>?
  let outJsonCapacity: UInt32
  var status: LalelaStatus = .ok

  init(
    context: UnsafeMutableRawPointer?,
    onProgress: LalelaProgressFn?,
    isCancelled: LalelaCancelledFn?,
    outJson: UnsafeMutablePointer<UInt8>?,
    outJsonCapacity: UInt32
  ) {
    self.context = context
    self.onProgress = onProgress
    self.isCancelled = isCancelled
    self.outJson = outJson
    self.outJsonCapacity = outJsonCapacity
  }

  func progress(phase: UInt32, permille: UInt32) {
    lock.lock()
    defer { lock.unlock() }
    guard !abandoned else { return }
    onProgress?(context, phase, permille)
  }

  /// True once abandoned; also true when the Rust side says cancelled.
  func cancelled() -> Bool {
    lock.lock()
    defer { lock.unlock() }
    if abandoned { return true }
    return isCancelled?(context) ?? false
  }

  /// Append a final segment for encoding.
  func appendSegment(_ range: CMTimeRange, text: String) {
    lock.lock()
    defer { lock.unlock() }
    guard !abandoned else { return }
    segments.append((range, text))
  }

  /// Snapshot the collected segments.
  func collectedSegments() -> [(CMTimeRange, String)] {
    lock.lock()
    defer { lock.unlock() }
    return segments
  }

  /// Copy JSON into the caller's buffer. Returns false when abandoned or the
  /// buffer cannot hold the data (the caller then reports OutputTooLarge).
  func write(_ json: Data) -> Bool {
    lock.lock()
    defer { lock.unlock() }
    guard !abandoned else { return false }
    guard let outJson, outJsonCapacity > 0 else { return false }
    guard json.count <= Int(outJsonCapacity) else { return false }
    let written = json.withUnsafeBytes { raw -> UInt32 in
      guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return 0 }
      memcpy(outJson, base, raw.count)
      return UInt32(raw.count)
    }
    outLength = written
    return true
  }

  /// The length stored by the last successful `write`.
  func currentLength() -> UInt32 {
    lock.lock()
    defer { lock.unlock() }
    return outLength
  }

  /// Take the lock and mark abandoned, waiting out any in-flight callback.
  func abandon() {
    lock.lock()
    abandoned = true
    lock.unlock()
  }
}

// swift-format-ignore: AlwaysUseLowerCamelCase
@c public func lalela_speech_is_available() -> Bool {
  SpeechTranscriber.isAvailable
}

// swift-format-ignore: AlwaysUseLowerCamelCase
@c public func lalela_speech_transcribe(
  audioPath: UnsafePointer<CChar>?,
  localeIdentifier: UnsafePointer<CChar>?,
  timeoutMs: UInt64,
  context: UnsafeMutableRawPointer?,
  onProgress: LalelaProgressFn?,
  isCancelled: LalelaCancelledFn?,
  outJson: UnsafeMutablePointer<UInt8>?,
  outJsonCapacity: UInt32,
  outJsonLength: UnsafeMutablePointer<UInt32>?
) -> Int32 {
  guard
    let audioPath, let localeIdentifier, let outJson, let outJsonLength,
    outJsonCapacity > 0
  else {
    return LalelaStatus.invalidArgument.rawValue
  }
  guard !Thread.isMainThread else { return LalelaStatus.invalidArgument.rawValue }
  guard SpeechTranscriber.isAvailable else { return LalelaStatus.speechUnavailable.rawValue }

  let audioPathStr = String(cString: audioPath)
  let localeStr = String(cString: localeIdentifier)

  let call = CallState(
    context: context,
    onProgress: onProgress,
    isCancelled: isCancelled,
    outJson: outJson,
    outJsonCapacity: outJsonCapacity
  )
  let semaphore = DispatchSemaphore(value: 0)

  let task = Task {
    do {
      let locale = await SpeechTranscriber.supportedLocale(
        equivalentTo: localeStr.isEmpty ? .current : Locale(identifier: localeStr)
      )
      guard let locale else {
        call.status = LalelaStatus.localeUnsupported
        return
      }

      let transcriber = SpeechTranscriber(
        locale: locale,
        transcriptionOptions: [],
        reportingOptions: [],
        attributeOptions: [.audioTimeRange]
      )

      if let assetRequest = try await AssetInventory.assetInstallationRequest(
        supporting: [transcriber]
      ) {
        let progressTask = Task {
          while !Task.isCancelled {
            let fraction = assetRequest.progress.fractionCompleted
            call.progress(phase: 1, permille: UInt32(fraction * 1000))
            try await Task.sleep(nanoseconds: 200_000_000)
          }
        }
        do {
          try await assetRequest.downloadAndInstall()
        } catch {
          progressTask.cancel()
          call.status = LalelaStatus.assetsUnavailable
          return
        }
        progressTask.cancel()
      }

      let file: AVAudioFile
      do {
        file = try AVAudioFile(forReading: URL(fileURLWithPath: audioPathStr))
      } catch let error as CocoaError where error.code == .fileNoSuchFile {
        call.status = LalelaStatus.fileNotFound
        return
      } catch {
        call.status = LalelaStatus.unsupportedAudio
        return
      }
      let durationSeconds = Double(file.length) / file.fileFormat.sampleRate

      let analyzer = SpeechAnalyzer(modules: [transcriber])

      let collector = Task {
        for try await result in transcriber.results where result.isFinal {
          let range = result.range
          let endSeconds = range.end.seconds
          let permille =
            durationSeconds > 0
            ? UInt32((endSeconds / durationSeconds) * 1000)
            : 0
          call.progress(phase: 2, permille: min(permille, 1000))
          call.appendSegment(range, text: String(result.text.characters))
        }
      }

      let watchdog = Task {
        while !call.cancelled() {
          try await Task.sleep(nanoseconds: 250_000_000)
        }
        await analyzer.cancelAndFinishNow()
      }

      let last: CMTime? = try await analyzer.analyzeSequence(from: file)
      if let last {
        try await analyzer.finalizeAndFinish(through: last)
      } else {
        await analyzer.cancelAndFinishNow()
      }
      try await collector.value
      watchdog.cancel()

      let encoded = speechOutputJSON(locale: locale, segments: call.collectedSegments())
      guard call.write(encoded) else {
        call.status = LalelaStatus.outputTooLarge
        return
      }
      call.status = LalelaStatus.ok
    } catch {
      call.status = LalelaStatus.from(error)
    }
    semaphore.signal()
  }

  let timedOut = semaphore.wait(timeout: .now() + .milliseconds(Int(timeoutMs))) == .timedOut
  call.abandon()
  task.cancel()

  if timedOut {
    return LalelaStatus.timedOut.rawValue
  }
  if call.status == LalelaStatus.ok, call.currentLength() > 0 {
    outJsonLength.pointee = call.currentLength()
  }
  return call.status.rawValue
}

/// Encode the shim output JSON: `{ locale, segments: [{start_ms, end_ms, text}] }`.
private func speechOutputJSON(locale: Locale, segments: [(CMTimeRange, String)]) -> Data {
  let segmentDicts = segments.map { range, text -> [String: Any] in
    [
      "start_ms": max(0, Int64((range.start.seconds * 1000).rounded())),
      "end_ms": max(0, Int64((range.end.seconds * 1000).rounded())),
      "text": text,
    ]
  }
  let root: [String: Any] = [
    "locale": locale.identifier(.bcp47),
    "segments": segmentDicts,
  ]
  // The caller's buffer is large; encoding cannot fail for these types.
  return (try? JSONSerialization.data(withJSONObject: root)) ?? Data("{}".utf8)
}
