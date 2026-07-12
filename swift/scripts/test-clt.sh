#!/bin/sh
# Command Line Tools のみ（Xcode 無し）の環境で swift test を動かすための補助。
# CLT は swift-testing のマクロプラグインと Testing.framework を標準の探索パスに
# 置いていないため、明示的に指定・リンクする。
set -eu
cd "$(dirname "$0")/.."

CLT=/Library/Developer/CommandLineTools
PLUGIN="$CLT/usr/lib/swift/host/plugins/testing"
FRAMEWORKS="$CLT/Library/Developer/Frameworks"
INTEROP="$CLT/Library/Developer/usr/lib/lib_TestingInterop.dylib"

mkdir -p .build/out/Products/Debug/PackageFrameworks
# ln -sf は既存 symlink の「中」に作ろうとするため -h（リンク自体を置換）を使う
ln -shf "$FRAMEWORKS/Testing.framework" .build/out/Products/Debug/PackageFrameworks/Testing.framework
[ -f "$INTEROP" ] && ln -sf "$INTEROP" .build/out/Products/Debug/lib_TestingInterop.dylib

exec swift test -Xswiftc -plugin-path -Xswiftc "$PLUGIN" "$@"
