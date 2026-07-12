// DirlensCore のユニットテスト（Rust 版 #[cfg(test)] の等価移植＋Swift 固有分）。
// swift-testing を使う（XCTest は Command Line Tools 環境に無いため）。

import Testing
@testable import DirlensCore

@Suite struct FmtTests {
    @Test func sizes() {
        #expect(fmtSize(0, false) == "0 bytes")
        #expect(fmtSize(1, false) == "1 byte")
        #expect(fmtSize(1, true) == "1+ bytes")
        #expect(fmtSize(1024, false) == "1 KB")
        #expect(fmtSize(1536, false) == "1.5 KB")
        #expect(fmtSize(1587, false) == "1.55 KB")
        #expect(fmtSize(307200, false) == "300 KB")
    }

    @Test func splitextCompat() {
        #expect(splitext("foo.py") == ("foo", ".py"))
        #expect(splitext(".env") == (".env", ""))
        #expect(splitext(".env.local") == (".env", ".local"))
        #expect(splitext("Makefile") == ("Makefile", ""))
        #expect(splitext("a.tar.gz") == ("a.tar", ".gz"))
        #expect(splitext("...") == ("...", ""))
        #expect(splitext("..a.b") == ("..a", ".b"))
    }

    @Test func dates() {
        #expect(fmtDate(1000.0, 990.0) == "今")
        #expect(fmtDate(90.0 * 60.0, 0.0) == "1時間前")
        #expect(fmtDate(100.0 * 86400.0, 0.0) == "3ヶ月前")
        #expect(fmtDate(30.0 * 3600.0, 0.0) == "昨日")
    }

    @Test func parseSizes() {
        #expect((try? parseSize("1K").get()) == 1024)
        #expect((try? parseSize("1.5M").get()) == 1572864)
        #expect((try? parseSize("2000").get()) == 2000)
        #expect((try? parseSize("xyz").get()) == nil)
        // 大文字化で長さが変わる文字でも壊れない（CPython と同じ解釈）:
        // "5ﬆ".upper() == "5ST" → 接尾辞 "T"、head は "5"
        #expect((try? parseSize("5ﬆ").get()) == 5 << 40)
        #expect((try? parseSize("ﬆ").get()) == nil)
    }

    @Test func sanitizeControlChars() {
        #expect(sanitizeCtrl("main.py") == "main.py")
        #expect(sanitizeCtrl("日本語 📁.txt") == "日本語 📁.txt")
        #expect(sanitizeCtrl("evil\u{1b}[2J.txt") == "evil?[2J.txt")
        #expect(sanitizeCtrl("t\u{1b}]0;pwned\u{07}.md") == "t?]0;pwned?.md")
        #expect(sanitizeCtrl("a\n└── fake.txt") == "a?└── fake.txt")
        #expect(sanitizeCtrl("x\u{9b}31mred") == "x?31mred")
        #expect(sanitizeCtrl("tab\there") == "tab?here")
    }

    @Test func filemodeCompat() {
        #expect(filemode(0o100644) == "-rw-r--r--")
        #expect(filemode(0o040755) == "drwxr-xr-x")
        #expect(filemode(0o120777) == "lrwxrwxrwx")
        #expect(filemode(0o104755) == "-rwsr-xr-x")
    }
}

@Suite struct PycTests {
    @Test func roundHalfEven() {
        #expect(pyRound(2.5) == 2)
        #expect(pyRound(3.5) == 4)
        #expect(pyRound(-2.5) == -2)
        #expect(pyRound(0.5) == 0)
    }

    @Test func fmtPrecMatchesPython() {
        #expect(fmtPrec(1.25, 1) == "1.2")
        #expect(fmtPrec(0.5, 0) == "0")
        #expect(fmtPrec(1536.0 / 1024.0, 2) == "1.50")
    }

