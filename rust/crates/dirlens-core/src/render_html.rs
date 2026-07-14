//! HTML レンダラ。
//!
//! 通常時はリッチテンプレート（検索・全展開/全折りたたみ・サイズバー・
//! ライト/ダークテーマ・大きいファイル一覧）。
//! `DIRLENS_COMPAT=python`（suppress_notes）では旧 Python 版とバイト一致の
//! レガシーテンプレートに切り替える（ゴールデン検証用）。

use std::path::Path;
use std::sync::Arc;

use crate::analysis::extras::file_extras;
use crate::cfg::Cfg;
use crate::emoji::get_emoji;
use crate::filter::{filter_entries, has_content, sort_entries};
use crate::fmt::{fmt_count, fmt_size};
use crate::gitignore::{extend_pats, relpath_slash};
use crate::i18n::{tr, Lang};
use crate::provider::{Entry, FsProvider};
use crate::session::Session;

/// Python の html.escape(s, quote=True) 相当。
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

// ─── レガシーテンプレート（Python 版バイト一致・compat 用） ───

fn node_legacy<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    depth: i64,
    cfg: &Cfg,
    cur_pats: &Arc<Vec<String>>,
) -> String {
    let pats = extend_pats(sess, cur_pats, path, cfg);
    let filtered = filter_entries(sess, path, cfg, &pats);
    let denied = filtered.is_none();
    let (mut dirs, mut files) = filtered.unwrap_or((Vec::new(), Vec::new()));
    if cfg.prune {
        dirs.retain(|d| has_content(sess, &d.path, depth + 1, cfg, &pats));
    }
    sort_entries(sess, &mut dirs, &mut files, cfg);
    let (sz, sz_err) = sess.dir_size(path);
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    let nd = dirs.len();
    let nf = files.len();

    let combined: Vec<Entry> = if cfg.files_first {
        files.into_iter().chain(dirs).collect()
    } else {
        dirs.into_iter().chain(files).collect()
    };

    let mut ch = String::new();
    for entry in combined {
        if entry.is_dir_nofollow {
            let within = cfg.max_depth.map(|md| depth < md).unwrap_or(true);
            if within {
                ch.push_str(&node_legacy(sess, &entry.path, depth + 1, cfg, &pats));
            } else {
                let (e_sz, e_err) = sess.dir_size(&entry.path);
                ch.push_str(&format!(
                    "<div class=\"item dir-leaf\">📁 {}/ <span class=\"sz\">{}</span></div>\n",
                    html_escape(&entry.name),
                    fmt_size(e_sz, e_err)
                ));
            }
        } else {
            let f_sz = sess.fs.stat(&entry.path, true).map(|s| s.size).unwrap_or(0);
            let sym = if entry.is_symlink {
                match sess.fs.read_link(&entry.path) {
                    Some(t) => format!(" → {}", html_escape(&t)),
                    None => " →".to_string(),
                }
            } else {
                String::new()
            };

            let rel = relpath_slash(&entry.path, &cfg.root);
            let extras = if cfg.has_extras {
                file_extras(sess, &entry, &rel, cfg)
            } else {
                Default::default()
            };
            let mut badges = String::new();
            if extras.is_entry {
                badges.push_str("<span class=\"badge entry\">entry</span>");
            }
            if extras.no_test {
                badges.push_str("<span class=\"badge notest\">no test</span>");
            }
            if !extras.todos.is_empty() {
                badges.push_str(&format!(
                    "<span class=\"badge todo\">TODO×{}</span>",
                    extras.todos.len()
                ));
            }

            ch.push_str(&format!(
                "<div class=\"item file\"><span class=\"emoji\">{}</span><span class=\"fname\"> {}{}</span><span class=\"sz\"> {}</span>{}</div>\n",
                get_emoji(&entry.name, false),
                html_escape(&entry.name),
                sym,
                fmt_size(f_sz, false),
                badges
            ));
        }
    }

    let opened = if depth == 0 { " open" } else { "" };
    format!(
        "<details{}><summary>📁 <strong>{}/</strong> <span class=\"sz\">({}, {})</span></summary><div class=\"ch\">{}</div></details>\n",
        opened,
        html_escape(&name),
        fmt_count(nd, nf, denied),
        fmt_size(sz, sz_err),
        ch
    )
}

