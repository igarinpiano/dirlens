//! 設定ファイル（native CLI のみ・コアは関与しない）。
//!
//! 2層 + プリセット:
//!   1. グローバル: `$XDG_CONFIG_HOME/dirlens/config.toml`（無ければ `~/.config/dirlens/config.toml`）
//!   2. プロジェクト: 対象ディレクトリから上方向に探索した最初の `.dirlens.toml`
//!
//! 優先順: CLI フラグ > プロジェクト設定 > グローバル設定 > 既定値。
//! ブールは「設定で true → 有効化」のみ（CLI に無効化フラグが無いため）。
//! `DIRLENS_CONFIG=off` または `--no-config` で全設定ファイルを無視する
//! （ゴールデンテスト・CI 等の決定論性が必要な場面用）。
//! `DIRLENS_COMPAT=python` でも無視する（旧 Python 版に設定ファイル機能は無い）。
//!
//! ```toml
//! lang = "ja"
//! gitignore = true
//! emoji = true
//! depth = 3
//! exclude = ["dist", "*.log"]
//!
//! [presets]
//! quick = ["-L", "2", "-G"]
//! paste = ["--ai", "-L", "3"]
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub lang: Option<String>,
    pub gitignore: Option<bool>,
    pub all: Option<bool>,
    pub date: Option<bool>,
    pub emoji: Option<bool>,
    pub markdown: Option<bool>,
    pub no_color: Option<bool>,
    pub bar: Option<bool>,
    pub prune: Option<bool>,
    pub filesfirst: Option<bool>,
    pub follow: Option<bool>,
    pub full_path: Option<bool>,
    pub depth: Option<i64>,
    pub min_size: Option<String>,
    pub max_size: Option<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub presets: BTreeMap<String, Vec<String>>,
}

impl FileConfig {
    /// other（優先側）を self に上書きマージする。
    fn overlay(&mut self, other: FileConfig) {
        macro_rules! ov {
            ($($f:ident),*) => { $( if other.$f.is_some() { self.$f = other.$f; } )* };
        }
        ov!(lang, gitignore, all, date, emoji, markdown, no_color, bar, prune,
            filesfirst, follow, full_path, depth, min_size, max_size);
        // 配列は「優先側で置き換え」ではなく連結（グローバル + プロジェクト両方効く）
        self.exclude.extend(other.exclude);
        self.include.extend(other.include);
        for (k, v) in other.presets {
            self.presets.insert(k, v);
        }
    }
}

/// 設定ファイルが無効化されているか。
pub fn config_disabled(argv: &[String]) -> bool {
    std::env::var("DIRLENS_CONFIG").as_deref() == Ok("off")
        || std::env::var("DIRLENS_COMPAT").as_deref() == Ok("python")
        || argv.iter().any(|a| a == "--no-config")
}

fn global_config_path() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return Some(PathBuf::from(x).join("dirlens").join("config.toml"));
        }
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".config").join("dirlens").join("config.toml"))
}

/// 対象ディレクトリから上方向へ `.dirlens.toml` を探す。
fn project_config_path(target: &Path) -> Option<PathBuf> {
    let mut dir = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(target)
    };
    loop {
        let cand = dir.join(".dirlens.toml");
        if cand.is_file() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn parse_file(path: &Path) -> Result<FileConfig, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: {}", path.display(), e))?;
    toml::from_str(&text).map_err(|e| format!("{}: {}", path.display(), e))
}

/// グローバル + プロジェクトの設定を読み込む（プロジェクト優先）。
/// 戻り値: (マージ済み設定, 警告メッセージ列)。
/// 壊れた設定ファイルは無視して警告に積む（起動を妨げない）。
pub fn load(target: &Path) -> (FileConfig, Vec<String>) {
    let mut cfg = FileConfig::default();
    let mut warnings = Vec::new();
    if let Some(p) = global_config_path() {
        if p.is_file() {
            match parse_file(&p) {
                Ok(c) => cfg.overlay(c),
                Err(e) => warnings.push(format!("dirlens: config ignored ({})", e)),
            }
        }
    }
    if let Some(p) = project_config_path(target) {
        match parse_file(&p) {
            Ok(c) => cfg.overlay(c),
            Err(e) => warnings.push(format!("dirlens: config ignored ({})", e)),
        }
    }
    (cfg, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_overlay() {
        let mut base: FileConfig = toml::from_str(
            r#"
            lang = "en"
            gitignore = true
            exclude = ["dist"]
            [presets]
            quick = ["-L", "2"]
            "#,
        )
        .unwrap();
        let over: FileConfig = toml::from_str(
            r#"
            lang = "ja"
            emoji = true
            exclude = ["*.log"]
            [presets]
            paste = ["--ai"]
            "#,
        )
        .unwrap();
        base.overlay(over);
        assert_eq!(base.lang.as_deref(), Some("ja"));
        assert_eq!(base.gitignore, Some(true));
        assert_eq!(base.emoji, Some(true));
        assert_eq!(base.exclude, vec!["dist", "*.log"]);
        assert_eq!(base.presets.len(), 2);
    }

    #[test]
    fn unknown_key_is_error() {
        assert!(toml::from_str::<FileConfig>("nope = 1").is_err());
    }
}