    @Test func decodeIgnore() {
        #expect(decodeUTF8Ignore(Array("caf".utf8) + [0xc3, 0xa9, 0x20, 0xff, 0xfe, 0x20] + Array("end".utf8))
            == "café  end")
        #expect(decodeUTF8Ignore(Array("abc".utf8) + [0xe3, 0x81]) == "abc")
    }

    @Test func splitLinesCRLF() {
        // Swift の Character は "\r\n" を 1 書記素にするため専用の分割を使う
        #expect(splitLines("a\r\nb\r\n") == ["a\r", "b\r", ""])
        #expect(splitLines("a\nb") == ["a", "b"])
        #expect(countLines("line one\r\nline two\r\n", 20, nil, false) == 2)
    }

    @Test func pyLessIsCodepointOrder() {
        #expect(pyLess("a", "b"))
        #expect(pyLess("a", "aa"))
        #expect(!pyLess("b", "a"))
        #expect(!pyLess("a", "a"))
    }
}

@Suite struct FnmatchTests {
    @Test func basics() {
        #expect(fnmatchCase("foo.md", "*.md"))
        #expect(!fnmatchCase("foo.mdx", "*.md"))
        #expect(fnmatchCase("a/b/c.md", "*.md")) // '*' は '/' を跨ぐ（CPython 互換）
        #expect(fnmatchCase("abc", "a?c"))
        #expect(fnmatchCase("a-c", "a[-x]c"))
        #expect(fnmatchCase("abc", "a[a-z]c"))
        #expect(!fnmatchCase("aBc", "a[a-z]c"))
        #expect(fnmatchCase("aBc", "a[!a-z]c"))
        #expect(fnmatchCase("a]c", "a[]]c"))
        #expect(fnmatchCase("a[c", "a[c")) // 閉じ ] 無し → リテラル
        #expect(!fnmatchCase("[bracket].txt", "[bracket].txt")) // クラス扱い
        #expect(fnmatchCase("b.txt", "[bracket].txt"))
    }
}

@Suite struct BpeTests {
    @Test func exactCounts() throws {
        let enc = try #require(O200KTokenizer.shared, "o200k 語彙リソースが必要")
        // o200k_base: "hello world" は 2 トークン
        #expect(enc.countTokens("hello world") == 2)
        #expect(countTokens("hello world", 11, nil, false, true) == 2)
        #expect(countTokens("", 0, nil, false, true) == 0)
        // preferBpe=false はヒューリスティック（11 ASCII 文字 / 4 → round(2.75) = 3）
        #expect(countTokens("hello world", 11, nil, false, false) == 3)
        #expect(enc.countTokens("日本語のテキストです。") > 0)
        #expect(enc.countTokens("def hello():\n    return 'hi'\n") > 0)
    }

    @Test func truncationScales() throws {
        _ = try #require(O200KTokenizer.shared, "o200k 語彙リソースが必要")
        let text = String(repeating: "abcd ", count: 100)
        let full = countTokens(text, 500, 1000, true, true)
        let half = countTokens(text, 500, 500, false, true)
        #expect(full >= half * 2 - 1)
    }
}

@Suite struct CycleTests {
    @Test func basic() {
        let m = ["a": ["b"], "b": ["a"]]
        #expect(detectCycles(m) == [["a", "b", "a"]])
    }

    @Test func deepChainNoStackOverflow() {
        // 10万ノードの直列 import 連鎖＋末尾→先頭の逆辺（1つの巨大サイクル）。
        // 再帰 DFS だとコールスタックが溢れるケース。
        let n = 100_000
        var m: [String: [String]] = [:]
        for i in 0..<n {
            let next = (i + 1 == n) ? 0 : i + 1
            m[String(format: "f%06d", i)] = [String(format: "f%06d", next)]
        }
        let cycles = detectCycles(m)
        #expect(cycles.count == 1)
        #expect(cycles[0].count == n + 1)
    }
}

@Suite struct ScannerTests {
    @Test func pyMaskIgnoresDocstringSymbols() {
        let code = "DOC = \"\"\"\ndef fake_in_string(x):\n    pass\n\"\"\"\n\ndef real_fn():\n    return DOC\n"
        let outline = structOutline(code, ".py")!
        #expect(outline.map { $0.name } == ["real_fn"])
        // 互換層（正規表現）は拾う（dirlens.py と同じ偽陽性を保存）
        let regex = extractOutline(code, ".py")!
        #expect(regex.map { $0.name }.contains("fake_in_string"))
    }

