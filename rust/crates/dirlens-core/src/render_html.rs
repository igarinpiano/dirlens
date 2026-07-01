//! HTML レンダラ。dirlens.py の generate_html の等価移植（テンプレートはバイト一致）。

use std::path::Path;
use std::sync::Arc;

use crate::analysis::extras::file_extras;
use crate::cfg::Cfg;
use crate::emoji::get_emoji;
use crate::filter::{filter_entries, has_content, sort_entries};
use crate::fmt::{fmt_count, fmt_size};
use crate::gitignore::{extend_pats, relpath_slash};
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

fn node<F: FsProvider>(
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
                ch.push_str(&node(sess, &entry.path, depth + 1, cfg, &pats));
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

const HTML_PART1: &str = r#"<!DOCTYPE html>
<html lang="ja"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>dirlens — "#;

const HTML_PART2: &str = r#"</title>
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

const HTML_PART3: &str = r#"</h1>
<input id="q" type="text" placeholder="ファイル名で検索…" oninput="search(this.value)">
<div id="tree">"#;

const HTML_PART4: &str = r#"</div>
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

pub fn generate_html<F: FsProvider>(
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
    let tree = node(sess, &cfg.root, 0, cfg, active_pats);
    format!(
        "{}{}{}{}{}{}{}",
        HTML_PART1, root_name, HTML_PART2, root_name, HTML_PART3, tree, HTML_PART4
    )
}
