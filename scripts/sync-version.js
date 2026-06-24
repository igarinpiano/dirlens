#!/usr/bin/env node
// npm version patch 実行時に package.json と pyproject.toml のバージョンを同期する。
// package.json の "version" ライフサイクルフックから呼ばれる。
'use strict';
const fs = require('fs');
const { version } = require('../package.json');
const path = require('path');

const tomlPath = path.join(__dirname, '..', 'pyproject.toml');
const updated = fs.readFileSync(tomlPath, 'utf8')
  .replace(/^version = ".*"/m, `version = "${version}"`);

fs.writeFileSync(tomlPath, updated);
console.log(`pyproject.toml: version = "${version}"`);
