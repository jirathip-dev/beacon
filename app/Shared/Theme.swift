import BeaconKit
import SwiftUI
import UIKit

/// The terminal look on Cooee's palette (q1584, app/design/beacon-app): follows System light/dark.
enum T {
    private static func hex(_ dark: UInt32, _ light: UInt32) -> Color {
        func ui(_ v: UInt32) -> UIColor {
            UIColor(red: CGFloat(v >> 16 & 0xff) / 255, green: CGFloat(v >> 8 & 0xff) / 255,
                    blue: CGFloat(v & 0xff) / 255, alpha: 1)
        }
        return Color(UIColor { $0.userInterfaceStyle == .light ? ui(light) : ui(dark) })
    }

    static let bg = hex(0x0f1a13, 0xf5efda)
    static let card = hex(0x1d1a12, 0xebe2c4)
    static let edge = hex(0x3b3a27, 0xcbbd90)
    static let line = hex(0x26301f, 0xe0d6b4)
    static let fg = hex(0xece4cc, 0x29251a)
    static let mute = hex(0xa3a68a, 0x5c573f)
    static let accent = hex(0xecdba6, 0x6b3410)
    static let green = hex(0x7fd06a, 0x2a6d1c)
    static let amber = hex(0xf29b4b, 0x7a5f00)
    static let red = hex(0xff7d6e, 0xb81d3c)

    static let radius: CGFloat = 6
    static let hit: CGFloat = 44

    static func font(_ size: CGFloat = 13, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }

    static func color(_ s: ResultState) -> Color {
        switch s {
        case .up: green
        case .down: red
        case .unknown: mute
        }
    }

    /// `HH:mm` today, `MMM d` earlier.
    static func stamp(_ date: Date) -> String {
        date.formatted(Calendar.current.isDateInToday(date)
            ? .dateTime.hour(.twoDigits(amPM: .omitted)).minute(.twoDigits)
            : .dateTime.month(.abbreviated).day())
    }
}

struct Dot: View {
    let color: Color
    var body: some View { Circle().fill(color).frame(width: 7, height: 7) }
}
