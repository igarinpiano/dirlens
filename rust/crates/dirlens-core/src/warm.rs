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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::analysis::extras::{compute_heavy_extras, HeavyExtras};
use crate::cfg::Cfg;
use crate::filter::filter_entries;
use crate::gitignore::{extend_pats, relpath_slash};
use crate::provider::{Entry, FsProvider};
use crate::render_text::deep_stats_wanted;
use crate::session::Session;

/// ワーカースレッド数の既定上限。逐次律速（アムダール・~10%）で漸近点に
/// 64 スレッド付近で到達するうえ、スレッド生成/スタックの固定コストや
/// メモリ帯域の飽和を踏まえ、これ以上は増やしても得が薄いため既定はここで
/// 頭打ちにする。ユーザは DIRLENS_MAX_WORKERS で上書きできる（`cfg.max_workers`）。
/// ロックフリーな atomic カーソルで分配するため、本数を増やしてもキュー競合は
/// 生じない。16 コア以下のマシンでは available_parallelism がこの値を下回るので
/// 既定のままでも影響しない。
pub(crate) const MAX_WORKERS: usize = 64;

/// 実効的なワーカー上限（DIRLENS_MAX_WORKERS の上書きがあればそれ、無ければ既定）。
pub(crate) fn worker_cap(cfg: &Cfg) -> usize {
    cfg.max_workers.unwrap_or(MAX_WORKERS).max(1)
}

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
        .min(worker_cap(cfg));
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

    // ロックフリーな atomic カーソルで動的に 1 件ずつ分配する（動的分配なので
    // 巨大ファイルが混じっても負荷が偏らない。ロックが無いのでコア数を上げても
    // 分配点がボトルネックにならない）。結果はワーカーごとにローカルへ溜め、
    // 最後に 1 回だけロックしてまとめて登録する（登録ロックも per-item にしない）。
    let targets = &targets;
    let n = targets.len();
    let workers = cores.min(n);
    let cursor = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut local: Vec<(PathBuf, HeavyExtras)> = Vec::new();
                loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let (entry, rel) = &targets[i];
                    let heavy = compute_heavy_extras(sess, entry, rel, cfg);
                    local.push((entry.path.clone(), heavy));
                }
                sess.insert_heavy_many(local);
            });
        }
    });
}

// ─── ディレクトリサイズの並列プリウォーム ─────────────────────────────
//
// フラグ無し（プレーンツリー）でも dirlens は合計サイズのために全サブツリーを
// 底まで stat する。これは syscall（I/O）律速で、従来はルート直下の各
// ディレクトリを並列プリフェッチするだけだった。巨大サブツリーが 1 つあると
// （例: ~/Library/Caches）それを 1 スレッドが延々と走査する負荷不均衡が起きる。
//
// ここではツリー全体をレベル同期の並列 BFS で走査し、各ディレクトリの「直下
// ファイルの合計サイズ・エラー有無・子ディレクトリ一覧」を全スレッドに分散して
// 集める（syscall がここで並列化される）。その後、発見の逆順にボトムアップで
// 合計を畳み込む（メモリ内の算術だけで安価）。結果は Session の sz_cache に入れ、
// 以後の dir_size はキャッシュヒットする。dir_size と完全に同じ加算規則
// （is_file_nofollow のみ加算・is_dir_nofollow のみ再帰・stat 失敗や scandir
// 失敗をエラーとして伝播）なので、直列 dir_size とバイト一致する。

/// 1 ディレクトリを走査し (直下ファイルの合計サイズ, エラー有無, 子ディレクトリ)
/// を返す。dir_size の 1 段分と同一の規則。
fn scan_dir_own<F: FsProvider>(sess: &Session<F>, dir: &Path) -> (u64, bool, Vec<PathBuf>) {
    match sess.fs.scan_dir(dir) {
        Err(()) => (0, true, Vec::new()),
        Ok(entries) => {
            let mut own: u64 = 0;
            let mut err = false;
            let mut children: Vec<PathBuf> = Vec::new();
            for e in entries {
                if e.is_file_nofollow {
                    match sess.fs.stat(&e.path, false) {
                        Some(st) => own += st.size,
                        None => err = true,
                    }
                } else if e.is_dir_nofollow {
                    children.push(e.path);
                }
            }
            (own, err, children)
        }
    }
}

/// フロンティア（同一レベルのディレクトリ群）を並列に走査する。
fn scan_frontier_parallel<F: FsProvider + Sync>(
    sess: &Session<F>,
    frontier: &[PathBuf],
    workers: usize,
) -> Vec<(PathBuf, (u64, bool, Vec<PathBuf>))> {
    let n = frontier.len();
    let cursor = AtomicUsize::new(0);
    let out: std::sync::Mutex<Vec<(PathBuf, (u64, bool, Vec<PathBuf>))>> =
        std::sync::Mutex::new(Vec::with_capacity(n));
    let w = workers.min(n).max(1);
    std::thread::scope(|scope| {
        for _ in 0..w {
            scope.spawn(|| {
                let mut local: Vec<(PathBuf, (u64, bool, Vec<PathBuf>))> = Vec::new();
                loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let dir = &frontier[i];
                    local.push((dir.clone(), scan_dir_own(sess, dir)));
                }
                out.lock().unwrap().extend(local);
            });
        }
    });
    out.into_inner().unwrap()
}

/// ルート配下の全ディレクトリサイズを並列で先に計算し、sz_cache を埋める。
/// 単一コアでは何もしない（レンダリング中の直列 dir_size に任せる）。
pub fn warm_dir_sizes_parallel<F: FsProvider + Sync>(sess: &Session<F>, root: &Path, cfg: &Cfg) {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(worker_cap(cfg));
    if cores < 2 {
        return;
    }

    // Phase 1: レベル同期の並列 BFS。各ディレクトリの (own, err, children) を集める。
    // order は発見順（親が子より前）。逆順に畳めば子が先に確定する。
    let mut info: std::collections::HashMap<PathBuf, (u64, bool, Vec<PathBuf>)> =
        std::collections::HashMap::new();
    let mut order: Vec<PathBuf> = Vec::new();
    let mut frontier: Vec<PathBuf> = vec![root.to_path_buf()];
    while !frontier.is_empty() {
        let results = scan_frontier_parallel(sess, &frontier, cores);
        let mut next: Vec<PathBuf> = Vec::new();
        for (dir, entry) in results {
            for c in &entry.2 {
                next.push(c.clone());
            }
            order.push(dir.clone());
            info.insert(dir, entry);
        }
        frontier = next;
    }

    // Phase 2: 発見の逆順でボトムアップ集計（子は必ず親より後に発見されるため、
    // 逆順では子の合計が先に確定している）。I/O は無くメモリ内の加算のみ。
    let mut totals: std::collections::HashMap<PathBuf, (u64, bool)> =
        std::collections::HashMap::with_capacity(order.len());
    for dir in order.iter().rev() {
        let (own, err, children) = &info[dir];
        let mut total = *own;
        let mut has_err = *err;
        for c in children {
            if let Some((ct, ce)) = totals.get(c) {
                total += *ct;
                has_err |= *ce;
            }
        }
        totals.insert(dir.clone(), (total, has_err));
    }

    sess.bulk_insert_sizes(totals.into_iter().collect());
}
