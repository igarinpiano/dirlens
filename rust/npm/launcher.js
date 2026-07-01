#!/usr/bin/env node
// dirlens – npm 配布用の薄いランチャ。
// optionalDependencies として同時インストールされる機種別バイナリパッケージ
// （dirlens-bin-<platform>-<arch>）から実バイナリを見つけて exec する。
// （esbuild / swc / Biome / turbo と同じ定番方式）
"use strict";
const { spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");

const PLATFORMS = {
  "darwin arm64": "dirlens-bin-darwin-arm64",
  "darwin x64": "dirlens-bin-darwin-x64",
  "linux arm64": "dirlens-bin-linux-arm64",
  "linux x64": "dirlens-bin-linux-x64",
  "win32 x64": "dirlens-bin-win32-x64",
};

function findBinary() {
  const key = `${process.platform} ${process.arch}`;
  const pkg = PLATFORMS[key];
  if (!pkg) {
    console.error(`dirlens: 未対応のプラットフォームです (${key})`);
    process.exit(1);
  }
  const exe = process.platform === "win32" ? "dirlens.exe" : "dirlens";
  try {
    return require.resolve(`${pkg}/bin/${exe}`);
  } catch (e) {
    // node_modules/dirlens/bin/ → node_modules/<pkg>/bin/ へのフォールバック
    const local = path.join(__dirname, "..", "..", pkg, "bin", exe);
    if (fs.existsSync(local)) return local;
    console.error(
      `dirlens: バイナリパッケージ ${pkg} が見つかりません。\n` +
        "npm install をやり直すか、--force オプション無しで再インストールしてください。"
    );
    process.exit(1);
  }
}

const result = spawnSync(findBinary(), process.argv.slice(2), {
  stdio: "inherit",
});
if (result.error) {
  console.error(`dirlens: 起動に失敗しました: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
