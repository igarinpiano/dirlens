// HTML レンダラ。rust/crates/dirlens-core/src/render_html.rs の等価移植
// （テンプレートはバイト一致）。

/// Python の html.escape(s, quote=True) 相当。
public func htmlEscape(_ s: String) -> String {
    return s.replacingOccurrences(of: "&", with: "&amp;")
        .replacingOccurrences(of: "<", with: "&lt;")
        .replacingOccurrences(of: ">", with: "&gt;")
        .replacingOccurrences(of: "\"", with: "&quot;")
        .replacingOccurrences(of: "'", with: "&#x27;")
}

private func node(_ sess: Session, _ path: String, _ depth: Int64, _ cfg: Cfg, _ curPats: [String]) -> String {
    let pats = extendPats(sess, curPats, path, cfg)
    let filtered = filterEntries(sess, path, cfg, pats)
    let denied = filtered == nil
    var (dirs, files) = filtered ?? ([], [])
    if cfg.prune {
        dirs = dirs.filter { hasContent(sess, $0.path, depth + 1, cfg, pats) }
    }
    sortEntries(sess, &dirs, &files, cfg)
    let (sz, szErr) = sess.dirSize(path)
    let name = fileName(path) ?? path

    let nd = dirs.count
    let nf = files.count

    let combined: [FSEntry] = cfg.filesFirst ? files + dirs : dirs + files

    var ch = ""
    for entry in combined {
        if entry.isDirNofollow {
            let within = cfg.maxDepth.map { depth < $0 } ?? true
            if within {
                ch += node(sess, entry.path, depth + 1, cfg, pats)
            } else {
                let (eSz, eErr) = sess.dirSize(entry.path)
                ch += "<div class=\"item dir-leaf\">📁 \(htmlEscape(entry.name))/ "
                    + "<span class=\"sz\">\(fmtSize(eSz, eErr))</span></div>\n"
            }
        } else {
            let fSz = sess.fs.stat(entry.path, follow: true)?.size ?? 0
            var sym = ""
            if entry.isSymlink {
                if let t = sess.fs.readLink(entry.path) {
                    sym = " → \(htmlEscape(t))"
                } else {
                    sym = " →"
                }
            }

            let rel = relpathSlash(entry.path, cfg.root)
            let extras = cfg.hasExtras ? fileExtras(sess, entry, rel, cfg) : FileExtras()
            var badges = ""
            if extras.isEntry {
                badges += "<span class=\"badge entry\">entry</span>"
            }
            if extras.noTest {
                badges += "<span class=\"badge notest\">no test</span>"
            }
            if !extras.todos.isEmpty {
                badges += "<span class=\"badge todo\">TODO×\(extras.todos.count)</span>"
            }

            ch += "<div class=\"item file\"><span class=\"emoji\">\(getEmoji(entry.name, isDir: false))</span>"
                + "<span class=\"fname\"> \(htmlEscape(entry.name))\(sym)</span>"
                + "<span class=\"sz\"> \(fmtSize(fSz, false))</span>\(badges)</div>\n"
        }
    }

    let opened = depth == 0 ? " open" : ""
    return "<details\(opened)><summary>📁 <strong>\(htmlEscape(name))/</strong> "
        + "<span class=\"sz\">(\(fmtCount(nd, nf, denied)), \(fmtSize(sz, szErr)))</span></summary>"
        + "<div class=\"ch\">\(ch)</div></details>\n"
}

// 注意: PART1/PART2 は「dirlens — 」の末尾スペースまで含めて Rust 版とバイト一致させる
private let htmlPart1 = """
<!DOCTYPE html>
<html lang="ja"><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>dirlens —
""" + " "

private let htmlPart2 = """
</title>
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
<h1>🌳 dirlens —
""" + " "

private let htmlPart3 = """
</h1>
<input id="q" type="text" placeholder="ファイル名で検索…" oninput="search(this.value)">
<div id="tree">
"""

private let htmlPart4 = """
</div>
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
</script></body></html>
"""

public func generateHtml(_ sess: Session, _ cfg: Cfg, _ activePats: [String]) -> String {
    let rootNameRaw = fileName(cfg.root) ?? cfg.root
    let rootName = htmlEscape(rootNameRaw)
    let tree = node(sess, cfg.root, 0, cfg, activePats)
    return htmlPart1 + rootName + htmlPart2 + rootName + htmlPart3 + tree + htmlPart4
}
