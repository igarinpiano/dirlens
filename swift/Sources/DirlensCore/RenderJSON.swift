// JSON レンダラ。rust/crates/dirlens-core/src/render_json.rs の等価移植。
// 出力は json.dumps(..., ensure_ascii=False, indent=2) と同一（キー順は挿入順）。

public struct JsonStats {
    public var tokens: Int64 = 0
    public var todoTotal: UInt64 = 0
    public var todoSamples: [(String, Int, String, String)] = []
}

private func buildJsonTree(
    _ sess: Session, _ path: String, _ depth: Int64, _ cfg: Cfg,
    _ activePats: [String], _ stats: inout JsonStats
) -> JSONValue {
    let curPats = extendPats(sess, activePats, path, cfg)
    let filtered = filterEntries(sess, path, cfg, curPats)
    let denied = filtered == nil
    var (dirs, files) = filtered ?? ([], [])
    if cfg.prune {
        dirs = dirs.filter { hasContent(sess, $0.path, depth + 1, cfg, curPats) }
    }
    sortEntries(sess, &dirs, &files, cfg)
    let (sz, szErr) = sess.dirSize(path)

    let nDirs = dirs.count
    let nFiles = files.count

    var children: [JSONValue] = []
    let withinDepth = cfg.maxDepth.map { depth < $0 } ?? true
    if withinDepth {
        let combined: [FSEntry] = cfg.filesFirst ? files + dirs : dirs + files
        for entry in combined {
            if entry.isDirNofollow {
                children.append(buildJsonTree(sess, entry.path, depth + 1, cfg, curPats, &stats))
            } else {
                let fSz = sess.fs.stat(entry.path, follow: true)?.size ?? 0
                let rel = relpathSlash(entry.path, cfg.root)
                let extras = cfg.hasExtras ? fileExtras(sess, entry, rel, cfg) : FileExtras()

                var obj = JSONObject()
                obj.insert("name", .string(entry.name))
                obj.insert("type", .string("file"))
                obj.insert("size", .uint(fSz))
                obj.insert("size_human", .string(fmtSize(fSz, false)))
                obj.insert("ext", .string(splitext(entry.name).1.lowercased()))
                obj.insert("path", .string(rel))

                if cfg.showTokens {
                    obj.insert("tokens", extras.tokens.map { JSONValue.int($0) } ?? .null)
                    obj.insert("lines", extras.lines.map { JSONValue.int($0) } ?? .null)
                    if let t = extras.tokens {
                        stats.tokens += t
                    }
                }
                if cfg.showGit {
                    if let g = extras.git {
                        var m = JSONObject()
                        m.insert("hash", .string(g.hash))
                        m.insert("date", .string(g.date))
                        m.insert("author", .string(g.author))
                        m.insert("subject", .string(g.subject))
                        obj.insert("git", .object(m))
                    } else {
                        obj.insert("git", .null)
                    }
                }
                if cfg.showTodo {
                    let todos: [JSONValue] = extras.todos.map { ln, k, s in
                        var m = JSONObject()
                        m.insert("line", .int(Int64(ln)))
                        m.insert("kind", .string(k))
                        m.insert("text", .string(s))
                        return .object(m)
                    }
                    stats.todoTotal += UInt64(extras.todos.count)
                    for item in extras.todos.prefix(3) where stats.todoSamples.count < 20 {
                        stats.todoSamples.append((rel, item.0, item.1, item.2))
                    }
                    obj.insert("todos", .array(todos))
                }
                if cfg.showTests {
                    obj.insert("has_test", .bool(!extras.noTest))
                }
                if cfg.showEntry {
                    obj.insert("is_entry", .bool(extras.isEntry))
                }
                if cfg.showConfig {
                    obj.insert("is_config", .bool(extras.isConfig))
                }
                if cfg.showOutline {
                    if let items = extras.outline {
                        let arr: [JSONValue] = items.map { item in
                            var m = JSONObject()
                            m.insert("kind", .string(item.kind))
                            m.insert("name", .string(item.name))
                            m.insert("public", .bool(item.isPublic))
                            return .object(m)
                        }
                        obj.insert("outline", .array(arr))
                    } else {
                        obj.insert("outline", .null)
                    }
                }
                if cfg.showImports {
                    obj.insert("imports", .array(extras.imports.map { .string($0) }))
                    obj.insert("imported_by", .array(extras.importedBy.map { .string($0) }))
                    obj.insert("external_imports", .array(extras.externalImports.map { .string($0) }))
                }

                children.append(.object(obj))
            }
        }
    }

    let name = fileName(path) ?? path
    let pathStr = path == cfg.root ? "." : relpath(path, cfg.root)

    var obj = JSONObject()
    obj.insert("name", .string(name))
    obj.insert("type", .string("directory"))
    obj.insert("size", .uint(sz))
    obj.insert("size_human", .string(fmtSize(sz, szErr)))
    obj.insert("path", .string(pathStr))
    var ic = JSONObject()
    ic.insert("dirs", .int(Int64(nDirs)))
    ic.insert("files", .int(Int64(nFiles)))
    ic.insert("permission_denied", .bool(denied))
    obj.insert("item_count", .object(ic))
    obj.insert("children", .array(children))
    return .object(obj)
}

