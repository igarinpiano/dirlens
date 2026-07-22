// dirlens – ファイルサイズ付きディレクトリツリー表示ツール（Rust 移植・解析コア）
//
// Copyright 2026 Igarin
// Licensed under the Apache License, Version 2.0.
// See the LICENSE file or http://www.apache.org/licenses/LICENSE-2.0
//
// このクレートは純粋な解析ロジックのみを持つ。std::fs / std::process / std::thread を
// 直接呼ばず、すべて provider トレイト経由で I/O を受け取る（native / wasm 両対応のため）。

pub mod analysis;
pub mod args;
pub mod cfg;
pub mod check;
pub mod colors;
pub mod emoji;
pub mod filter;
pub mod fmt;
pub mod fnmatch;
pub mod gitignore;
pub mod i18n;
pub mod modes;
pub mod provider;
pub mod pyc;
pub mod render_html;
pub mod report;
pub mod render_json;
pub mod render_text;
pub mod run;
pub mod session;
// 並列プリウォーム（native 専用・std::thread を使うため feature gate）。
#[cfg(feature = "parallel")]
pub mod warm;

pub use args::Args;
pub use cfg::Cfg;
pub use i18n::Lang;
pub use provider::{ClipboardProvider, Entry, FsProvider, GitProvider, StatInfo};
pub use run::{execute, prepare, prefetch_targets, run, RunResult};
pub use session::Session;
