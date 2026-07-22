//! 進捗スピナー（stderr が端末のときのみ）。
//!
//! 起動から 400ms 以上かかるスキャンでだけ現れ、完了時に行を消す。
//! 語彙はツリー・レンズ・探索をテーマにした dirlens オリジナルの現在分詞。
//! （既存ツールの語彙リストの複製ではない）

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// dirlens オリジナルの「〜ing」語彙ストック。
/// テーマ: 木（tree）・レンズ（lens）・書庫・探検・掃除。
const WORDS: &[&str] = &[
    // 木・植物
    "Branching", "Leafing", "Rooting", "Pruning", "Climbing", "Sprouting",
    "Rustling", "Unfurling", "Grafting", "Mulching",
    // レンズ・視覚
    "Focusing", "Zooming", "Refocusing", "Peering", "Squinting", "Glancing",
    "Polishing", "Framing", "Panning", "Developing",
    // 計測・集計
    "Measuring", "Weighing", "Counting", "Sizing", "Tallying", "Gauging",
    "Sounding", "Surveying", "Mapping", "Charting",
    // 探索・散策
    "Traversing", "Scanning", "Wandering", "Roaming", "Trekking", "Spelunking",
    "Descending", "Skimming", "Perusing", "Foraging",
    // 書庫・整理
    "Indexing", "Cataloguing", "Shelving", "Filing", "Archiving", "Sorting",
    "Sifting", "Combing", "Rummaging", "Sweeping",
];

const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    /// stderr が端末でなければ何もしない Spinner を返す。
    pub fn start() -> Spinner {
        if !std::io::stderr().is_terminal() {
            return Spinner { stop: Arc::new(AtomicBool::new(true)), handle: None };
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let handle = std::thread::spawn(move || {
            // 速い実行では現れない（チラつき防止）
            let delay = Duration::from_millis(400);
            let started = std::time::Instant::now();
            while started.elapsed() < delay {
                if stop2.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize)
                .unwrap_or(0);
            let mut word_i = seed % WORDS.len();
            let mut frame_i = 0usize;
            let mut ticks = 0usize;
            let mut err = std::io::stderr();
            // \x1b[2K（行クリア）は Windows の従来コンソール（Windows Terminal
            // 以外の cmd.exe / PowerShell 等）では ENABLE_VIRTUAL_TERMINAL_PROCESSING
            // が有効でない限り解釈されず、行が消えずに前フレームの文字が残って
            // ちらつく。\r + 末尾スペース埋めなら制御文字のみで完結し、ANSI 非対応
            // 環境でも確実に前の行を上書きできる。
            let mut prev_len = 0usize;
            while !stop2.load(Ordering::Relaxed) {
                let word = WORDS[word_i];
                let line = format!("{} {}…", FRAMES[frame_i], word);
                let line_len = line.chars().count();
                let pad = " ".repeat(prev_len.saturating_sub(line_len));
                let _ = write!(err, "\r{line}{pad}");
                let _ = err.flush();
                prev_len = line_len;
                frame_i = (frame_i + 1) % FRAMES.len();
                ticks += 1;
                if ticks % 25 == 0 {
                    word_i = (word_i + 1) % WORDS.len(); // 約2.5秒ごとに言葉を替える
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = write!(err, "\r{}\r", " ".repeat(prev_len));
            let _ = err.flush();
        });
        Spinner { stop, handle: Some(handle) }
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
