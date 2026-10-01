//! 実バイナリを起動して確認する回帰テスト（BUG_REPORT 対応分）。

use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dirlens_cli_reg_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn dirlens(cwd: &Path, args: &[&str]) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_dirlens"))
        .args(args)
        .arg("--no-config")
        .current_dir(cwd)
        .env("DIRLENS_CACHE", "off")
        .env("DIRLENS_GITIGNORE", "builtin")
        .output()
        .expect("spawn dirlens");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// --compare -G: 比較先（B）のルート .gitignore も B 側に適用される
/// （以前は A の無視パターンを使い回し、B 直下の .gitignore が読まれなかった）。
#[test]
fn compare_applies_each_trees_own_gitignore() {
    let root = temp_dir("compare");
    let a = root.join("a");
    let b = root.join("b");
    for (dir, only) in [(&a, "a_only.txt"), (&b, "b_only.txt")] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(".gitignore"), format!("{}\n", only)).unwrap();
        std::fs::write(dir.join(only), "x").unwrap();
        std::fs::write(dir.join("common.txt"), "same").unwrap();
    }
    for (cwd, other) in [(&a, &b), (&b, &a)] {
        let (out, err) = dirlens(cwd, &["-G", "--compare", other.to_str().unwrap()]);
        assert!(!out.contains("a_only.txt"), "{}{}", out, err);
        assert!(!out.contains("b_only.txt"), "{}{}", out, err);
        assert!(out.contains("No differences found."), "{}{}", out, err);
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// バイナリ拡張子でも中身がテキストならトークン・TODO を数える。
#[test]
fn disguised_text_with_binary_extension_is_analyzed() {
    let root = temp_dir("sniff");
    std::fs::write(root.join("payload.png"), "# TODO: hidden\nhello world\n").unwrap();
    std::fs::write(root.join("real.png"), b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR").unwrap();
    let (out, err) = dirlens(&root, &["-T", "-K", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}{err}"));
    let child = |name: &str| {
        v["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .cloned()
            .unwrap()
    };
    assert!(child("payload.png")["tokens"].as_i64().unwrap_or(0) > 0);
    assert!(child("real.png")["tokens"].is_null());
    let _ = std::fs::remove_dir_all(&root);
}

/// 不正な DIRLENS_MAX_FILE_BYTES は警告を出し、固定の 5MB ではなく
/// 未指定時と同じ上限（メモリ量に応じた動的値）になる。
#[test]
fn invalid_max_file_bytes_warns_and_matches_unset_limit() {
    let root = temp_dir("maxbytes");
    let run = |val: Option<&str>| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dirlens"));
        cmd.args(["--check", "--json", "--no-config"]).current_dir(&root);
        cmd.env_remove("DIRLENS_MAX_FILE_BYTES").env_remove("DIRLENS_COMPAT");
        if let Some(v) = val {
            cmd.env("DIRLENS_MAX_FILE_BYTES", v);
        }
        let out = cmd.output().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        (
            v["capabilities"]["max_file_bytes"].clone(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (unset, _) = run(None);
    let (bad, warn) = run(Some("12MB"));
    assert_eq!(bad, unset);
    assert!(warn.contains("invalid DIRLENS_MAX_FILE_BYTES"), "{}", warn);
    let _ = std::fs::remove_dir_all(&root);
}
