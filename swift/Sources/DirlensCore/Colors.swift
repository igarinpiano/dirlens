// ANSI カラー（rust/crates/dirlens-core/src/colors.rs の等価移植）。

public enum Ansi {
    public static let reset = "\u{1b}[0m"
    public static let bold = "\u{1b}[1m"
    public static let dim = "\u{1b}[2m"
    public static let blue = "\u{1b}[34m"
    public static let cyan = "\u{1b}[36m"
    public static let green = "\u{1b}[32m"
    public static let magenta = "\u{1b}[35m"
    public static let red = "\u{1b}[31m"
    public static let yellow = "\u{1b}[33m"
}

/// Python の `c(text, *codes)` 相当。
public func c(_ text: String, _ codes: [String], _ useColor: Bool) -> String {
    if useColor {
        return codes.joined() + text + Ansi.reset
    }
    return text
}

/// Python の `strip_ansi`（`\033\[[0-9;]*[mK]` を除去）相当。
public func stripAnsi(_ text: String) -> String {
    let chars = Array(text.unicodeScalars)
    var out = String.UnicodeScalarView()
    out.reserveCapacity(chars.count)
    var i = 0
    while i < chars.count {
        if chars[i] == "\u{1b}", i + 1 < chars.count, chars[i + 1] == "[" {
            var j = i + 2
            while j < chars.count, (chars[j].value >= 48 && chars[j].value <= 57) || chars[j] == ";" {
                j += 1
            }
            if j < chars.count, chars[j] == "m" || chars[j] == "K" {
                i = j + 1
                continue
            }
        }
        out.append(chars[i])
        i += 1
    }
    return String(out)
}
