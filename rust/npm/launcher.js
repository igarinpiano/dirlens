#!/usr/bin/env node
// dirlens – npm 配布用の薄いランチャ。
// optionalDependencies として同時インストールされる機種別バイナリパッケージ
// （dirlens-bin-<platform>-<arch>）から実バイナリを見つけて exec する。
// （esbuild / swc / Biome / turbo と同じ定番方式）
"use strict";
const { spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");

// linux x64/arm64 は glibc 版と musl 版（Alpine 等）を分けて配布している。
// os/cpu だけでは区別できないため、両方が package.json の optionalDependencies に
// 並び（musl 版には "libc": ["musl"] を付与）、対応した npm（9+）は libc も見て
// 適切な方だけ入れる。古い npm は libc フィールドを無視して両方入れることがある
// ため、実行時にも isMusl() で判定して musl ホストでは musl 版を優先する。
const PLATFORMS = {
  "darwin arm64": { pkg: "dirlens-bin-darwin-arm64" },
  "darwin x64": { pkg: "dirlens-bin-darwin-x64" },
  "linux arm64": { pkg: "dirlens-bin-linux-arm64", muslPkg: "dirlens-bin-linux-arm64-musl" },
  "linux x64": { pkg: "dirlens-bin-linux-x64", muslPkg: "dirlens-bin-linux-x64-musl" },
  "linux ppc64": { pkg: "dirlens-bin-linux-ppc64" },
  "linux s390x": { pkg: "dirlens-bin-linux-s390x" },
  "win32 x64": { pkg: "dirlens-bin-win32-x64" },
  "win32 arm64": { pkg: "dirlens-bin-win32-arm64" },
};

// esbuild 等が使う定番の判定方法: Node の process.report にはビルド時の glibc
// バージョンが載る（musl ビルドの Node には無い）。process.report が使えない
// 古い Node では ldd の出力に "musl" が含まれるかで判定する。
function isMusl() {
  if (process.platform !== "linux") return false;
  if (!process.report || typeof process.report.getReport !== "function") {
    try {
      return fs.readFileSync("/usr/bin/ldd", "utf8").includes("musl");
    } catch (e) {
      return false;
    }
  }
  const { glibcVersionRuntime } = process.report.getReport().header;
  return !glibcVersionRuntime;
}

function resolveFromPkg(pkg, exe) {
  try {
    return require.resolve(`${pkg}/bin/${exe}`);
  } catch (e) {
    // node_modules/dirlens/bin/ → node_modules/<pkg>/bin/ へのフォールバック
    const local = path.join(__dirname, "..", "..", pkg, "bin", exe);
    if (fs.existsSync(local)) return local;
    return null;
  }
}

function findBinary() {
  const key = `${process.platform} ${process.arch}`;
  const entry = PLATFORMS[key];
  if (!entry) {
    console.error(`dirlens: 未対応のプラットフォームです (${key})`);
    process.exit(1);
  }
  const exe = process.platform === "win32" ? "dirlens.exe" : "dirlens";
  const candidates = entry.muslPkg && isMusl() ? [entry.muslPkg, entry.pkg] : [entry.pkg];
  for (const pkg of candidates) {
    const resolved = resolveFromPkg(pkg, exe);
    if (resolved) return resolved;
  }
  console.error(
    `dirlens: バイナリパッケージ ${candidates.join(" / ")} が見つかりません。\n` +
      "npm install をやり直すか、--force オプション無しで再インストールしてください。"
  );
  process.exit(1);
}

const result = spawnSync(findBinary(), process.argv.slice(2), {
  stdio: "inherit",
});
if (result.error) {
  console.error(`dirlens: 起動に失敗しました: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