const LEGACY_PART1: &str = r#"<!DOCTYPE html>
<html lang="ja"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>dirlens — "#;

const LEGACY_PART2: &str = r#"</title>
<style>
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:Menlo,Consolas,monospace;font-size:14px;background:#1e1e2e;color:#cdd6f4;padding:24px}
h1{color:#89b4fa;margin-bottom:12px;font-size:18px}
#q{background:#313244;border:1px solid #45475a;color:#cdd6f4;padding:6px 12px;
    border-radius:6px;font-size:13px;margin-bottom:16px;width:280px;outline:none}
#q:focus{border-color:#89b4fa}
details{margin-left:18px}
summary{cursor:pointer;padding:2px 6px;border-radius:4px;list-style:none;
         white-space:nowrap;color:#89dceb}
summary::-webkit-details-marker{display:none}
summary::before{content:"▶ ";font-size:10px;opacity:.4}
details[open]>summary::before{content:"▼ "}
summary:hover{background:rgba(255,255,255,.06)}
.ch{border-left:1px solid rgba(255,255,255,.08);margin-left:10px}
.item{padding:2px 6px;white-space:nowrap;margin-left:18px}
.item:hover{background:rgba(255,255,255,.05);border-radius:4px}
.fname{color:#a6e3a1}.sz{color:#585b70;font-size:12px}
.emoji{width:1.6em;display:inline-block}.hidden{display:none!important}
.badge{display:inline-block;margin-left:6px;padding:0 6px;border-radius:8px;
        font-size:10px;vertical-align:middle}
.badge.entry{background:#89b4fa;color:#1e1e2e}
.badge.notest{background:#f9e2af;color:#1e1e2e}
.badge.todo{background:#f38ba8;color:#1e1e2e}
</style></head><body>
<h1>🌳 dirlens — "#;

const LEGACY_PART3: &str = r#"</h1>
<input id="q" type="text" placeholder="ファイル名で検索…" oninput="search(this.value)">
<div id="tree">"#;

const LEGACY_PART4: &str = r#"</div>
<script>
function search(q){
  q=q.toLowerCase().trim();
  document.querySelectorAll('.hidden').forEach(el=>el.classList.remove('hidden'));
  if(!q) return;
  document.querySelectorAll('details').forEach(d=>d.open=true);
  document.querySelectorAll('.file').forEach(el=>{
    const n=el.querySelector('.fname')?.textContent.toLowerCase()||'';
    if(!n.includes(q)) el.classList.add('hidden');
  });
  document.querySelectorAll('#tree details').forEach(detail=>{
    const hasVisible=[...detail.querySelectorAll('.file')]
      .some(f=>!f.classList.contains('hidden'));
    if(!hasVisible) detail.classList.add('hidden');
  });
}
</script></body></html>"#;

fn generate_legacy<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> String {
    let root_name_raw = cfg
        .root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| cfg.root.to_string_lossy().into_owned());
    let root_name = html_escape(&root_name_raw);
    let tree = node_legacy(sess, &cfg.root, 0, cfg, active_pats);
    format!(
        "{}{}{}{}{}{}{}",
        LEGACY_PART1, root_name, LEGACY_PART2, root_name, LEGACY_PART3, tree, LEGACY_PART4
    )
}

// ─── リッチテンプレート（通常時） ─────────────────────────────

struct HtmlStats {
    files: u64,
    dirs: u64,
    largest: Vec<(String, u64)>, // (rel, size)
    root_size: u64,
}

#[allow(clippy::too_many_arguments)]
fn node_rich<F: FsProvider>(
    sess: &Session<F>,
    path: &Path,
    depth: i64,
    cfg: &Cfg,
    cur_pats: &Arc<Vec<String>>,
    stats: &mut HtmlStats,
) -> String {
    let pats = extend_pats(sess, cur_pats, path, cfg);
    let filtered = filter_entries(sess, path, cfg, &pats);
    let denied = filtered.is_none();
    let (mut dirs, mut files) = filtered.unwrap_or((Vec::new(), Vec::new()));
    if cfg.prune {
        dirs.retain(|d| has_content(sess, &d.path, depth + 1, cfg, &pats));
    }
    sort_entries(sess, &mut dirs, &mut files, cfg);
    let (sz, sz_err) = sess.dir_size(path);
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    let nd = dirs.len();
    let nf = files.len();

    let combined: Vec<Entry> = if cfg.files_first {
        files.into_iter().chain(dirs).collect()
    } else {
        dirs.into_iter().chain(files).collect()
    };

    // サイズバーの幅: root に対する割合の平方根（小さいファイルも見えるように）
    let root_size = stats.root_size;
    let bar_pct = move |s: u64| -> f64 {
        if root_size == 0 {
            0.0
        } else {
            ((s as f64 / root_size as f64).sqrt() * 100.0).min(100.0)
        }
    };

    let mut ch = String::new();
    for entry in combined {
        if entry.is_dir_nofollow {
            let within = cfg.max_depth.map(|md| depth < md).unwrap_or(true);
            if within {
                stats.dirs += 1;
                ch.push_str(&node_rich(sess, &entry.path, depth + 1, cfg, &pats, stats));
            } else {
                stats.dirs += 1;
                let (e_sz, e_err) = sess.dir_size(&entry.path);
                ch.push_str(&format!(
                    "<div class=\"item dir-leaf\">📁 {}/ <span class=\"sz\">{}</span></div>\n",
                    html_escape(&entry.name),
                    fmt_size(e_sz, e_err)
                ));
            }
        } else {
            stats.files += 1;
            let f_sz = sess.fs.stat(&entry.path, true).map(|s| s.size).unwrap_or(0);
            let sym = if entry.is_symlink {
                match sess.fs.read_link(&entry.path) {
                    Some(t) => format!(" → {}", html_escape(&t)),
                    None => " →".to_string(),
                }
            } else {
                String::new()
            };

            let rel = relpath_slash(&entry.path, &cfg.root);
            stats.largest.push((rel.clone(), f_sz));
            if stats.largest.len() > 256 {
                stats.largest.sort_by(|a, b| b.1.cmp(&a.1));
                stats.largest.truncate(16);
            }
            let extras = if cfg.has_extras {
                file_extras(sess, &entry, &rel, cfg)
            } else {
                Default::default()
            };
            let mut badges = String::new();
            if extras.is_entry {
                badges.push_str("<span class=\"badge entry\">entry</span>");
            }
            if extras.is_config {
                badges.push_str("<span class=\"badge config\">config</span>");
            }
            if extras.no_test {
                badges.push_str("<span class=\"badge notest\">no test</span>");
            }
            if !extras.todos.is_empty() {
                badges.push_str(&format!(
                    "<span class=\"badge todo\">TODO×{}</span>",
                    extras.todos.len()
                ));
            }
            if let Some(tok) = extras.tokens {
                badges.push_str(&format!(
                    "<span class=\"badge tok\">{}</span>",
                    html_escape(&crate::fmt::fmt_tokens(tok))
                ));
            }

            ch.push_str(&format!(
                "<div class=\"item file\"><span class=\"emoji\">{}</span><span class=\"fname\"> {}{}</span><span class=\"sz\"> {}</span><span class=\"bar\"><span style=\"width:{:.2}%\"></span></span>{}</div>\n",
                get_emoji(&entry.name, false),
                html_escape(&entry.name),
                sym,
                fmt_size(f_sz, false),
                bar_pct(f_sz),
                badges
            ));
        }
    }

    let opened = if depth == 0 { " open" } else { "" };
    format!(
        "<details{}><summary>📁 <strong>{}/</strong> <span class=\"sz\">({}, {})</span><span class=\"bar\"><span style=\"width:{:.2}%\"></span></span></summary><div class=\"ch\">{}</div></details>\n",
        opened,
        html_escape(&name),
        fmt_count(nd, nf, denied),
        fmt_size(sz, sz_err),
        bar_pct(sz),
        ch
    )
}

const RICH_STYLE: &str = r#"
:root{--bg:#1e1e2e;--fg:#cdd6f4;--accent:#89b4fa;--dir:#89dceb;--file:#a6e3a1;
      --muted:#585b70;--panel:#313244;--border:#45475a;--hover:rgba(255,255,255,.06);
      --barbg:rgba(255,255,255,.07);--barfg:#89b4fa}
:root[data-theme="light"]{--bg:#f6f6f8;--fg:#2b2d3a;--accent:#1e66f5;--dir:#04a5e5;
      --file:#40a02b;--muted:#8c8fa1;--panel:#e6e9ef;--border:#ccd0da;
      --hover:rgba(0,0,0,.05);--barbg:rgba(0,0,0,.07);--barfg:#1e66f5}
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:Menlo,Consolas,monospace;font-size:14px;background:var(--bg);color:var(--fg);padding:24px}
h1{color:var(--accent);margin-bottom:4px;font-size:18px}
.stats{color:var(--muted);font-size:12px;margin-bottom:12px}
.controls{display:flex;gap:8px;align-items:center;margin-bottom:16px;flex-wrap:wrap}
#q{background:var(--panel);border:1px solid var(--border);color:var(--fg);padding:6px 12px;
    border-radius:6px;font-size:13px;width:280px;outline:none}
#q:focus{border-color:var(--accent)}
button{background:var(--panel);border:1px solid var(--border);color:var(--fg);
       padding:6px 10px;border-radius:6px;font-size:12px;cursor:pointer;font-family:inherit}
button:hover{border-color:var(--accent)}
details{margin-left:18px}
summary{cursor:pointer;padding:2px 6px;border-radius:4px;list-style:none;
         white-space:nowrap;color:var(--dir)}
summary::-webkit-details-marker{display:none}
summary::before{content:"▶ ";font-size:10px;opacity:.4}
details[open]>summary::before{content:"▼ "}
summary:hover{background:var(--hover)}
.ch{border-left:1px solid var(--border);margin-left:10px}
.item{padding:2px 6px;white-space:nowrap;margin-left:18px}
.item:hover{background:var(--hover);border-radius:4px}
.fname{color:var(--file)}.sz{color:var(--muted);font-size:12px}
.emoji{width:1.6em;display:inline-block}.hidden{display:none!important}
.bar{display:inline-block;width:80px;height:6px;background:var(--barbg);
     border-radius:3px;margin-left:8px;vertical-align:middle;overflow:hidden}
.bar>span{display:block;height:100%;background:var(--barfg);border-radius:3px}
.badge{display:inline-block;margin-left:6px;padding:0 6px;border-radius:8px;
        font-size:10px;vertical-align:middle;background:var(--panel);color:var(--fg)}
.badge.entry{background:#89b4fa;color:#1e1e2e}
.badge.config{background:#94e2d5;color:#1e1e2e}
.badge.notest{background:#f9e2af;color:#1e1e2e}
.badge.todo{background:#f38ba8;color:#1e1e2e}
.badge.tok{background:var(--panel);color:var(--muted)}
.panel{background:var(--panel);border:1px solid var(--border);border-radius:8px;
       padding:12px 16px;margin:16px 0;max-width:720px}
.panel h2{font-size:13px;color:var(--accent);margin-bottom:8px}
.panel .row{display:flex;justify-content:space-between;font-size:12px;padding:1px 0}
.panel .row .p{color:var(--fg)}.panel .row .s{color:var(--muted)}
"#;

const RICH_SCRIPT: &str = r#"
function search(q){
  q=q.toLowerCase().trim();
  document.querySelectorAll('.hidden').forEach(el=>el.classList.remove('hidden'));
  if(!q) return;
  document.querySelectorAll('#tree details').forEach(d=>d.open=true);
  document.querySelectorAll('.file').forEach(el=>{
    const n=el.querySelector('.fname')?.textContent.toLowerCase()||'';
    if(!n.includes(q)) el.classList.add('hidden');
  });
  document.querySelectorAll('#tree details').forEach(detail=>{
    const hasVisible=[...detail.querySelectorAll('.file')]
      .some(f=>!f.classList.contains('hidden'));
    if(!hasVisible) detail.classList.add('hidden');
  });
}
function setAll(open){document.querySelectorAll('#tree details').forEach(d=>d.open=open)}
function toggleTheme(){
  const r=document.documentElement;
  const next=r.dataset.theme==='light'?'dark':'light';
  r.dataset.theme=next;
  try{localStorage.setItem('dirlens-theme',next)}catch(e){}
}
(function(){
  let t=null;
  try{t=localStorage.getItem('dirlens-theme')}catch(e){}
  if(!t&&window.matchMedia&&window.matchMedia('(prefers-color-scheme: light)').matches)t='light';
  if(t)document.documentElement.dataset.theme=t;
})();
"#;

fn generate_rich<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> String {
    let lang = cfg.lang;
    let root_name_raw = cfg
        .root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| cfg.root.to_string_lossy().into_owned());
    let root_name = html_escape(&root_name_raw);
    let (root_size, _) = sess.dir_size(&cfg.root);
    let mut stats = HtmlStats {
        files: 0,
        dirs: 0,
        largest: Vec::new(),
        root_size,
    };
    let tree = node_rich(sess, &cfg.root, 0, cfg, active_pats, &mut stats);

    stats.largest.sort_by(|a, b| b.1.cmp(&a.1));
    stats.largest.truncate(10);
    let mut largest_html = String::new();
    for (rel, sz) in &stats.largest {
        largest_html.push_str(&format!(
            "<div class=\"row\"><span class=\"p\">{}</span><span class=\"s\">{}</span></div>\n",
            html_escape(rel),
            fmt_size(*sz, false)
        ));
    }

    let html_lang = tr(lang, "en", "ja");
    let search_ph = tr(lang, "search file names…", "ファイル名で検索…");
    let expand = tr(lang, "expand all", "全て展開");
    let collapse = tr(lang, "collapse all", "全て折りたたむ");
    let theme = tr(lang, "theme", "テーマ");
    let largest_title = tr(lang, "Largest files", "サイズの大きいファイル");
    let stats_line = match lang {
        Lang::Ja => format!(
            "{} ディレクトリ / {} ファイル / {}",
            stats.dirs,
            stats.files,
            fmt_size(root_size, false)
        ),
        Lang::En => format!(
            "{} directories / {} files / {}",
            stats.dirs,
            stats.files,
            fmt_size(root_size, false)
        ),
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="{html_lang}"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>dirlens — {root_name}</title>
<style>{RICH_STYLE}</style></head><body>
<h1>🌳 dirlens — {root_name}</h1>
<div class="stats">{stats_line}</div>
<div class="controls">
<input id="q" type="text" placeholder="{search_ph}" oninput="search(this.value)">
<button onclick="setAll(true)">{expand}</button>
<button onclick="setAll(false)">{collapse}</button>
<button onclick="toggleTheme()">☀/🌙 {theme}</button>
</div>
<div id="tree">{tree}</div>
<div class="panel"><h2>{largest_title}</h2>
{largest_html}</div>
<script>{RICH_SCRIPT}</script></body></html>"#
    )
}

pub fn generate_html<F: FsProvider>(
    sess: &Session<F>,
    cfg: &Cfg,
    active_pats: &Arc<Vec<String>>,
) -> String {
    if cfg.suppress_notes {
        generate_legacy(sess, cfg, active_pats)
    } else {
        generate_rich(sess, cfg, active_pats)
    }
}
