// FsProvider / GitProvider / ClipboardProvider の std 実装
// （rust/crates/dirlens-cli/src/providers.rs の等価移植・POSIX 前提）。

import Foundation
import DirlensCore

#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif

/// lstat/stat の薄いラッパ（StdFs のメソッド名 `stat` と C API の衝突を避ける）。
private func posixStatRaw(_ path: String, follow: Bool) -> stat? {
    var st = stat()
    let rc = follow ? stat(path, &st) : lstat(path, &st)
    return rc == 0 ? st : nil
}

private func toStatInfo(_ st: stat) -> StatInfo {
    #if canImport(Darwin)
    let mtime = Double(st.st_mtimespec.tv_sec) + 1e-9 * Double(st.st_mtimespec.tv_nsec)
    let ctime = Double(st.st_ctimespec.tv_sec) + 1e-9 * Double(st.st_ctimespec.tv_nsec)
    #else
    let mtime = Double(st.st_mtim.tv_sec) + 1e-9 * Double(st.st_mtim.tv_nsec)
    let ctime = Double(st.st_ctim.tv_sec) + 1e-9 * Double(st.st_ctim.tv_nsec)
    #endif
    return StatInfo(
        size: UInt64(clamping: st.st_size),
        mtime: mtime,
        ctime: ctime,
        mode: UInt32(st.st_mode),
        uid: st.st_uid,
        gid: st.st_gid
    )
}

struct StdFs: FsProvider {
    func scanDir(_ path: String) -> [FSEntry]? {
        // opendir/readdir で OS の返す順のまま列挙する（os.scandir 相当）。
        guard let dir = opendir(path) else { return nil }
        defer { closedir(dir) }
        var out: [FSEntry] = []
        while let ent = readdir(dir) {
            let name = withUnsafePointer(to: ent.pointee.d_name) { ptr -> String in
                ptr.withMemoryRebound(to: CChar.self, capacity: 1024) {
                    String(cString: $0)
                }
            }
            if name == "." || name == ".." { continue }
            let p = joinPath(path, name)
            var isDirNf = false
            var isFileNf = false
            var isSym = false
            if let st = posixStatRaw(p, follow: false) {
                let fmt = st.st_mode & S_IFMT
                isDirNf = fmt == S_IFDIR
                isFileNf = fmt == S_IFREG
                isSym = fmt == S_IFLNK
            }
            var isDirFollow = isDirNf
            if isSym {
                if let st2 = posixStatRaw(p, follow: true) {
                    isDirFollow = (st2.st_mode & S_IFMT) == S_IFDIR
                } else {
                    isDirFollow = false
                }
            }
            out.append(FSEntry(
                name: name, path: p,
                isDirNofollow: isDirNf, isFileNofollow: isFileNf,
                isSymlink: isSym, isDirFollow: isDirFollow
            ))
        }
        return out
    }

    func stat(_ path: String, follow: Bool) -> StatInfo? {
        guard let st = posixStatRaw(path, follow: follow) else { return nil }
        return toStatInfo(st)
    }

    func readPrefix(_ path: String, limit: Int) -> [UInt8]? {
        let fd = open(path, O_RDONLY)
        guard fd >= 0 else { return nil }
        defer { close(fd) }
        var buf = [UInt8]()
        buf.reserveCapacity(min(limit, 1 << 20))
        var chunk = [UInt8](repeating: 0, count: 1 << 16)
        while buf.count < limit {
            let want = min(chunk.count, limit - buf.count)
            let n = chunk.withUnsafeMutableBytes { ptr in
                read(fd, ptr.baseAddress, want)
            }
            if n < 0 { return nil }
            if n == 0 { break }
            buf.append(contentsOf: chunk[0..<n])
        }
        return buf
    }

    func readLink(_ path: String) -> String? {
        var buf = [CChar](repeating: 0, count: 4096)
        let n = readlink(path, &buf, buf.count - 1)
        guard n >= 0 else { return nil }
        buf[n] = 0
        return String(cString: buf)
    }

    func realPath(_ path: String) -> String {
        var buf = [CChar](repeating: 0, count: Int(PATH_MAX) + 1)
        if realpath(path, &buf) != nil {
            return String(cString: buf)
        }
        return path
    }

