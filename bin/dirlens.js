#!/usr/bin/env node
'use strict';
const { spawnSync } = require('child_process');
const path = require('path');

const script = path.join(__dirname, '..', 'dirlens.py');
const args = process.argv.slice(2);

function findPython() {
  if (process.env.DIRLENS_PYTHON) return process.env.DIRLENS_PYTHON;
  if (process.platform === 'win32') return 'python';
  return 'python3';
}

const python = findPython();
const result = spawnSync(python, [script, ...args], { stdio: 'inherit' });

if (result.error) {
  process.stderr.write(
    `dirlens: failed to start "${python}": ${result.error.message}\n` +
    `dirlens requires Python 3.8+ to be available on PATH.\n` +
    `Try: python3 --version\n` +
    `Or run directly: python3 "${script}" ${args.join(' ')}\n`
  );
  process.exit(127);
}

process.exit(result.status ?? 1);
