// Where the wasmcart package is: WASMCART_DIR (a checkout), a normal install,
// or the copy npx fetched for `npx wasmcart` (test.sh's default).
import { createRequire } from 'node:module';
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export function wasmcartDir() {
  if (process.env.WASMCART_DIR) return process.env.WASMCART_DIR;
  const require = createRequire(import.meta.url);
  try {
    return path.dirname(require.resolve('wasmcart/package.json'));
  } catch {}
  const npx = path.join(os.homedir(), '.npm', '_npx');
  const find = () => {
    if (!fs.existsSync(npx)) return [];
    return fs
      .readdirSync(npx)
      .map((d) => path.join(npx, d, 'node_modules', 'wasmcart'))
      .filter((d) => fs.existsSync(path.join(d, 'package.json')))
      .sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs);
  };
  if (!find().length) execSync('npx --yes wasmcart --help', { stdio: 'ignore' });
  const found = find();
  if (!found.length) throw new Error('wasmcart not found: npm i wasmcart, or set WASMCART_DIR');
  return found[0];
}
