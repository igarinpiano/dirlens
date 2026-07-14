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
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use dirlens_core::analysis::extras::{file_extras, FileExtras};
use dirlens_core::analysis::gitlog::load_git_log;
use dirlens_core::filter::{filter_entries, sort_entries};
use dirlens_core::fmt::{fmt_date, fmt_size, fmt_tokens};
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
            let (map, counts) = load_git_log(&StdGit, &self.cfg.root);
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
        lines: e.lines,
        git: e.git.clone(),
        todos: e.todos.clone(),
        is_entry: e.is_entry,
        is_config: e.is_config,
        no_test: e.no_test,
        outline: e.outline.clone(),
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
    loop {
        list_state.select(Some(app.selected));
        terminal
            .draw(|f| draw(f, app, &mut list_state, lang))
            .map_err(|e| e.to_string())?;

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
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => break,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
            KeyCode::Up | KeyCode::Char('k') => {
                app.selected = app.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.selected + 1 < app.visible.len() {
                    app.selected += 1;
                }
            }
            KeyCode::PageUp => app.selected = app.selected.saturating_sub(20),
            KeyCode::PageDown => {
                app.selected = (app.selected + 20).min(app.visible.len().saturating_sub(1));
            }
            KeyCode::Char('g') => app.selected = 0,
            KeyCode::Char('G') => app.selected = app.visible.len().saturating_sub(1),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => app.toggle_expand(Some(true)),
            KeyCode::Left | KeyCode::Char('h') => app.collapse_or_parent(),
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

fn draw(f: &mut ratatui::Frame, app: &mut App, list_state: &mut ListState, lang: Lang) {
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
            if let Some(items) = &ex.outline {
                if !items.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        tr("Outline:", "アウトライン:"),
                        Style::default().fg(Color::Cyan),
                    )));
                    for it in items.iter().take(14) {
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
                    if items.len() > 14 {
                        lines.push(Line::from(format!("  … +{}", items.len() - 14)));
                    }
                }
            }
            if !ex.todos.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("TODO ({}):", ex.todos.len()),
                    Style::default().fg(Color::Red),
                )));
                for (ln, kind, text) in ex.todos.iter().take(6) {
                    lines.push(Line::from(format!(" {}: [{}] {}", ln, kind, text)));
                }
            }
        }
    }
    let details = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(tr(" Details ", " 詳細 ")));
    f.render_widget(details, panes[1]);

    // ── ステータスバー ────────────────────────────────────────
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
            " ↑↓ move · →/Enter open · ← close · Space toggle · s size-sort · a hidden · / filter · q quit",
            " ↑↓ 移動 · →/Enter 展開 · ← 閉じる · Space 開閉 · s サイズ順 · a 隠し · / フィルタ · q 終了",
        )
        .to_string()
    };
    f.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
        outer[1],
    );
}
