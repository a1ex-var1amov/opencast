import Foundation

/// Emoji → SF Symbol for launcher rows. Pure (no AppKit) so panel-logic tests can
/// assert every `BUILTINS` icon is covered (P13-1).
enum Glyphs {
    /// Keys have U+FE0F / U+FE0E stripped so ⚙️ and ⚙ share one row.
    static let byEmoji: [String: String] = [
        "\u{1F4AC}": "bubble.left.and.bubble.right.fill", // 💬
        "\u{1F504}": "arrow.clockwise", // 🔄
        "\u{1F4C1}": "folder.fill", // 📁
        "\u{1F4C2}": "folder.fill", // 📂
        "\u{23FB}": "power", // ⏻
        "\u{1F5A5}": "desktopcomputer", // 🖥️
        "\u{2600}": "sun.max.fill", // ☀
        "\u{1F319}": "moon.stars.fill", // 🌙
        "\u{1F3DD}": "rectangle.topthird.inset.filled", // 🏝
        "\u{1F526}": "flashlight.on.fill", // 🔦
        "\u{2699}": "gearshape.fill", // ⚙
        "\u{2328}": "keyboard.fill", // ⌨
        "\u{1F4A4}": "zzz", // 💤
        "\u{2615}": "cup.and.saucer.fill", // ☕
        "\u{1F4CB}": "clipboard.fill", // 📋
        "\u{1F600}": "face.smiling.fill", // 😀
        "\u{1F4DD}": "note.text", // 📝
        "\u{1F317}": "circle.lefthalf.filled", // 🌗
        "\u{1F50A}": "speaker.wave.3.fill", // 🔊
        "\u{1F509}": "speaker.wave.1.fill", // 🔉
        "\u{1F507}": "speaker.slash.fill", // 🔇
        "\u{1F324}": "sun.min.fill", // 🌤
        "\u{1F5D1}": "trash.fill", // 🗑
        "\u{1F512}": "lock.fill", // 🔒
        "\u{1F634}": "moon.zzz.fill", // 😴
        "\u{1F4BF}": "opticaldisc", // 💿
        "\u{2715}": "xmark", // ✕
        "\u{2620}": "xmark.octagon.fill", // ☠
        "\u{1F3AF}": "viewfinder", // 🎯
        "\u{1F4F8}": "camera.viewfinder", // 📸
        "\u{1F5C2}": "square.stack.3d.up.fill", // 🗂
        "\u{2728}": "sparkles", // ✨
        "\u{1F4DC}": "terminal.fill", // 📜
        "\u{1F517}": "link", // 🔗
        "\u{2702}": "scissors", // ✂
        "\u{1F4C4}": "doc.fill", // 📄
        "\u{1F5BC}": "photo.fill", // 🖼
        "\u{1F321}": "thermometer.medium", // 🌡
        "\u{25E7}": "rectangle.lefthalf.filled",
        "\u{25E8}": "rectangle.righthalf.filled",
        "\u{2B12}": "rectangle.tophalf.filled",
        "\u{2B13}": "rectangle.bottomhalf.filled",
        "\u{25F0}": "rectangle.inset.topleft.filled",
        "\u{25F3}": "rectangle.inset.topright.filled",
        "\u{25F1}": "rectangle.inset.bottomleft.filled",
        "\u{25F2}": "rectangle.inset.bottomright.filled",
        "\u{26F6}": "arrow.up.left.and.arrow.down.right",
        "\u{25CE}": "circle.circle.fill",
        "\u{25A3}": "square.grid.2x2.fill", // ▣
        "\u{2192}": "arrow.right",
        "\u{2190}": "arrow.left",
        "\u{21A9}": "arrow.uturn.backward",
        "\u{21B5}": "return", // ↩
        "\u{21E5}": "arrow.right.to.line.compact", // ⇥
        "\u{238B}": "escape", // ⎋
        "\u{1F9EE}": "plus.forwardslash.minus", // 🧮
        "\u{1F418}": "globe", // 🐙
        "\u{1F50D}": "magnifyingglass", // 🔎
        "\u{1FA7A}": "stethoscope", // doctor
        // Pass-13: remaining builtins + dynamic fallbacks
        "\u{2630}": "line.3.horizontal", // ☰
        "\u{1F399}": "mic.fill", // 🎙
        "\u{1F4C5}": "calendar", // 📅
        "\u{2601}": "icloud.fill", // ☁
        "\u{2B06}": "arrow.up.circle.fill", // ⬆
        "\u{1F50C}": "powerplug.fill", // 🔌
        "\u{1F9E9}": "puzzlepiece.extension.fill", // 🧩
        "\u{1F3A9}": "square.and.arrow.down.fill", // 🎩
        "\u{2705}": "checkmark.circle.fill", // ✅
        "\u{1F464}": "person.crop.circle.fill", // 👤
        "\u{2611}": "checklist", // ☑
        "\u{1F4D6}": "character.book.closed.fill", // 📖
        "\u{1F520}": "textformat.size", // 🔠
        "\u{23EF}": "playpause.fill", // ⏯
        "\u{23ED}": "forward.end.fill", // ⏭
        "\u{23EE}": "backward.end.fill", // ⏮
        "\u{1F3A7}": "music.note", // 🎧
        "\u{2733}": "text.cursor", // ✳
        "\u{23F8}": "pause.fill", // ⏸
        "\u{1F516}": "bookmark.fill", // 🔖
        "\u{1F558}": "clock.fill", // 🕘
        "\u{1F501}": "arrow.triangle.2.circlepath", // 🔁
        "\u{1F6AA}": "rectangle.portrait.and.arrow.right", // 🚪
        "\u{1F441}": "eye.fill", // 👁
        "\u{1F648}": "eye.slash.fill", // 🙈
        "\u{1FA9F}": "macwindow", // 🪟
        "\u{1F310}": "globe", // 🌐
        "\u{1F9ED}": "safari.fill", // 🧭
        "\u{1F4F0}": "doc.text.fill", // 📰
        "\u{2696}": "scalemass.fill", // ⚖
        "\u{1F5D2}": "note.text", // 🗒
        "\u{1F524}": "textformat.abc", // 🔤
        "\u{1F4CE}": "paperclip", // 📎
        "\u{1F4CA}": "chart.bar.fill", // 📊
        "\u{1FAAA}": "person.text.rectangle.fill", // 🪪
        "\u{26D3}": "link", // ⛓
        // 🧑‍💻 person + ZWJ + laptop
        "\u{1F9D1}\u{200D}\u{1F4BB}": "person.fill.viewfinder",
    ]
}
