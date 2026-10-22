import AVFoundation
import Foundation

/// A single AVAudioPlayer slot (PLAYERS_OPEN_MAX = 1). All calls are main
/// thread only.
enum PlayerSlot {
  static let lock = NSLock()
  nonisolated(unsafe) static var player: AVAudioPlayer?

  static func replace(_ newPlayer: AVAudioPlayer?) {
    lock.lock()
    defer { lock.unlock() }
    player?.stop()
    player = newPlayer
  }

  static func current() -> AVAudioPlayer? {
    lock.lock()
    defer { lock.unlock() }
    return player
  }
}

// swift-format-ignore: AlwaysUseLowerCamelCase
@c public func lalela_player_open(
  audioPath: UnsafePointer<CChar>?,
  outDurationMs: UnsafeMutablePointer<UInt64>?
) -> Int32 {
  guard Thread.isMainThread else { return LalelaStatus.invalidArgument.rawValue }
  guard let audioPath, let outDurationMs else { return LalelaStatus.invalidArgument.rawValue }
  let path = String(cString: audioPath)
  let player: AVAudioPlayer
  do {
    player = try AVAudioPlayer(contentsOf: URL(fileURLWithPath: path))
  } catch let error as CocoaError where error.code == .fileNoSuchFile {
    return LalelaStatus.fileNotFound.rawValue
  } catch {
    return LalelaStatus.unsupportedAudio.rawValue
  }
  player.prepareToPlay()
  PlayerSlot.replace(player)
  outDurationMs.pointee = UInt64((player.duration * 1000).rounded())
  return LalelaStatus.ok.rawValue
}

// swift-format-ignore: AlwaysUseLowerCamelCase
@c public func lalela_player_play() -> Int32 {
  guard Thread.isMainThread else { return LalelaStatus.invalidArgument.rawValue }
  guard let player = PlayerSlot.current() else { return LalelaStatus.noPlayer.rawValue }
  player.play()
  return LalelaStatus.ok.rawValue
}

// swift-format-ignore: AlwaysUseLowerCamelCase
@c public func lalela_player_pause() -> Int32 {
  guard Thread.isMainThread else { return LalelaStatus.invalidArgument.rawValue }
  guard let player = PlayerSlot.current() else { return LalelaStatus.noPlayer.rawValue }
  player.pause()
  return LalelaStatus.ok.rawValue
}

// swift-format-ignore: AlwaysUseLowerCamelCase
@c public func lalela_player_seek(positionMs: UInt64) -> Int32 {
  guard Thread.isMainThread else { return LalelaStatus.invalidArgument.rawValue }
  guard let player = PlayerSlot.current() else { return LalelaStatus.noPlayer.rawValue }
  let clamped = min(Double(positionMs) / 1000.0, player.duration)
  player.currentTime = clamped
  return LalelaStatus.ok.rawValue
}

// swift-format-ignore: AlwaysUseLowerCamelCase
@c public func lalela_player_state(
  outPositionMs: UnsafeMutablePointer<UInt64>?,
  outIsPlaying: UnsafeMutablePointer<Bool>?
) -> Int32 {
  guard Thread.isMainThread else { return LalelaStatus.invalidArgument.rawValue }
  guard let outPositionMs, let outIsPlaying else { return LalelaStatus.invalidArgument.rawValue }
  guard let player = PlayerSlot.current() else { return LalelaStatus.noPlayer.rawValue }
  outPositionMs.pointee = UInt64((player.currentTime * 1000).rounded())
  outIsPlaying.pointee = player.isPlaying
  return LalelaStatus.ok.rawValue
}

// swift-format-ignore: AlwaysUseLowerCamelCase
@c public func lalela_player_close() {
  guard Thread.isMainThread else { return }
  PlayerSlot.replace(nil)
}
