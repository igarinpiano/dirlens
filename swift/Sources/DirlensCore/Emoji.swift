// 絵文字アイコン（--emoji）。rust/crates/dirlens-core/src/emoji.rs の等価移植。

private let emojiExt: [String: String] = [
    ".py": "🐍", ".js": "🟨", ".ts": "🔷", ".jsx": "⚛️", ".tsx": "⚛️",
    ".rs": "🦀", ".go": "🐹", ".rb": "💎", ".java": "☕", ".kt": "🟣",
    ".c": "🔧", ".cpp": "🔧", ".h": "🔧", ".cs": "🔵", ".php": "🐘",
    ".swift": "🍊", ".dart": "🎯",
    ".json": "📋", ".yaml": "⚙️", ".yml": "⚙️", ".toml": "⚙️", ".xml": "📰",
    ".csv": "📊", ".sql": "🗄️", ".db": "🗄️", ".ini": "⚙️", ".env": "🔑",
    ".md": "📝", ".txt": "📄", ".pdf": "📕", ".doc": "📘", ".docx": "📘",
    ".html": "🌐", ".css": "🎨", ".scss": "🎨",
    ".png": "🖼️", ".jpg": "🖼️", ".jpeg": "🖼️", ".gif": "🖼️",
    ".svg": "🎨", ".ico": "🖼️", ".webp": "🖼️",
    ".mp4": "🎬", ".mov": "🎬", ".mp3": "🎵", ".wav": "🎵", ".flac": "🎵",
    ".zip": "📦", ".tar": "📦", ".gz": "📦", ".rar": "📦", ".7z": "📦",
    ".sh": "📜", ".bash": "📜", ".zsh": "📜", ".bat": "📜", ".ps1": "📜",
]

private let emojiName: [String: String] = [
    "dockerfile": "🐳", "makefile": "⚙️", "license": "⚖️",
    ".gitignore": "🚫", "package.json": "📦", "requirements.txt": "📋",
    "pyproject.toml": "⚙️", "cargo.toml": "📦", "readme.md": "📖",
    "package.swift": "📦",
]

public func getEmoji(_ name: String, isDir: Bool) -> String {
    if isDir { return "📁" }
    let lower = name.lowercased()
    if let e = emojiName[lower] { return e }
    let (_, ext) = splitext(lower)
    return emojiExt[ext] ?? "📄"
}
