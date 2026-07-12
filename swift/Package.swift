// swift-tools-version:5.9
// dirlens – ファイルサイズ付きディレクトリツリー表示ツール（Swift 移植）
//
// 依存ポリシー: 外部 SwiftPM パッケージには一切依存しない（Foundation のみ）。
// 外部依存で精度が上がる解析（Python/JS/TS の AST）は「実行時に外部ツールを
// 探して使い、無ければ内蔵の構造走査へフォールバック」する二層構成にする。
import PackageDescription

let package = Package(
    name: "dirlens",
    platforms: [.macOS(.v13)],
    targets: [
        .target(
            name: "DirlensCore",
            path: "Sources/DirlensCore",
            resources: [
                // BPE トークナイザ（o200k_base）の語彙。tiktoken (MIT, OpenAI) 由来。
                // リソースが見つからない実行環境では文字数ヒューリスティックへ縮退する。
                .copy("Resources/o200k_base.tiktoken")
            ]
        ),
        .executableTarget(
            name: "dirlens",
            dependencies: ["DirlensCore"],
            path: "Sources/dirlens"
        ),
        .testTarget(
            name: "DirlensCoreTests",
            dependencies: ["DirlensCore"],
            path: "Tests/DirlensCoreTests"
        ),
    ]
)
