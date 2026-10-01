// Unit tests for launcher.js (run with `node --test rust/npm/`). Not shipped:
// assemble.py copies only launcher.js into the npm package.
"use strict";
const test = require("node:test");
const assert = require("node:assert");
const { findBinary, launchErrorMessage } = require("./launcher.js");

const installed = (...pkgs) => (pkg, exe) => (pkgs.includes(pkg) ? `/nm/${pkg}/bin/${exe}` : null);
const linux = (musl, resolve) => ({ platform: "linux", arch: "x64", musl: () => musl, resolve });

test("glibc host uses the glibc build", () => {
  const f = findBinary(linux(false, installed("dirlens-bin-linux-x64", "dirlens-bin-linux-x64-musl")));
  assert.strictEqual(f.pkg, "dirlens-bin-linux-x64");
  assert.strictEqual(f.muslFallback, false);
});

test("musl host prefers the musl build", () => {
  const f = findBinary(linux(true, installed("dirlens-bin-linux-x64", "dirlens-bin-linux-x64-musl")));
  assert.strictEqual(f.pkg, "dirlens-bin-linux-x64-musl");
  assert.strictEqual(f.muslFallback, false);
});

test("musl host with only the glibc build is flagged as a fallback", () => {
  const f = findBinary(linux(true, installed("dirlens-bin-linux-x64")));
  assert.strictEqual(f.pkg, "dirlens-bin-linux-x64");
  assert.strictEqual(f.muslFallback, true);
});

test("missing packages and unsupported platforms are reported", () => {
  assert.match(findBinary(linux(false, installed())).error, /could not find binary package/);
  assert.match(findBinary({ platform: "sunos", arch: "x64" }).error, /unsupported platform/);
});

test("ENOENT after a musl fallback explains the glibc/musl mismatch", () => {
  const err = Object.assign(new Error("spawnSync /nm/x ENOENT"), { code: "ENOENT" });
  const fallback = { pkg: "dirlens-bin-linux-x64", muslFallback: true };
  const msg = launchErrorMessage(fallback, err, "x64");
  assert.match(msg, /musl system/);
  assert.match(msg, /npm install -g dirlens-bin-linux-x64-musl/);
  // Without the fallback, the message stays the plain error.
  assert.strictEqual(
    launchErrorMessage({ pkg: "dirlens-bin-linux-x64", muslFallback: false }, err, "x64"),
    "dirlens: failed to launch: spawnSync /nm/x ENOENT"
  );
});
