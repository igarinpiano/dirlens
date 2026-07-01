//! 絵文字アイコン（--emoji）。dirlens.py のテーブルをそのまま移植。

use crate::fmt::splitext;

const EMOJI_EXT: &[(&str, &str)] = &[
    (".py", "🐍"), (".js", "🟨"), (".ts", "🔷"), (".jsx", "⚛️"), (".tsx", "⚛️"),
    (".rs", "🦀"), (".go", "🐹"), (".rb", "💎"), (".java", "☕"), (".kt", "🟣"),
    (".c", "🔧"), (".cpp", "🔧"), (".h", "🔧"), (".cs", "🔵"), (".php", "🐘"),
    (".swift", "🍊"), (".dart", "🎯"),
    (".json", "📋"), (".yaml", "⚙️"), (".yml", "⚙️"), (".toml", "⚙️"), (".xml", "📰"),
    (".csv", "📊"), (".sql", "🗄️"), (".db", "🗄️"), (".ini", "⚙️"), (".env", "🔑"),
    (".md", "📝"), (".txt", "📄"), (".pdf", "📕"), (".doc", "📘"), (".docx", "📘"),
    (".html", "🌐"), (".css", "🎨"), (".scss", "🎨"),
    (".png", "🖼️"), (".jpg", "🖼️"), (".jpeg", "🖼️"), (".gif", "🖼️"),
    (".svg", "🎨"), (".ico", "🖼️"), (".webp", "🖼️"),
    (".mp4", "🎬"), (".mov", "🎬"), (".mp3", "🎵"), (".wav", "🎵"), (".flac", "🎵"),
    (".zip", "📦"), (".tar", "📦"), (".gz", "📦"), (".rar", "📦"), (".7z", "📦"),
    (".sh", "📜"), (".bash", "📜"), (".zsh", "📜"), (".bat", "📜"), (".ps1", "📜"),
];

const EMOJI_NAME: &[(&str, &str)] = &[
    ("dockerfile", "🐳"), ("makefile", "⚙️"), ("license", "⚖️"),
    (".gitignore", "🚫"), ("package.json", "📦"), ("requirements.txt", "📋"),
    ("pyproject.toml", "⚙️"), ("cargo.toml", "📦"), ("readme.md", "📖"),
];

pub fn get_emoji(name: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "📁";
    }
    let lower = name.to_lowercase();
    if let Some((_, e)) = EMOJI_NAME.iter().find(|(n, _)| *n == lower) {
        return e;
    }
    let (_, ext) = splitext(&lower);
    EMOJI_EXT
        .iter()
        .find(|(x, _)| *x == ext)
        .map(|(_, e)| *e)
        .unwrap_or("📄")
}
