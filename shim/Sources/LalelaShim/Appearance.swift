import AppKit
import Foundation

// swift-format-ignore: AlwaysUseLowerCamelCase
@c public func lalela_appearance_accent_rgba(
  outRgba: UnsafeMutablePointer<UInt8>?
) -> Int32 {
  guard Thread.isMainThread else { return LalelaStatus.invalidArgument.rawValue }
  guard let outRgba else { return LalelaStatus.invalidArgument.rawValue }
  guard let color = NSColor.controlAccentColor.usingColorSpace(.sRGB) else {
    return LalelaStatus.internal.rawValue
  }
  var red: CGFloat = 0
  var green: CGFloat = 0
  var blue: CGFloat = 0
  var alpha: CGFloat = 0
  color.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
  outRgba[0] = UInt8((red * 255).rounded())
  outRgba[1] = UInt8((green * 255).rounded())
  outRgba[2] = UInt8((blue * 255).rounded())
  outRgba[3] = UInt8((alpha * 255).rounded())
  return LalelaStatus.ok.rawValue
}