    func resolve(_ path: String) -> String? {
        var buf = [CChar](repeating: 0, count: Int(PATH_MAX) + 1)
        if realpath(path, &buf) != nil {
            return String(cString: buf)
        }
        // 存在しないパス: Python の Path.resolve(strict=False) 同様、
        // 絶対化＋字句的正規化のみ行う（cwd が取れない場合は nil）。
        let abs: String
        if path.hasPrefix("/") {
            abs = path
        } else {
            guard getcwd(&buf, buf.count) != nil else { return nil }
            abs = joinPath(String(cString: buf), path)
        }
        var comps: [String] = []
        for c in abs.split(separator: "/") {
            if c == "." { continue }
            if c == ".." {
                if !comps.isEmpty { comps.removeLast() }
                continue
            }
            comps.append(String(c))
        }
        return "/" + comps.joined(separator: "/")
    }

    func now() -> Double {
        return Date().timeIntervalSince1970
    }

    func userName(_ uid: UInt32) -> String? {
        guard let pw = getpwuid(uid) else { return nil }
        return String(cString: pw.pointee.pw_name)
    }

    func groupName(_ gid: UInt32) -> String? {
        guard let gr = getgrgid(gid) else { return nil }
        return String(cString: gr.pointee.gr_name)
    }
}

// ─── サブプロセス実行 ─────────────────────────────────────────

let gitTimeout: TimeInterval = 8.0

/// タイムアウトつきでコマンドを実行し (終了ステータス, stdout) を返す。
/// input があれば stdin へ書き込む。git が固まった場合（fsmonitor フック・
/// ネットワーク FS 等）に dirlens 全体がハングしないよう、全 git 呼び出しは
/// 必ずこの関数を通すこと。パイプ詰まりを避けるため stdout / stdin は
/// 別スレッドで処理する。
///
/// Foundation の Process は 1 呼び出しあたり数十 ms のオーバーヘッドがあり
/// git check-ignore の BFS（レベルごとに 1 回）で効いてくるため、
/// posix_spawn を直接使う。
func runWithTimeout(
    _ launchPath: String, _ args: [String], timeout: TimeInterval, input: Data? = nil
) -> (Int32, Data)? {
    var stdoutPipe: [Int32] = [0, 0]
    guard pipe(&stdoutPipe) == 0 else { return nil }
    var stdinPipe: [Int32] = [-1, -1]
    if input != nil {
        guard pipe(&stdinPipe) == 0 else {
            close(stdoutPipe[0])
            close(stdoutPipe[1])
            return nil
        }
    }

    var actions: posix_spawn_file_actions_t? = nil
    posix_spawn_file_actions_init(&actions)
    defer { posix_spawn_file_actions_destroy(&actions) }
    posix_spawn_file_actions_adddup2(&actions, stdoutPipe[1], 1)
    if input != nil {
        posix_spawn_file_actions_adddup2(&actions, stdinPipe[0], 0)
    } else {
        posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0)
    }
    posix_spawn_file_actions_addopen(&actions, 2, "/dev/null", O_WRONLY, 0)
    for fd in [stdoutPipe[0], stdoutPipe[1], stdinPipe[0], stdinPipe[1]] where fd >= 0 {
        posix_spawn_file_actions_addclose(&actions, fd)
    }

    var argv: [UnsafeMutablePointer<CChar>?] = ([launchPath] + args).map { strdup($0) }
    argv.append(nil)
    defer {
        for p in argv where p != nil {
            free(p)
        }
    }

    var pid: pid_t = 0
    let rc = posix_spawn(&pid, launchPath, &actions, nil, argv, environ)
    // 親側は子に渡した端を閉じる（EOF 検知のため）
    close(stdoutPipe[1])
    if input != nil {
        close(stdinPipe[0])
    }
    guard rc == 0 else {
        close(stdoutPipe[0])
        if input != nil {
            close(stdinPipe[1])
        }
        return nil
    }

    if let input {
        let wfd = stdinPipe[1]
        signal(SIGPIPE, SIG_IGN)
        Thread.detachNewThread {
            input.withUnsafeBytes { (buf: UnsafeRawBufferPointer) in
                var off = 0
                while off < buf.count {
                    let n = write(wfd, buf.baseAddress!.advanced(by: off), buf.count - off)
                    if n <= 0 { break }
                    off += n
                }
            }
            close(wfd)
        }
    }

    // 読み切りは別スレッドで（プロセス終了待ちとパイプ詰まりのデッドロック回避）
    let readDone = DispatchSemaphore(value: 0)
    let outBox = OutBox()
    let rfd = stdoutPipe[0]
    Thread.detachNewThread {
        var buf = [UInt8](repeating: 0, count: 1 << 16)
        while true {
            let n = read(rfd, &buf, buf.count)
            if n <= 0 { break }
            outBox.data.append(contentsOf: buf[0..<n])
        }
        close(rfd)
        readDone.signal()
    }

    let start = Date()
    var status: Int32 = 0
    while true {
        let r = waitpid(pid, &status, WNOHANG)
        if r == pid { break }
        if r < 0 { return nil }
        if Date().timeIntervalSince(start) > timeout {
            kill(pid, SIGKILL)
            waitpid(pid, &status, 0)
            return nil
        }
        usleep(2_000)
    }
    _ = readDone.wait(timeout: .now() + 5)
    let exitCode: Int32
    if status & 0x7f == 0 {
        exitCode = (status >> 8) & 0xff // WEXITSTATUS
    } else {
        return nil // シグナル終了
    }
    return (exitCode, Data(outBox.data))
}

