//! 重い解析（tokens / lines / todos / outline）の並列プリウォーム（native 専用）。
//!
//! `file_extras` が参照する重い項目はファイル内容と cfg だけで決まる純粋な計算で、
//! I/O と CPU（BPE トークナイズ・AST パース）が集中する。走査を始める前に全ファイル
//! 分をワーカースレッドで先に計算し `Session` のキャッシュへ入れておくことで、
//! その後の（順序を保った）直列レンダリングはキャッシュ参照だけで済む。
//! 出力は入力だけで決まるため、直列実行と完全にバイト一致する。
//!
//! std::thread を使うためコア本体からは feature gate 越しに分離している
//! （wasm ビルドではこのモジュールをコンパイルしない）。

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::analysis::extras::compute_heavy_extras;
use crate::cfg::Cfg;
use crate::filter::filter_entries;
use crate::gitignore::{extend_pats, relpath_slash};
use crate::provider::{Entry, FsProvider};
use crate::render_text::deep_stats_wanted;
use crate::session::Session;

/// レンダリング（および -L 切り詰め時の全階層集計）で参照されるファイルを列挙する。
/// symlink ディレクトリは循環回避のため辿らない（辿った先のファイルはキャッシュ
/// ミスで直列計算に落ちるだけで、結果は変わらない）。
fn collect_files<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
    depth: i64,
    visit_limit: Option<i64>,
    out: &mut Vec<(Entry, String)>,
) {
    if let Some(md) = visit_limit {
        if depth >= md {
            return;
        }
    }
    let cur_pats = extend_pats(sess, active_pats, path, cfg);
    let Some((dirs, files)) = filter_entries(sess, path, cfg, &cur_pats) else {
        return;
    };
    for f in files {
        let rel = relpath_slash(&f.path, &cfg.root);
        out.push((f, rel));
    }
    for d in dirs {
        if d.is_dir_nofollow {
            collect_files(sess, &d.path, cfg, &cur_pats, depth + 1, visit_limit, out);
        }
    }
}

/// 全ファイルの重い解析を並列で先に計算し、`Session` のキャッシュへ入れる。
/// need_text 系フラグ（-T / -K / -O）が無いときは重い処理が発生しないため何もしない。
pub fn warm_extras_parallel<F: FsProvider + Sync>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) {
    // 本文読込を伴う解析が無ければ、事前計算する重い項目が無い。
    if !(cfg.show_tokens || cfg.show_todo || cfg.show_outline) {
        return;
    }

    // 単一コアでは並列化の得が無いので、ファイル列挙もスレッド生成もせず
    // 直列パス（file_extras がその場で計算）に丸ごと任せる。ここで早期に
    // 判定することで、単一コア環境に列挙ウォークのオーバーヘッドを課さない。
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(16);
    if cores < 2 {
        return;
    }

    // レンダリングが参照する範囲だけを列挙する（-L 切り詰め時、全階層集計を出す
    // モードでは全階層、そうでなければ表示深さまで）。
    let visit_limit = if deep_stats_wanted(cfg) {
        None
    } else {
        cfg.max_depth
    };

    let mut targets: Vec<(Entry, String)> = Vec::new();
    collect_files(sess, &cfg.root, cfg, active_pats, 0, visit_limit, &mut targets);
    if targets.len() < 2 {
        return;
    }

    let workers = cores.min(targets.len());
    let queue = Mutex::new(targets);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                // 1 件ずつ取り出す（キューロックは短時間）。
                let next = queue.lock().unwrap().pop();
                let Some((entry, rel)) = next else { break };
                let heavy = compute_heavy_extras(sess, &entry, &rel, cfg);
                sess.insert_heavy(entry.path, heavy);
            });
        }
    });
}
