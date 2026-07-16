//! ツリー表示以外の出力モード（--top / --dupes / --compare）。
//! いずれも通常のフィルタ（-a / -e / -P / -I / -G / --min-size 等）を尊重して
//! ファイルを収集し、フラットなレポートを出力する。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::cfg::Cfg;
use crate::filter::filter_entries;
use crate::fmt::fmt_size;
use crate::gitignore::{extend_pats, relpath_slash};
use crate::i18n::tr;
use crate::provider::FsProvider;
use crate::session::Session;

/// フィルタ適用済みの全ファイル（rel, size）と全ディレクトリ（rel, size）を収集する。
pub fn collect_files<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
    files: &mut Vec<(String, u64)>,
    dirs: &mut Vec<(String, u64)>,
) {
    let cur_pats = extend_pats(sess, active_pats, path, cfg);
    let Some((sub_dirs, sub_files)) = filter_entries(sess, path, cfg, &cur_pats) else {
        return;
    };
    for f in sub_files {
        let sz = sess.fs.stat(&f.path, true).map(|s| s.size).unwrap_or(0);
        files.push((relpath_slash(&f.path, &cfg.root), sz));
    }
    for d in sub_dirs {
        let (sz, _) = sess.dir_size(&d.path);
        dirs.push((relpath_slash(&d.path, &cfg.root), sz));
        collect_files(sess, &d.path, cfg, &cur_pats, files, dirs);
    }
}

