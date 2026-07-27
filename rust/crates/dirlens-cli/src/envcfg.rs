//! 環境変数による `Cfg` の上書き。CLI 経路（`main()`）と MCP 経路（`mcp.rs`）の
//! 両方から呼ぶ共通ロジック。
//!
//! 以前は `--mcp` が `main()` 冒頭で早期 return するため、このファイルの内容
//! （DIRLENS_GITIGNORE / DIRLENS_AST / DIRLENS_TOKENS / DIRLENS_MAX_WORKERS /
//! DIRLENS_MAX_FILE_BYTES / DIRLENS_COMPAT の反映）が MCP 経由の呼び出しには
//! 一切効かなかった。同じバイナリ・同じプロセス環境で動く以上、どちらの経路でも
//! 同じ環境変数が同じ意味を持つべきなので、ここへ切り出して両方から呼ぶ。

use dirlens_core::Cfg;

/// `apply` の結果。呼び出し側が永続キャッシュの有効/無効判定に使う。
pub(crate) struct EnvConfig {
    pub compat_python: bool,
}

/// DIRLENS_* 環境変数を `cfg` に反映する。
pub(crate) fn apply(cfg: &mut Cfg) -> EnvConfig {
    // gitignore 層の選択（テスト・検証用の環境変数。通常は auto = Tier1 を試す）:
    //   DIRLENS_GITIGNORE=builtin … 内蔵マッチャ（Tier3）を強制
    //   DIRLENS_COMPAT=python     … Python 版完全互換モード（ゴールデン検証用）
    let compat_python = std::env::var("DIRLENS_COMPAT").as_deref() == Ok("python");
    match std::env::var("DIRLENS_GITIGNORE").as_deref() {
        Ok("builtin") => cfg.gitignore_prefer_git = false,
        Ok("git") => cfg.gitignore_prefer_git = true,
        _ => {
            if compat_python {
                cfg.gitignore_prefer_git = false;
            }
        }
    }
    // AST 第1段＋import 解決改善の無効化（DIRLENS_AST=off または互換モード）
    if compat_python || std::env::var("DIRLENS_AST").as_deref() == Ok("off") {
        cfg.enhanced_analysis = false;
    }
    // トークン計数層の選択（DIRLENS_TOKENS=heuristic で Tier2 固定）
    if compat_python || std::env::var("DIRLENS_TOKENS").as_deref() == Ok("heuristic") {
        cfg.tokens_bpe = false;
    }
    // 並列ワーカー数の上限の上書き（DIRLENS_MAX_WORKERS）。高コア機で既定 64 を
    // 超えて使いたい場合や、CPU 制限付きコンテナ等で絞りたい場合に指定する。
    // 1 未満・数値でない値は無視して既定に従う。
    if let Ok(v) = std::env::var("DIRLENS_MAX_WORKERS") {
        if let Ok(n) = v.trim().parse::<usize>() {
            if n >= 1 {
                cfg.max_workers = Some(n);
            }
        }
    }
    // 本文読み込み・BPE正確計数の対象にする1ファイルあたりの上限（既定 5MB）を、
    // マシンの物理メモリ量に応じて動的に引き上げる（DIRLENS_MAX_FILE_BYTES で
    // 明示指定した場合はそちらを優先。互換モードは Python 版とバイト一致させる
    // 検証用なので固定 5MB のまま変えない）。メモリ量を検出できない環境・取得
    // 失敗時は total_memory_bytes() が None を返し、resolve_text_read_limit が
    // default（＝ここに渡す既存の cfg.text_read_limit = TEXT_READ_LIMIT）へ
    // フォールバックする（sysmem::tests で分岐を個別に検証済み）。
    cfg.text_read_limit = crate::sysmem::resolve_text_read_limit(
        std::env::var("DIRLENS_MAX_FILE_BYTES").ok().as_deref(),
        compat_python,
        crate::sysmem::total_memory_bytes(),
        cfg.text_read_limit,
    );
    // 互換モードでは精度注記・schema_version・capabilities も出さない。
    // --agent/--ai バンドルに含まれる --status も Python 版に無いため無効化する
    if compat_python {
        cfg.suppress_notes = true;
        cfg.show_status = false;
    }
    EnvConfig { compat_python }
}

/// 永続トークンキャッシュ（DIRLENS_CACHE=off で無効化）を使うか。互換モードでは
/// Python 版とのバイト一致検証を汚さないよう常に無効。CLI の `--no-cache` フラグは
/// 呼び出し側で別途 AND すること（MCP には該当フラグが無いためここには含めない）。
pub(crate) fn cache_env_enabled(compat_python: bool) -> bool {
    std::env::var("DIRLENS_CACHE").as_deref() != Ok("off") && !compat_python
}
