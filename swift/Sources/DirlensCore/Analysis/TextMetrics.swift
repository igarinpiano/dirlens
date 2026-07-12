// トークン数概算（-T）と行数カウント
// （rust/crates/dirlens-core/src/analysis/text_metrics.rs の等価移植）。

/// テキスト本文の最大読み込みバイト数（dirlens.py の _TEXT_READ_LIMIT）。
public let textReadLimit = 5_000_000

private let binaryExts: Set<String> = [
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".webp",
    ".mp3", ".mp4", ".mov", ".avi", ".wav", ".flac", ".ogg", ".webm", ".mkv",
    ".zip", ".tar", ".gz", ".rar", ".7z", ".bz2", ".xz",
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    ".exe", ".dll", ".so", ".dylib", ".bin", ".o", ".a", ".class", ".jar",
    ".woff", ".woff2", ".ttf", ".otf", ".eot",
    ".db", ".sqlite", ".sqlite3", ".pyc", ".pyo", ".whl",
]

public func isProbablyBinary(_ name: String) -> Bool {
    let (_, ext) = splitext(name.lowercased())
    return binaryExts.contains(ext)
}

/// このビルド・環境で BPE 計数が使えるか（--check / capabilities 用）。
public func bpeAvailable() -> Bool {
    return O200KTokenizer.shared != nil
}

/// トークン計数の 2 層エントリポイント。
/// Tier1: BPE（o200k_base・語彙同梱）による正確値（打ち切り時はスケール補正で概算に戻る）。
/// Tier2: 文字数ヒューリスティック（Python 版と同一式）へ縮退。
public func countTokens(
    _ text: String, _ byteLen: Int, _ actualSize: UInt64?, _ truncated: Bool, _ preferBpe: Bool
) -> Int64 {
    if text.isEmpty { return 0 }
    if preferBpe, let enc = O200KTokenizer.shared {
        var tokens = Double(enc.countTokens(text))
        if truncated, let sz = actualSize, sz != 0, byteLen > 0 {
            tokens *= Double(sz) / Double(byteLen)
        }
        return max(1, pyRound(tokens))
    }
    return estimateTokens(text, byteLen, actualSize, truncated)
}

/// トークン数概算（Tier2）。英数字記号は約4文字/トークン、それ以外は約1.5文字/トークン。
/// 打ち切り時は実サイズとの比でスケール補正する。
public func estimateTokens(_ text: String, _ byteLen: Int, _ actualSize: UInt64?, _ truncated: Bool) -> Int64 {
    if text.isEmpty { return 0 }
    var asciiChars: Int64 = 0
    var otherChars: Int64 = 0
    for ch in text.unicodeScalars {
        if ch.value < 128 {
            asciiChars += 1
        } else {
            otherChars += 1
        }
    }
    var tokens = Double(asciiChars) / 4.0 + Double(otherChars) / 1.5
    if truncated, let sz = actualSize, sz != 0, byteLen > 0 {
        tokens *= Double(sz) / Double(byteLen)
    }
    return max(1, pyRound(tokens))
}

/// 行数カウント（打ち切り時はスケール補正）。
public func countLines(_ text: String, _ byteLen: Int, _ actualSize: UInt64?, _ truncated: Bool) -> Int64 {
    if text.isEmpty { return 0 }
    var n: Int64 = 0
    for b in text.utf8 where b == 0x0A {
        n += 1
    }
    if text.unicodeScalars.last != "\n" {
        n += 1
    }
    if truncated, let sz = actualSize, sz != 0, byteLen > 0 {
        n = max(1, pyRound(Double(n) * Double(sz) / Double(byteLen)))
    }
    return n
}