/// `--json` 出力スキーマの版数。フィールド追加は後方互換、
/// 改名・削除・型変更時にインクリメントする（安定した公開契約・spec §8）。
public let schemaVersion = 1

/// JSON 出力全体（project_summary を含む）を文字列で返す（末尾改行つき）。
public func renderJson(_ sess: Session, _ cfg: Cfg, _ activePats: [String], _ probe: EnvProbe) -> String {
    var stats = JsonStats()
    let treeValue = buildJsonTree(sess, cfg.root, 0, cfg, activePats, &stats)
    guard var tree = treeValue.asObject else { return "{}\n" }

    if cfg.hasExtras {
        let mostDepended: JSONValue
        if cfg.showImports, !cfg.importedByMap.isEmpty {
            let items = sortDescByCount(cfg.importedByMap.pairs.map { ($0.0, $0.1.count) })
            mostDepended = .array(items.prefix(10).map { p, n in
                var m = JSONObject()
                m.insert("path", .string(p))
                m.insert("used_by_count", .int(Int64(n)))
                return .object(m)
            })
        } else {
            mostDepended = .null
        }

        let hotspots: JSONValue
        if cfg.showGit, !cfg.gitChangeCounts.isEmpty {
            let items = sortDescByCount64(cfg.gitChangeCounts.pairs)
            hotspots = .array(items.prefix(10).map { p, n in
                var m = JSONObject()
                m.insert("path", .string(p))
                m.insert("change_count", .uint(n))
                return .object(m)
            })
        } else {
            hotspots = .null
        }

        let readingOrder: JSONValue
        if cfg.showEntry, cfg.showImports, !cfg.entrySet.isEmpty || !cfg.importedByMap.isEmpty {
            readingOrder = .array(readingOrderCandidates(cfg, 5, 8).map { .string($0) })
        } else {
            readingOrder = .null
        }

        var ps = JSONObject()
        ps.insert("estimated_tokens", cfg.showTokens ? .int(stats.tokens) : .null)
        ps.insert("todo_count", cfg.showTodo ? .uint(stats.todoTotal) : .null)
        ps.insert("missing_tests_count", cfg.showTests ? .int(Int64(cfg.untestedSet.count)) : .null)
        ps.insert("entry_points_count", cfg.showEntry ? .int(Int64(cfg.entrySet.count)) : .null)
        ps.insert("config_files_count", cfg.showConfig ? .int(Int64(cfg.configSet.count)) : .null)
        ps.insert("git_available", cfg.showGit ? .bool(!cfg.gitMap.isEmpty) : .null)
        ps.insert("most_depended_on", mostDepended)
        ps.insert("hotspots", hotspots)
        if cfg.showImports, !cfg.cycles.isEmpty {
            ps.insert("circular_dependencies",
                      .array(cfg.cycles.map { cycle in .array(cycle.map { .string($0) }) }))
        } else if cfg.showImports {
            ps.insert("circular_dependencies", .array([]))
        } else {
            ps.insert("circular_dependencies", .null)
        }
        ps.insert("reading_order_candidates", readingOrder)
        tree.insert("project_summary", .object(ps))
    }

    // schema_version（先頭キー）と --agent 用メタブロック。
    // DIRLENS_COMPAT=python（suppressNotes）では Python 版とのバイト一致のため出さない。
    var result: JSONValue
    if !cfg.suppressNotes {
        var wrapped = JSONObject()
        wrapped.insert("schema_version", .int(Int64(schemaVersion)))
        for (k, v) in tree.pairs {
            wrapped.insert(k, v)
        }
        if cfg.agent {
            wrapped.insert("capabilities", capabilitiesJson(cfg, probe))
            var analysis = JSONObject()
            analysis.insert("gitignore_tier", cfg.gitignoreTier.map { JSONValue.string($0) } ?? .null)
            let outlineMode: String
            let importsMode: String
            if cfg.enhancedAnalysis {
                if probe.python3 || probe.nodeTypescript {
                    outlineMode = "ast+scanner+regex-fallback"
                    importsMode = "ast+manifest"
                } else {
                    outlineMode = "scanner+regex-fallback"
                    importsMode = "scanner+manifest"
                }
            } else {
                outlineMode = "regex"
                importsMode = "regex"
            }
            analysis.insert("outline", .string(outlineMode))
            analysis.insert("imports", .string(importsMode))
            analysis.insert("tokens", .string(tokensMode(cfg)))
            wrapped.insert("analysis", .object(analysis))
        }
        result = .object(wrapped)
    } else {
        result = .object(tree)
    }

    return result.pretty() + "\n"
}