    @Test func jsScannerMatchesOxcSemantics() {
        let code = """
        export class Kind {
          name = '';
        }

        export const DEFAULT_KIND = (
          new Kind()
        );
        export const bootstrap = async () => {
          return 1;
        };
        const arrowLocal = (b: number) => b - 1;
        function plain() {}
        const s = "class NotReal {";
        """
        let outline = jsStructOutline(maskJs(code), ".ts")
        #expect(outline.map { [$0.kind, $0.name, String($0.isPublic)] } == [
            ["class", "Kind", "true"],
            ["func", "bootstrap", "true"],
            ["func", "arrowLocal", "false"],
            ["func", "plain", "false"],
        ])
    }

    @Test func jsScannerImports() {
        let code = """
        import { helper } from './lib/util';
        import React from 'react';
        const data = require('./lib/data');
        export * from './reexport';
        const s = "import x from 'phantom-pkg'";
        export const go = async () => import('./cli');
        """
        let imports = jsStructImports(code, ".js")
        #expect(imports == ["./lib/util", "react", "./reexport", "./lib/data", "./cli"])
    }

    @Test func rustUseExpansion() {
        var out: [String] = []
        expandUseTree("crate::util::{Thing, io::Writer}", prefix: "", into: &out)
        #expect(out == ["crate::util::Thing", "crate::util::io::Writer"])
        out = []
        expandUseTree("std::fmt as f", prefix: "", into: &out)
        #expect(out == ["std::fmt"])
        out = []
        expandUseTree("crate::a::*", prefix: "", into: &out)
        #expect(out == ["crate::a::*"])
    }

    @Test func rustScannerIgnoresStrings() {
        let code = "const S: &str = \"pub fn fake() {}\";\n// pub fn commented() {}\npub fn real() {}\n"
        let outline = structOutline(code, ".rs")!
        #expect(outline.map { $0.name } == ["real"])
    }

    @Test func swiftOutlineAndImports() {
        let code = """
        import Foundation

        public struct Config {
            public func load() -> Int { 1 }
        }

        func helper() {}
        // func commented() {}
        let s = "func fake() {}"
        actor Worker {}
        """
        let outline = structOutline(code, ".swift")!
        #expect(outline.map { [$0.kind, $0.name, String($0.isPublic)] } == [
            ["struct", "Config", "true"],
            ["func", "load", "true"],
            ["func", "helper", "false"],
            ["actor", "Worker", "false"],
        ])
        #expect(swiftStructImports(code) == ["Foundation"])
    }

    @Test func goScannerImports() {
        let code = """
        package main

        import (
            "fmt"

            "example.com/demo/internal/util"
        )

        var s = "import \\"phantom\\""
        """
        #expect(goStructImports(code) == ["fmt", "example.com/demo/internal/util"])
    }
}

@Suite struct JsonTests {
    @Test func prettyMatchesSerde() {
        var obj = JSONObject()
        obj.insert("a", .int(1))
        obj.insert("b", .array([.string("x"), .null]))
        obj.insert("c", .object(JSONObject()))
        obj.insert("d", .array([]))
        let expected = """
        {
          "a": 1,
          "b": [
            "x",
            null
          ],
          "c": {},
          "d": []
        }
        """
        #expect(JSONValue.object(obj).pretty() == expected)
    }

    @Test func escape() {
        #expect(jsonEscape("a\"b\\c\nd\u{1b}e") == "\"a\\\"b\\\\c\\nd\\u001be\"")
        #expect(jsonEscape("日本語") == "\"日本語\"")
    }

    @Test func parser() {
        let v = JSONParser.parse("{\"main\": \"src/index.js\", \"bin\": {\"x\": \"cli.js\"}, \"n\": 3}")
        #expect(v?.get("main")?.asString == "src/index.js")
        #expect(v?.get("bin")?.get("x")?.asString == "cli.js")
    }
}

@Suite struct GitignoreTests {
    @Test func builtinMatcher() {
        let pats = ["*.log", "!important.log", "build/", "temp*"]
        #expect(isIgnored("app.log", "app.log", false, pats))
        #expect(!isIgnored("important.log", "important.log", false, pats))
        #expect(isIgnored("build", "build", true, pats))
        #expect(!isIgnored("build", "build", false, pats))
        #expect(isIgnored("temp1.txt", "temp1.txt", false, pats))
        #expect(isIgnored("deep.log", "src/deep.log", false, pats))
    }
}
