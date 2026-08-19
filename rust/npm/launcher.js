#!/usr/bin/env node
// dirlens – thin launcher for npm distribution.
// Finds and execs the real binary from the platform-specific binary package
// (dirlens-bin-<platform>-<arch>) installed alongside via optionalDependencies.
// (the same well-established approach used by esbuild / swc / Biome / turbo)
"use strict";
const { spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");

// linux x64/arm64 ship separate glibc and musl (Alpine, etc.) builds. Since
// os/cpu alone can't tell them apart, both are listed in package.json's
// optionalDependencies (the musl build carries "libc": ["musl"]), and npm
// versions that support this (9+) use the libc field to install only the
// right one. Older npm versions may ignore the libc field and install both,
// so we also detect this at runtime via isMusl() and prefer the musl build
// on musl hosts.
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

// Standard detection method used by esbuild and others: Node's process.report
// includes the glibc version it was built against (absent for musl builds of
// Node). On older Node versions where process.report isn't available, fall
// back to checking whether ldd's output contains "musl".
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
    // Fallback from node_modules/dirlens/bin/ to node_modules/<pkg>/bin/
    const local = path.join(__dirname, "..", "..", pkg, "bin", exe);
    if (fs.existsSync(local)) return local;
    return null;
  }
}

function findBinary() {
  const key = `${process.platform} ${process.arch}`;
  const entry = PLATFORMS[key];
  if (!entry) {
    console.error(`dirlens: unsupported platform (${key})`);
    process.exit(1);
  }
  const exe = process.platform === "win32" ? "dirlens.exe" : "dirlens";
  const candidates = entry.muslPkg && isMusl() ? [entry.muslPkg, entry.pkg] : [entry.pkg];
  for (const pkg of candidates) {
    const resolved = resolveFromPkg(pkg, exe);
    if (resolved) return resolved;
  }
  console.error(
    `dirlens: could not find binary package ${candidates.join(" / ")}.\n` +
      "Try re-running npm install, or reinstall without the --force option."
  );
  process.exit(1);
}

const result = spawnSync(findBinary(), process.argv.slice(2), {
  stdio: "inherit",
});
if (result.error) {
  console.error(`dirlens: failed to launch: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
