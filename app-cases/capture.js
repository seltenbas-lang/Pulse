// Preloaded into `node --test`: logs every call the original tests make to the ported modules.
// CAPTURE_OUT=file node --require ./capture.js --test ...
const fs = require('fs');
const path = require('path');
const Module = require('module');
const ROOT = path.join(__dirname, '..', '..');
const MODS = { 'library-pages.js': 'library_pages', 'recommend-core.js': 'recommend', 'search-text.js': 'search_text', 'lyrics-rtl.js': 'lyrics_rtl', 'lyrics-match.js': 'lyrics_match', 'track-match.js': 'track_match', 'track-sort.js': 'track_sort' };
const snake = (s) => s.replace(/[A-Z]/g, (c) => '_' + c.toLowerCase());
const plain = (v, depth = 0) => {
  if (typeof v === 'function' || typeof v === 'symbol' || typeof v === 'bigint') return false;
  if (v && typeof v === 'object') return depth < 8 && Object.values(v).every((x) => plain(x, depth + 1));
  return true;
};
const load = Module.prototype.require;
const done = new Set();
Module.prototype.require = function (id) {
  const exp = load.apply(this, arguments);
  let file;
  try { file = path.relative(ROOT, Module._resolveFilename(id, this)); } catch { return exp; }
  const mod = MODS[file];
  if (!mod || done.has(exp)) return exp;
  done.add(exp);
  for (const [name, fn] of Object.entries(exp)) {
    if (typeof fn !== 'function') continue;
    exp[name] = function (...args) {
      const r = fn.apply(this, args);
      if (plain(args) && plain(r)) {
        try { fs.appendFileSync(process.env.CAPTURE_OUT, JSON.stringify({ mod, fn: snake(name), args: JSON.parse(JSON.stringify(args)), ok: r === undefined ? null : JSON.parse(JSON.stringify(r)) }) + '\n'); } catch {}
      }
      return r;
    };
  }
  return exp;
};
