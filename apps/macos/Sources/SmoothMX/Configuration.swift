import Foundation

struct MouseAction: Codable, Equatable {
    var kind: UInt32 = 0
    var code: UInt32 = 0
    var modifiers: UInt32 = 0 // command=1, shift=2, option=4, control=8
    var words: [UInt32] { [kind, code, modifiers] }
    static let presets: [(String, MouseAction)] = [
        ("Do nothing", .init()),
        ("Back", .init(kind: 1, code: 33, modifiers: 1)),
        ("Forward", .init(kind: 1, code: 30, modifiers: 1)),
        ("Middle click", .init(kind: 2, code: 2)),
        ("Copy", .init(kind: 1, code: 8, modifiers: 1)),
        ("Paste", .init(kind: 1, code: 9, modifiers: 1)),
        ("Undo", .init(kind: 1, code: 6, modifiers: 1)),
        ("Redo", .init(kind: 1, code: 6, modifiers: 3)),
        ("Mission Control", .init(kind: 1, code: 126, modifiers: 8)),
        ("App windows", .init(kind: 1, code: 125, modifiers: 8)),
        ("Desktop left", .init(kind: 1, code: 123, modifiers: 8)),
        ("Desktop right", .init(kind: 1, code: 124, modifiers: 8)),
        ("Show desktop", .init(kind: 1, code: 103)),
        ("Switch application", .init(kind: 1, code: 48, modifiers: 1)),
        ("Volume up", .init(kind: 4, code: 0)),
        ("Volume down", .init(kind: 4, code: 1)),
        ("Mute", .init(kind: 4, code: 7)),
        ("Play / pause", .init(kind: 4, code: 16)),
        ("Next track", .init(kind: 4, code: 17)),
        ("Previous track", .init(kind: 4, code: 18)),
        ("Toggle wheel mode", .init(kind: 5))
    ]

    var needsAccessibility: Bool { kind == 1 || kind == 2 || kind == 4 }

    func validate() throws {
        let valid = switch kind {
        case 0: code == 0 && modifiers == 0
        case 1: code <= 127 && modifiers <= 15
        case 2: code <= 4 && modifiers == 0
        case 4: [0, 1, 7, 16, 17, 18].contains(code) && modifiers == 0
        case 5: code == 0 && modifiers == 0
        default: false
        }
        guard valid else { throw ConfigurationError.invalid }
    }
}
struct ButtonBinding: Codable, Equatable {
    var enabled = false
    var gesture = false
    var actions = Array(repeating: MouseAction(), count: 5)
}
struct MouseSettings: Codable, Equatable {
    var dpi: UInt32 = 0
    var wheelMode: UInt32 = 2
    var smartShift = true
    var threshold: Double = 16
    var vertical = false
    var verticalSpeed: Double = 100
    var verticalInvert = false
    var smooth = true
    var horizontal = false
    var horizontalSpeed: Double = 100
    var horizontalInvert = false
    var horizontalActions = [MouseAction(), MouseAction()]
    var gestureThreshold: Double = 80
    var buttons = Array(repeating: ButtonBinding(), count: 5)
    var needsAccessibility: Bool {
        if vertical { return true }
        if horizontal {
            let configured = horizontalActions.contains { $0.kind != 0 }
            if !configured || horizontalActions.contains(where: \.needsAccessibility) { return true }
        }
        return buttons.contains { binding in
            guard binding.enabled else { return false }
            let actions = binding.gesture ? binding.actions[...] : binding.actions.prefix(1)
            return actions.contains(where: \.needsAccessibility)
        }
    }
    var words: [UInt32] {
        var w = Array(repeating: UInt32(0), count: 105)
        w[0] = 1; w[1] = dpi; w[2] = smartShift ? 2 : wheelMode
        w[3] = smartShift ? UInt32(threshold) : 255
        w[4] = vertical ? 1 : 0; w[5] = UInt32(verticalSpeed)
        w[6] = verticalInvert ? 1 : 0; w[7] = smooth ? 1 : 0
        w[8] = horizontal ? 1 : 0; w[9] = UInt32(horizontalSpeed)
        w[10] = horizontalInvert ? 1 : 0; w[11] = UInt32(gestureThreshold)
        for i in 0..<2 { w.replaceSubrange((12+i*3)..<(15+i*3), with: horizontalActions[i].words) }
        for i in 0..<5 {
            let p = 20+i*17
            w[p] = buttons[i].enabled ? 1 : 0; w[p+1] = buttons[i].gesture ? 1 : 0
            for j in 0..<5 { w.replaceSubrange((p+2+j*3)..<(p+5+j*3), with: buttons[i].actions[j].words) }
        }
        return w
    }
    func validate() throws {
        guard buttons.count == 5, buttons.allSatisfy({ $0.actions.count == 5 }), horizontalActions.count == 2,
              threshold.isFinite, (1...254).contains(threshold), verticalSpeed.isFinite,
              (10...500).contains(verticalSpeed), horizontalSpeed.isFinite, (10...500).contains(horizontalSpeed),
              gestureThreshold.isFinite, (10...2000).contains(gestureThreshold), dpi <= 57343,
              (1...2).contains(wheelMode) else { throw ConfigurationError.invalid }
        for action in horizontalActions { try action.validate() }
        for binding in buttons {
            for action in binding.actions { try action.validate() }
        }
    }
}
enum ConfigurationError: Error { case invalid }
struct ApplicationProfile: Codable, Identifiable, Equatable {
    var id: String
    var name: String
    var settings: MouseSettings
    static let global = ApplicationProfile(id: "*", name: "All applications", settings: MouseSettings())
}
struct Configuration: Codable {
    var version = 1
    var profiles = [ApplicationProfile.global]
    func validate() throws {
        guard version == 1, !profiles.isEmpty, profiles.count <= 100, profiles.filter({ $0.id == "*" }).count == 1,
              Set(profiles.map(\.id)).count == profiles.count else { throw ConfigurationError.invalid }
        for profile in profiles { try profile.settings.validate() }
    }
}