/// --top N: 大きいファイル / ディレクトリの上位を表示。
pub fn render_top<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
    n: usize,
) -> String {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    collect_files(sess, &cfg.root, cfg, active_pats, &mut files, &mut dirs);
    files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    dirs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let lang = cfg.lang;
    let mut out = String::new();
    out.push_str(tr(lang, "Largest files:\n", "サイズの大きいファイル:\n"));
    for (rel, sz) in files.iter().take(n) {
        out.push_str(&format!("  {:>10}  {}\n", fmt_size(*sz, false), rel));
    }
    if files.is_empty() {
        out.push_str(tr(lang, "  (no files)\n", "  (ファイルなし)\n"));
    }
    out.push_str(tr(lang, "\nLargest directories:\n", "\nサイズの大きいディレクトリ:\n"));
    for (rel, sz) in dirs.iter().take(n) {
        out.push_str(&format!("  {:>10}  {}/\n", fmt_size(*sz, false), rel));
    }
    if dirs.is_empty() {
        out.push_str(tr(lang, "  (no directories)\n", "  (ディレクトリなし)\n"));
    }
    // ディレクトリサイズは du 相当の生ディスクサイズで -G の影響を受けない。
    // 「大きいものを探す」目的で使うモードなので、gitignore 適用時は
    // target/ 等の除外済みディレクトリが数字を支配しうる旨を明示する
    if cfg.use_gitignore && !cfg.suppress_notes && !dirs.is_empty() {
        out.push_str(tr(
            lang,
            "\nnote: directory sizes are raw disk usage — gitignored contents (node_modules/, target/, ...) still count toward them\n",
            "\n注: ディレクトリサイズはディスク上の生サイズです — gitignore 済みの中身（node_modules/ や target/ 等）も含まれます\n",
        ));
    }
    out
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// ハッシュ対象の上限（これを超えるファイルは --dupes / --compare の内容比較対象外）。
const HASH_LIMIT: u64 = 512 * 1024 * 1024;

/// --dupes: 同一内容のファイル群を検出（サイズ→FNV-1a 64bit の2段）。
pub fn render_dupes<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> String {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    collect_files(sess, &cfg.root, cfg, active_pats, &mut files, &mut dirs);

    // サイズでグループ化（0 バイトは除外・巨大ファイルはスキップ）
    let mut by_size: HashMap<u64, Vec<String>> = HashMap::new();
    for (rel, sz) in files {
        if sz > 0 && sz <= HASH_LIMIT {
            by_size.entry(sz).or_default().push(rel);
        }
    }

    // 同サイズのグループのみハッシュを取る
    let mut groups: Vec<(u64, Vec<String>)> = Vec::new(); // (size, paths)
    for (sz, paths) in by_size {
        if paths.len() < 2 {
            continue;
        }
        let mut by_hash: HashMap<u64, Vec<String>> = HashMap::new();
        for rel in paths {
            let full = cfg.root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(data) = sess.fs.read_prefix(&full, usize::MAX) {
                by_hash.entry(fnv1a64(&data)).or_default().push(rel);
            }
        }
        for (_, mut same) in by_hash {
            if same.len() >= 2 {
                same.sort();
                groups.push((sz, same));
            }
        }
    }
    // 無駄容量（size × (n-1)）の大きい順
    groups.sort_by(|a, b| {
        let wa = a.0 * (a.1.len() as u64 - 1);
        let wb = b.0 * (b.1.len() as u64 - 1);
        wb.cmp(&wa).then_with(|| a.1[0].cmp(&b.1[0]))
    });

    let lang = cfg.lang;
    let mut out = String::new();
    if groups.is_empty() {
        out.push_str(tr(lang, "No duplicate files found.\n", "重複ファイルは見つかりませんでした。\n"));
        return out;
    }
    let total_wasted: u64 = groups.iter().map(|(sz, v)| sz * (v.len() as u64 - 1)).sum();
    match lang {
        crate::i18n::Lang::Ja => out.push_str(&format!(
            "重複ファイル: {} グループ（無駄容量 {}）\n\n",
            groups.len(),
            fmt_size(total_wasted, false)
        )),
        crate::i18n::Lang::En => out.push_str(&format!(
            "Duplicate files: {} groups ({} wasted)\n\n",
            groups.len(),
            fmt_size(total_wasted, false)
        )),
    }
    const SHOW: usize = 20;
    for (sz, paths) in groups.iter().take(SHOW) {
        out.push_str(&format!("  {} × {}:\n", fmt_size(*sz, false), paths.len()));
        for p in paths {
            out.push_str(&format!("    {}\n", p));
        }
    }
    if groups.len() > SHOW {
        out.push_str(&format!(
            "{}\n",
            crate::i18n::more_items(lang, (groups.len() - SHOW) as u64)
        ));
    }
    out
}

/// --compare: 2つのディレクトリツリーの差分（追加 / 削除 / 変更）。
/// gitignore の Tier1（git check-ignore）はルート A 基準のため、このモードでは
/// 呼び出し側が Tier3（内蔵マッチャ）に落としてから呼ぶこと。
pub fn render_compare<F: FsProvider>(
    sess: &Session<F>,
    cfg: &mut Cfg,
    active_pats: &Arc<Vec<String>>,
    other_root: &Path,
) -> String {
    let lang = cfg.lang;
    let root_a = cfg.root.clone();

    let mut files_a = Vec::new();
    let mut dirs_a = Vec::new();
    collect_files(sess, &root_a, cfg, active_pats, &mut files_a, &mut dirs_a);

    cfg.root = other_root.to_path_buf();
    let mut files_b = Vec::new();
    let mut dirs_b = Vec::new();
    collect_files(sess, other_root, cfg, active_pats, &mut files_b, &mut dirs_b);
    cfg.root = root_a.clone();

    let map_a: HashMap<String, u64> = files_a.into_iter().collect();
    let map_b: HashMap<String, u64> = files_b.into_iter().collect();

    let mut only_a: Vec<(&String, u64)> = Vec::new();
    let mut only_b: Vec<(&String, u64)> = Vec::new();
    let mut changed: Vec<(&String, u64, u64)> = Vec::new(); // (rel, size_a, size_b)

    for (rel, sa) in &map_a {
        match map_b.get(rel) {
            None => only_a.push((rel, *sa)),
            Some(sb) if sb != sa => changed.push((rel, *sa, *sb)),
            Some(sb) => {
                // 同サイズ: 小さいファイルは内容ハッシュで変更を検出
                if *sb <= 4 * 1024 * 1024 && *sb > 0 {
                    let pa = root_a.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    let pb = other_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    let ha = sess.fs.read_prefix(&pa, usize::MAX).map(|d| fnv1a64(&d));
                    let hb = sess.fs.read_prefix(&pb, usize::MAX).map(|d| fnv1a64(&d));
                    if ha.is_some() && hb.is_some() && ha != hb {
                        changed.push((rel, *sa, *sb));
                    }
                }
            }
        }
    }
    for (rel, sb) in &map_b {
        if !map_a.contains_key(rel) {
            only_b.push((rel, *sb));
        }
    }
    only_a.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    only_b.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    changed.sort_by(|a, b| a.0.cmp(b.0));

    let total_a: u64 = map_a.values().sum();
    let total_b: u64 = map_b.values().sum();

    let mut out = String::new();
    match lang {
        crate::i18n::Lang::Ja => out.push_str(&format!(
            "比較: {} → {}\n\n",
            root_a.display(),
            other_root.display()
        )),
        crate::i18n::Lang::En => out.push_str(&format!(
            "Compare: {} → {}\n\n",
            root_a.display(),
            other_root.display()
        )),
    }
    const CAP: usize = 50;
    let section = |out: &mut String, title: &str, items: &[(&String, u64)], suffix: &str| {
        if items.is_empty() {
            return;
        }
        out.push_str(&format!("{} ({}):\n", title, items.len()));
        for (rel, sz) in items.iter().take(CAP) {
            out.push_str(&format!("  {:>10}  {}{}\n", fmt_size(*sz, false), rel, suffix));
        }
        if items.len() > CAP {
            out.push_str(&format!(
                "{}\n",
                crate::i18n::more_items(lang, (items.len() - CAP) as u64)
            ));
        }
        out.push('\n');
    };
    section(
        &mut out,
        tr(lang, "Only in the first tree", "1つ目のツリーのみ"),
        &only_a,
        "",
    );
    section(
        &mut out,
        tr(lang, "Only in the second tree", "2つ目のツリーのみ"),
        &only_b,
        "",
    );
    if !changed.is_empty() {
        out.push_str(&format!(
            "{} ({}):\n",
            tr(lang, "Changed", "変更あり"),
            changed.len()
        ));
        for (rel, sa, sb) in changed.iter().take(CAP) {
            out.push_str(&format!(
                "  {}  ({} → {})\n",
                rel,
                fmt_size(*sa, false),
                fmt_size(*sb, false)
            ));
        }
        if changed.len() > CAP {
            out.push_str(&format!(
                "{}\n",
                crate::i18n::more_items(lang, (changed.len() - CAP) as u64)
            ));
        }
        out.push('\n');
    }
    if only_a.is_empty() && only_b.is_empty() && changed.is_empty() {
        out.push_str(tr(lang, "No differences found.\n\n", "差分はありません。\n\n"));
    }
    let delta = total_b as i64 - total_a as i64;
    let delta_str = if delta >= 0 {
        format!("+{}", fmt_size(delta as u64, false))
    } else {
        format!("-{}", fmt_size((-delta) as u64, false))
    };
    match lang {
        crate::i18n::Lang::Ja => out.push_str(&format!(
            "合計サイズ: {} → {} ({})\n",
            fmt_size(total_a, false),
            fmt_size(total_b, false),
            delta_str
        )),
        crate::i18n::Lang::En => out.push_str(&format!(
            "Total size: {} → {} ({})\n",
            fmt_size(total_a, false),
            fmt_size(total_b, false),
            delta_str
        )),
    }
    out
}
