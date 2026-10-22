import Foundation
import os

/// Mirrors the `lalela_progress_fn` typedef in lalela_shim.h.
public typealias LalelaProgressFn =
  @convention(c) (UnsafeMutableRawPointer?, UInt32, UInt32) -> Void

/// Mirrors the `lalela_cancelled_fn` typedef in lalela_shim.h.
public typealias LalelaCancelledFn = @convention(c) (UnsafeMutableRawPointer?) -> Bool

/// Status codes crossing the C ABI. The Rust `ShimStatus` mirror must agree
/// (checked by a unit test in lalela-macos).
public enum LalelaStatus: Int32 {
  case ok = 0
  case invalidArgument = 1
  case fileNotFound = 2
  case unsupportedAudio = 3
  case speechUnavailable = 4
  case localeUnsupported = 5
  case assetsUnavailable = 6
  case cancelled = 7
  case timedOut = 8
  case outputTooLarge = 9
  case noPlayer = 10
  case insufficientResources = 11
  case `internal` = 99

  /// Map a thrown error to a status. The shim logs the error before
  /// returning, so the mapping itself carries no text.
  static func from(_ error: Error) -> LalelaStatus {
    switch error {
    case let error as CocoaError where error.code == .fileNoSuchFile:
      return .fileNotFound
    case let error as CocoaError where error.code == .fileReadUnknown:
      return .fileNotFound
    default:
      os_log("LalelaShim: unhandled error: %@", error.localizedDescription)
      return .internal
    }
  }
}
