//! インタラクティブ TUI（-i / --interactive）。
//!
//! 左: ツリー（遅延読み込み・開閉）、右: 選択エントリの詳細
//! （サイズ・更新日時・トークン数・アウトライン・TODO・最終コミット）。
//! キー: ↑↓/jk 移動, →/l/Enter 展開, ←/h 折りたたみ/親へ, g/G 先頭/末尾,
//!       s サイズ順切替, a 隠しファイル切替, / フィルタ, q/Esc 終了

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};

use dirlens_core::analysis::extras::{file_extras, FileExtras};
use dirlens_core::analysis::gitlog::load_git_log;
use dirlens_core::filter::{count_entries, filter_entries, sort_entries};
use dirlens_core::fmt::{fmt_count, fmt_date, fmt_size, fmt_tokens};
use dirlens_core::gitignore::{extend_pats, relpath_slash};
use dirlens_core::provider::{Entry, FsProvider};
use dirlens_core::{prepare, Args, Cfg, Lang, Session};

use crate::providers::{StdFs, StdGit};

struct Node {
    entry: Entry,
    depth: usize,
    expanded: bool,
    children: Option<Vec<usize>>, // None = 未読込
    parent: Option<usize>,
}

struct App<'a> {
    sess: Session<'a, StdFs>,
    cfg: Cfg,
    pats: Arc<Vec<String>>,
    nodes: Vec<Node>,
    roots: Vec<usize>,
    visible: Vec<usize>,
    selected: usize,
    filter: String,
    filter_mode: bool,
    extras_cache: HashMap<PathBuf, FileExtras>,
    git_loaded: bool,
    /// 詳細ペインの縦スクロール位置（選択が動くと 0 に戻る）
    detail_scroll: u16,
    /// 直近の描画での詳細ペインの (総行数, 表示可能行数)。スクロールの上限計算用
    detail_extent: (usize, usize),
}

impl<'a> App<'a> {
    fn load_children(&mut self, idx: Option<usize>) -> Vec<usize> {
        let (path, depth) = match idx {
            Some(i) => (self.nodes[i].entry.path.clone(), self.nodes[i].depth + 1),
            None => (self.cfg.root.clone(), 0),
        };
        let pats = extend_pats(&self.sess, &self.pats, &path, &self.cfg);
        let Some((mut dirs, mut files)) = filter_entries(&self.sess, &path, &self.cfg, &pats)
        else {
            return Vec::new();
        };
        sort_entries(&self.sess, &mut dirs, &mut files, &self.cfg);
        let combined: Vec<Entry> = dirs.into_iter().chain(files).collect();
        let mut out = Vec::new();
        for e in combined {
            self.nodes.push(Node {
                entry: e,
                depth,
                expanded: false,
                children: None,
                parent: idx,
            });
            out.push(self.nodes.len() - 1);
        }
        out
    }

