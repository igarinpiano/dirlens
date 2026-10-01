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

// Returns { path, pkg, muslFallback } for the binary to run, or { error }.
// muslFallback is true when the host is musl but only the glibc build was
// found (e.g. npm < 9 installed it, or the musl package was omitted). That
// glibc binary usually can't start on musl (no glibc dynamic loader), and the
// spawn then fails with a bare ENOENT, so the caller explains it instead.
function findBinary({ platform = process.platform, arch = process.arch, musl = isMusl, resolve = resolveFromPkg } = {}) {
  const key = `${platform} ${arch}`;
  const entry = PLATFORMS[key];
  if (!entry) return { error: `dirlens: unsupported platform (${key})` };
  const exe = platform === "win32" ? "dirlens.exe" : "dirlens";
  const onMusl = Boolean(entry.muslPkg) && musl();
  const candidates = onMusl ? [entry.muslPkg, entry.pkg] : [entry.pkg];
  for (const pkg of candidates) {
    const resolved = resolve(pkg, exe);
    if (resolved) return { path: resolved, pkg, muslFallback: onMusl && pkg !== entry.muslPkg };
  }
  return {
    error:
      `dirlens: could not find binary package ${candidates.join(" / ")}.\n` +
      "Try re-running npm install, or reinstall without the --force option.",
  };
}

// Message for a failed spawn. A glibc build started on a musl host fails with
// ENOENT (its ELF interpreter is missing), which on its own looks like the
// binary itself is missing.
function launchErrorMessage(found, error, arch = process.arch) {
  let msg = `dirlens: failed to launch: ${error.message}`;
  if (found.muslFallback && error.code === "ENOENT") {
    const muslPkg = PLATFORMS[`linux ${arch}`] && PLATFORMS[`linux ${arch}`].muslPkg;
    msg +=
      `\ndirlens: this looks like a musl system (Alpine, etc.), but only the glibc build (${found.pkg}) is installed,` +
      " and it cannot run without glibc." +
      `\nInstall the musl build: npm install -g ${muslPkg}` +
      " (or reinstall dirlens with npm 9+ without --omit=optional / --no-optional).";
  }
  return msg;
}

function main() {
  const found = findBinary();
  if (found.error) {
    console.error(found.error);
    process.exit(1);
  }
  const result = spawnSync(found.path, process.argv.slice(2), {
    stdio: "inherit",
  });
  if (result.error) {
    console.error(launchErrorMessage(found, result.error));
    process.exit(1);
  }
  process.exit(result.status === null ? 1 : result.status);
}

if (require.main === module) {
  main();
}

module.exports = { PLATFORMS, findBinary, launchErrorMessage };
