#!/usr/bin/env node
'use strict';
const { spawnSync } = require('child_process');
const path = require('path');

const python = process.platform === 'win32' ? 'python' : 'python3';
const script = path.join(__dirname, '..', 'dirlens.py');
const args = process.argv.slice(2);

const result = spawnSync(python, [script, ...args], { stdio: 'inherit' });

if (result.error) {
  // python3 が PATH に無い等で spawn 自体に失敗した場合、
  // 以前はここで何も表示せず exit 1 になっていた（無言の失敗）。
  console.error(`dirlens: failed to start "${python}": ${result.error.message}`);
  console.error('dirlens requires Python 3.8+ to be available on PATH.');
  console.error('Try: python3 --version');
  console.error(`Or run directly: python3 "${script}" ${args.join(' ')}`);
  process.exit(127);
}

process.exit(result.status ?? 1);