    fn rebuild_visible(&mut self) {
        fn walk(app: &App, idx: usize, out: &mut Vec<usize>) {
            let filter = app.filter.to_lowercase();
            let show = filter.is_empty()
                || app.nodes[idx].entry.name.to_lowercase().contains(&filter)
                || app.nodes[idx].entry.is_dir_nofollow;
            if show {
                out.push(idx);
            }
            if app.nodes[idx].expanded {
                if let Some(children) = &app.nodes[idx].children {
                    for &c in children {
                        walk(app, c, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        for &r in &self.roots.clone() {
            walk(self, r, &mut out);
        }
        self.visible = out;
        if self.selected >= self.visible.len() {
            self.selected = self.visible.len().saturating_sub(1);
        }
        self.detail_scroll = 0;
    }

    fn toggle_expand(&mut self, open: Option<bool>) {
        let Some(&idx) = self.visible.get(self.selected) else { return };
        if !self.nodes[idx].entry.is_dir_nofollow {
            return;
        }
        let target = open.unwrap_or(!self.nodes[idx].expanded);
        if target && self.nodes[idx].children.is_none() {
            let ch = self.load_children(Some(idx));
            self.nodes[idx].children = Some(ch);
        }
        self.nodes[idx].expanded = target;
        self.rebuild_visible();
    }

    fn collapse_or_parent(&mut self) {
        let Some(&idx) = self.visible.get(self.selected) else { return };
        if self.nodes[idx].entry.is_dir_nofollow && self.nodes[idx].expanded {
            self.nodes[idx].expanded = false;
        } else if let Some(p) = self.nodes[idx].parent {
            if let Some(pos) = self.visible.iter().position(|&v| v == p) {
                self.selected = pos;
            }
        }
        self.rebuild_visible();
    }

    fn ensure_git(&mut self) {
        if !self.git_loaded {
            self.git_loaded = true;
            let (map, counts) = load_git_log(&StdGit, &self.cfg.root, !self.cfg.suppress_notes);
            self.cfg.git_map = map;
            self.cfg.git_change_counts = counts;
        }
    }

    fn extras_for(&mut self, idx: usize) -> FileExtras {
        let path = self.nodes[idx].entry.path.clone();
        if let Some(e) = self.extras_cache.get(&path) {
            return clone_extras(e);
        }
        self.ensure_git();
        let rel = relpath_slash(&path, &self.cfg.root);
        let entry = clone_entry(&self.nodes[idx].entry);
        let ex = file_extras(&self.sess, &entry, &rel, &self.cfg);
        let ret = clone_extras(&ex);
        self.extras_cache.insert(path, ex);
        ret
    }
}

fn clone_entry(e: &Entry) -> Entry {
    Entry {
        name: e.name.clone(),
        path: e.path.clone(),
        is_dir_nofollow: e.is_dir_nofollow,
        is_file_nofollow: e.is_file_nofollow,
        is_symlink: e.is_symlink,
        is_dir_follow: e.is_dir_follow,
    }
}

fn clone_extras(e: &FileExtras) -> FileExtras {
    FileExtras {
        tokens: e.tokens,
        tokens_estimated: e.tokens_estimated,
        lines: e.lines,
        git: e.git.clone(),
        todos: e.todos.clone(),
        is_entry: e.is_entry,
        is_config: e.is_config,
        no_test: e.no_test,
        outline: e.outline.clone(),
        outline_method: e.outline_method,
        imports: e.imports.clone(),
        imported_by: e.imported_by.clone(),
        external_imports: e.external_imports.clone(),
    }
}

pub fn run_tui(mut args: Args) -> Result<(), String> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return Err("interactive mode requires a terminal".to_string());
    }
    // TUI では詳細ペイン用の解析を常に有効化する
    args.tokens = true;
    args.todo = true;
    args.outline = true;
    args.git = true;
    let fs = StdFs;
    let mut cfg = prepare(&args, &fs, true).map_err(|r| r.stderr.trim().to_string())?;
    cfg.show_git = true;
    let mut sess = Session::new(&fs);
    // gitignore Tier1（-G 時）
    let pats: Arc<Vec<String>> = if cfg.use_gitignore {
        let p = sess.load_gitignore(&cfg.root.clone());
        if let Some(set) =
            dirlens_core::gitignore::build_git_ignored_set(&sess, &StdGit, &cfg.root.clone())
        {
            sess.git_ignored = Some(set);
        }
        p
    } else {
        Arc::new(Vec::new())
    };

    let lang = cfg.lang;
    let mut app = App {
        sess,
        cfg,
        pats,
        nodes: Vec::new(),
        roots: Vec::new(),
        visible: Vec::new(),
        selected: 0,
        filter: String::new(),
        filter_mode: false,
        extras_cache: HashMap::new(),
        git_loaded: false,
        detail_scroll: 0,
        detail_extent: (0, 0),
    };
    app.roots = app.load_children(None);
    app.rebuild_visible();

    let mut terminal = ratatui::init();
    let res = event_loop(&mut terminal, &mut app, lang);
    ratatui::restore();
    res
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    lang: Lang,
) -> Result<(), String> {
    let mut list_state = ListState::default();
    // マーキー（ステータスバーが幅に収まらないときだけ流れる）用の時刻カウンタ
    let mut ticker: usize = 0;
    loop {
        list_state.select(Some(app.selected));
        terminal
            .draw(|f| draw(f, app, &mut list_state, lang, ticker))
            .map_err(|e| e.to_string())?;

        // 200ms でタイムアウトしてマーキーを1桁ぶん進める（入力があれば即応答）
        match event::poll(std::time::Duration::from_millis(200)) {
            Ok(true) => {}
            Ok(false) => {
                ticker = ticker.wrapping_add(1);
                continue;
            }
            Err(_) => break,
        }
        let Ok(ev) = event::read() else { break };
        let Event::Key(key) = ev else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if app.filter_mode {
            match key.code {
                KeyCode::Esc => {
                    app.filter.clear();
                    app.filter_mode = false;
                    app.rebuild_visible();
                }
                KeyCode::Enter => app.filter_mode = false,
                KeyCode::Backspace => {
                    app.filter.pop();
                    app.rebuild_visible();
                }
                KeyCode::Char(c) => {
                    app.filter.push(c);
                    app.rebuild_visible();
                }
                _ => {}
            }
            continue;
        }
        // 詳細ペインのスクロール上限（直近の描画時の行数から計算）
        let detail_max = {
            let (total, height) = app.detail_extent;
            total.saturating_sub(height) as u16
        };
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => break,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
            KeyCode::Up | KeyCode::Char('k') => {
                app.selected = app.selected.saturating_sub(1);
                app.detail_scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.selected + 1 < app.visible.len() {
                    app.selected += 1;
                }
                app.detail_scroll = 0;
            }
            // 詳細ペインのスクロール: J/K（1行）・PageUp/PageDown = Mac の fn+↑↓（10行）
            KeyCode::Char('J') => {
                app.detail_scroll = (app.detail_scroll + 1).min(detail_max);
            }
            KeyCode::Char('K') => {
                app.detail_scroll = app.detail_scroll.saturating_sub(1);
            }
            KeyCode::PageDown => {
                app.detail_scroll = (app.detail_scroll + 10).min(detail_max);
            }
            KeyCode::PageUp => {
                app.detail_scroll = app.detail_scroll.saturating_sub(10);
            }
            // ツリーのページ移動は Ctrl+D / Ctrl+U（vim 慣習）
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.selected = (app.selected + 20).min(app.visible.len().saturating_sub(1));
                app.detail_scroll = 0;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.selected = app.selected.saturating_sub(20);
                app.detail_scroll = 0;
            }
            KeyCode::Char('g') => {
                app.selected = 0;
                app.detail_scroll = 0;
            }
            KeyCode::Char('G') => {
                app.selected = app.visible.len().saturating_sub(1);
                app.detail_scroll = 0;
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => app.toggle_expand(Some(true)),
            KeyCode::Left | KeyCode::Char('h') => {
                app.collapse_or_parent();
                app.detail_scroll = 0;
            }
            KeyCode::Char(' ') => app.toggle_expand(None),
            KeyCode::Char('s') => {
                app.cfg.by_size = !app.cfg.by_size;
                // 並びが変わるため全ノードを読み直す
                app.nodes.clear();
                app.extras_cache.clear();
                app.roots = app.load_children(None);
                app.rebuild_visible();
                app.selected = 0;
            }
            KeyCode::Char('a') => {
                app.cfg.show_all = !app.cfg.show_all;
                app.nodes.clear();
                app.roots = app.load_children(None);
                app.rebuild_visible();
                app.selected = 0;
            }
            KeyCode::Char('/') => app.filter_mode = true,
            _ => {}
        }
    }
    Ok(())
}

/// マーキー表示: text + 区切りを循環バッファとみなし、start_col 桁目から
/// width 桁ぶんを表示幅（東アジア全角=2桁）ベースで切り出す。
fn marquee_slice(text: &str, start_col: usize, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    const GAP: &str = "   •   ";
    if text.is_empty() {
        return String::new();
    }
    let cycle: Vec<(char, usize)> = text
        .chars()
        .chain(GAP.chars())
        .map(|c| (c, c.width().unwrap_or(0)))
        .collect();
    let total: usize = cycle.iter().map(|(_, w)| w).sum();
    if total == 0 || width == 0 {
        return String::new();
    }
    let start = start_col % total;
    // start 桁目に達する位置（文字境界に丸める）を探す
    let mut col = 0;
    let mut i = 0;
    while col < start {
        col += cycle[i].1;
        i = (i + 1) % cycle.len();
    }
    // width 桁ぶん詰める（全角が境界をまたぐ場合はそこで打ち切り）
    let mut out = String::new();
    let mut used = 0;
    while used < width {
        let (c, w) = cycle[i];
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
        i = (i + 1) % cycle.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::marquee_slice;

    #[test]
    fn marquee_ascii() {
        // "abc" + GAP("   •   ") = 3 + 7 = 10 桁の循環
        assert_eq!(marquee_slice("abc", 0, 3), "abc");
        assert_eq!(marquee_slice("abc", 1, 3), "bc ");
        assert_eq!(marquee_slice("abc", 10, 3), "abc"); // 1周して先頭に戻る
    }

    #[test]
    fn marquee_cjk_width() {
        use unicode_width::UnicodeWidthStr;
        // 全角（幅2）が境界をまたぐときは詰め込まない（幅超過しない）
        for start in 0..20 {
            let s = marquee_slice("移動 · 展開", start, 8);
            assert!(s.as_str().width() <= 8, "width overflow at start={}", start);
        }
    }

    #[test]
    fn marquee_zero() {
        assert_eq!(marquee_slice("", 5, 10), "");
        assert_eq!(marquee_slice("abc", 5, 0), "");
    }
}

fn draw(
    f: &mut ratatui::Frame,
    app: &mut App,
    list_state: &mut ListState,
    lang: Lang,
    ticker: usize,
) {
    let tr = |en: &'static str, ja: &'static str| dirlens_core::i18n::tr(lang, en, ja);
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(f.area());
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(outer[0]);

    // ── ツリーペイン ──────────────────────────────────────────
    let items: Vec<ListItem> = app
        .visible
        .iter()
        .map(|&idx| {
            let n = &app.nodes[idx];
            let indent = "  ".repeat(n.depth);
            let (icon, style) = if n.entry.is_dir_nofollow {
                (
                    if n.expanded { "▼ " } else { "▶ " },
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )
            } else {
                ("  ", Style::default().fg(Color::Green))
            };
            let size = if n.entry.is_dir_nofollow {
                let (sz, err) = app.sess.dir_size(&n.entry.path);
                fmt_size(sz, err)
            } else {
                let sz = app.sess.fs.stat(&n.entry.path, true).map(|s| s.size).unwrap_or(0);
                fmt_size(sz, false)
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{}{}", indent, icon)),
                Span::styled(n.entry.name.clone(), style),
                Span::styled(format!("  {}", size), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    let title = format!(" 🌳 {} ", app.cfg.root_label);
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, panes[0], list_state);

    // ── 詳細ペイン ────────────────────────────────────────────
    let mut lines: Vec<Line> = Vec::new();
    if let Some(&idx) = app.visible.get(app.selected).copied().as_ref() {
        let is_dir = app.nodes[idx].entry.is_dir_nofollow;
        let name = app.nodes[idx].entry.name.clone();
        let path = app.nodes[idx].entry.path.clone();
        lines.push(Line::from(Span::styled(
            name,
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        if is_dir {
            let (sz, err) = app.sess.dir_size(&path);
            lines.push(Line::from(format!(
                "{}: {}",
                tr("size", "サイズ"),
                fmt_size(sz, err)
            )));
            // 直下の内容数（ツリー表示の "(2 dirs, 3 files)" と同じ書式）
            let (nd, nf, denied) = count_entries(&app.sess, &path, &app.cfg, &app.pats);
            lines.push(Line::from(format!(
                "{}: {}",
                tr("contents", "内容"),
                fmt_count(nd, nf, denied)
            )));
            if let Some(st) = app.sess.fs.stat(&path, true) {
                lines.push(Line::from(format!(
                    "{}: {}",
                    tr("modified", "更新"),
                    fmt_date(app.sess.fs.now(), st.mtime, lang)
                )));
            }
            // 直下の子のプレビュー（ツリー表示風・サイズつき）
            let pats = extend_pats(&app.sess, &app.pats, &path, &app.cfg);
            if let Some((mut dirs, mut files)) =
                filter_entries(&app.sess, &path, &app.cfg, &pats)
            {
                sort_entries(&app.sess, &mut dirs, &mut files, &app.cfg);
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    tr("Contents:", "直下の内容:"),
                    Style::default().fg(Color::Cyan),
                )));
                // あふれた分はスクロールで見られるため、プレビューは十分大きく取る
                const PREVIEW: usize = 200;
                let mut shown = 0;
                for d in dirs.iter().take(PREVIEW) {
                    let (dsz, derr) = app.sess.dir_size(&d.path);
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!(" {}/", d.name),
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::styled(
                            format!("  {}", fmt_size(dsz, derr)),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                    shown += 1;
                }
                for fl in files.iter().take(PREVIEW.saturating_sub(shown)) {
                    let fsz = app
                        .sess
                        .fs
                        .stat(&fl.path, true)
                        .map(|s| s.size)
                        .unwrap_or(0);
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!(" {}", fl.name),
                            Style::default().fg(Color::Green),
                        ),
                        Span::styled(
                            format!("  {}", fmt_size(fsz, false)),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                    shown += 1;
                }
                let total = dirs.len() + files.len();
                if total > shown {
                    lines.push(Line::from(Span::styled(
                        format!("  … +{}", total - shown),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        } else {
            let st = app.sess.fs.stat(&path, true);
            if let Some(st) = st {
                lines.push(Line::from(format!(
                    "{}: {}",
                    tr("size", "サイズ"),
                    fmt_size(st.size, false)
                )));
                lines.push(Line::from(format!(
                    "{}: {}",
                    tr("modified", "更新"),
                    fmt_date(app.sess.fs.now(), st.mtime, lang)
                )));
            }
            let ex = app.extras_for(idx);
            if let (Some(tok), Some(l)) = (ex.tokens, ex.lines) {
                lines.push(Line::from(format!(
                    "{}: {} / {} lines",
                    tr("tokens", "トークン"),
                    fmt_tokens(tok),
                    l
                )));
            }
            if let Some(g) = &ex.git {
                lines.push(Line::from(format!("git: \"{}\" ({})", g.subject, g.date)));
            }
            // アウトライン・TODO は打ち切らず全件出す（あふれた分はスクロールで見る）。
            // 極端なファイルでの描画コストだけ CAP で抑える。
            const CAP: usize = 500;
            if let Some(items) = &ex.outline {
                if !items.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!("{} ({})", tr("Outline", "アウトライン"), items.len()),
                        Style::default().fg(Color::Cyan),
                    )));
                    for it in items.iter().take(CAP) {
                        let vis = if it.public { "+" } else { "-" };
                        let span = it
                            .span
                            .map(|(a, b)| format!("  L{}-{}", a, b))
                            .unwrap_or_default();
                        lines.push(Line::from(format!(
                            " {} {} {}{}",
                            vis, it.kind, it.name, span
                        )));
                    }
                    if items.len() > CAP {
                        lines.push(Line::from(format!("  … +{}", items.len() - CAP)));
                    }
                }
            }
            if !ex.todos.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("TODO ({}):", ex.todos.len()),
                    Style::default().fg(Color::Red),
                )));
                for (ln, kind, text) in ex.todos.iter().take(CAP) {
                    lines.push(Line::from(format!(" {}: [{}] {}", ln, kind, text)));
                }
                if ex.todos.len() > CAP {
                    lines.push(Line::from(format!("  … +{}", ex.todos.len() - CAP)));
                }
            }
        }
    }
    // スクロール適用（上限は「最終行が最下段に来る」まで）。
    // Paragraph の scroll は折り返し後の表示行単位のため、上限も
    // 折り返し後の行数（表示幅から見積もり）で数える。
    let inner_width = panes[1].width.saturating_sub(2).max(1) as usize; // 枠線ぶん
    let inner_height = panes[1].height.saturating_sub(2) as usize;
    let total_lines: usize = lines
        .iter()
        .map(|l| l.width().div_ceil(inner_width).max(1))
        .sum();
    app.detail_extent = (total_lines, inner_height);
    let max_scroll = total_lines.saturating_sub(inner_height) as u16;
    if app.detail_scroll > max_scroll {
        app.detail_scroll = max_scroll;
    }
    let scrollable = total_lines > inner_height;
    let title = if scrollable {
        tr(
            " Details (Shift+J/K · PgUp/PgDn scroll) ",
            " 詳細 (Shift+J/K · PgUp/PgDn スクロール) ",
        )
    } else {
        tr(" Details ", " 詳細 ")
    };
    let details = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0))
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(details, panes[1]);
    if scrollable {
        let mut sb_state = ScrollbarState::new(max_scroll as usize)
            .position(app.detail_scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            panes[1],
            &mut sb_state,
        );
    }

    // ── ステータスバー ────────────────────────────────────────
    // 幅に収まるときは固定表示、収まらないときだけ電光掲示板のように流す
    let status = if app.filter_mode {
        format!("/{}", app.filter)
    } else if !app.filter.is_empty() {
        format!(
            "/{}  {}",
            app.filter,
            tr("(Esc in filter mode to clear)", "(フィルタ中に Esc で解除)")
        )
    } else {
        tr(
            " ↑↓/jk tree · Ctrl+D/U tree page · →/Enter open · ← close · Space toggle · Shift+J/K details · PgUp/PgDn(fn+↑↓) details page · s size-sort · a hidden · / filter · q quit",
            " ↑↓/jk ツリー · Ctrl+D/U ツリーページ · →/Enter 展開 · ← 閉じる · Space 開閉 · Shift+J/K 詳細 · PgUp/PgDn(fn+↑↓) 詳細ページ · s サイズ順 · a 隠し · / フィルタ · q 終了",
        )
        .to_string()
    };
    let bar_width = outer[1].width as usize;
    let status_width = unicode_width::UnicodeWidthStr::width(status.as_str());
    let status = if !app.filter_mode && status_width > bar_width {
        marquee_slice(&status, ticker, bar_width)
    } else {
        status
    };
    f.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
        outer[1],
    );
}