/// リーダスレッドと共有する出力バッファ。
private final class OutBox: @unchecked Sendable {
    var data: [UInt8] = []
}

/// PATH からコマンドを探す（--check の存在確認用）。フルパスを返す。
func findCmd(_ name: String) -> String? {
    guard let paths = ProcessInfo.processInfo.environment["PATH"] else { return nil }
    let fm = FileManager.default
    for dir in paths.split(separator: ":") {
        let p = joinPath(String(dir), name)
        var isDir: ObjCBool = false
        if fm.fileExists(atPath: p, isDirectory: &isDir), !isDir.boolValue {
            return p
        }
    }
    return nil
}

func hasCmd(_ name: String) -> Bool {
    return findCmd(name) != nil
}

// ─── git ─────────────────────────────────────────────────────

struct StdGit: GitProvider {
    func logOutput(root: String, maxCommits: Int) -> String? {
        guard let git = findCmd("git") else { return nil }
        guard let (status, out) = runWithTimeout(git, [
            "-C", root, "log", "-n", String(maxCommits),
            "--name-only", "--date=relative",
            "--pretty=format:\u{1}%H\u{2}%ad\u{2}%an\u{2}%s\u{3}",
        ], timeout: gitTimeout) else {
            return nil
        }
        guard status == 0 else { return nil }
        return String(decoding: out, as: UTF8.self)
    }

    func checkIgnore(root: String, relPaths: [String]) -> [String]? {
        if relPaths.isEmpty { return [] }
        guard let git = findCmd("git") else { return nil }
        var input = Data()
        for p in relPaths {
            input.append(contentsOf: Array(p.utf8))
            input.append(0)
        }
        guard let (status, out) = runWithTimeout(
            git, ["-C", root, "check-ignore", "--stdin", "-z"],
            timeout: gitTimeout, input: input
        ) else {
            return nil
        }
        // check-ignore: 0=無視あり, 1=無視なし, 128=エラー（非リポジトリ等）
        guard status == 0 || status == 1 else { return nil }
        let text = String(decoding: out, as: UTF8.self)
        return text.split(separator: "\0").map(String.init)
    }

    func available() -> Bool {
        return hasCmd("git")
    }

    func isWorkTree(root: String) -> Bool {
        guard let git = findCmd("git") else { return false }
        guard let (status, out) = runWithTimeout(
            git, ["-C", root, "rev-parse", "--is-inside-work-tree"], timeout: gitTimeout
        ) else {
            return false
        }
        return status == 0
            && String(decoding: out, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines) == "true"
    }
}

// ─── クリップボード ───────────────────────────────────────────

struct StdClipboard: ClipboardProvider {
    private func pipeTo(_ cmd: [String], _ input: Data) -> Bool? {
        guard let path = findCmd(cmd[0]) else { return nil } // NotFound 相当
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: path)
        proc.arguments = Array(cmd.dropFirst())
        let inPipe = Pipe()
        proc.standardInput = inPipe
        proc.standardOutput = FileHandle.nullDevice
        proc.standardError = FileHandle.nullDevice
        do {
            try proc.run()
        } catch {
            return nil
        }
        let fh = inPipe.fileHandleForWriting
        try? fh.write(contentsOf: input)
        try? fh.close()
        proc.waitUntilExit()
        return proc.terminationStatus == 0
    }

    func copy(_ text: String) -> Bool {
        let data = Data(text.utf8)
        #if os(macOS)
        return pipeTo(["pbcopy"], data) ?? false
        #else
        for cmd in [["wl-copy"], ["xclip", "-selection", "clipboard"], ["xsel", "--clipboard", "--input"]] {
            if let ok = pipeTo(cmd, data) {
                return ok
            }
        }
        return false
        #endif
    }

    func available() -> Bool {
        #if os(macOS)
        return hasCmd("pbcopy")
        #else
        return hasCmd("wl-copy") || hasCmd("xclip") || hasCmd("xsel")
        #endif
    }
}
