// Records the original JS functions' outputs: node record.js > cases.jsonl
const path = require('path');
const src = (m) => require(path.join(__dirname, '..', '..', m));
const { strings } = require('./corpus');
const out = [];
const rec = (mod, fn, args, f) => {
  let result;
  try { result = { ok: f(...args) }; } catch (e) { result = { err: String(e && e.message) }; }
  out.push(JSON.stringify({ mod, fn, args, ...result }));
};

const st = src('search-text.js');
for (const s of strings) rec('search_text', 'normalize_search', [s], st.normalizeSearch);

const rtl = src('lyrics-rtl.js');
for (const s of strings) rec('lyrics_rtl', 'is_rtl_text', [s], rtl.isRtlText);

const np = src('now-playing.js');
const nums = [NaN, 0, -1, 0.5, 3, 200, 250, Infinity];
for (const d of nums) for (const t of [NaN, 0, 10, 300]) for (const fb of [NaN, 0, 180]) for (const r of [NaN, 0, 1.5])
  rec('now_playing', 'position_state', [d, t, r, fb], (d, t, r, fb) => np.positionState({ duration: d, currentTime: t, playbackRate: r }, fb));
for (const paused of [true, false]) for (const ended of [true, false]) for (const pbu of [true, false]) for (const last of [0, 1000, 5000]) for (const now of [3500, 9000])
  rec('now_playing', 'was_playing_at_interruption', [paused, ended, pbu, last, now], (paused, ended, pausedByUser, lastPlayingAt, now) => np.wasPlayingAtInterruption({ paused, ended, pausedByUser, lastPlayingAt, now }));

module.exports = { rec, out, strings, src };
if (require.main === module) {
  for (const extra of ['record-lyrics.js', 'record-tracks.js', 'record-library.js', 'record-backup.js', 'record-profiles.js']) { try { require('./' + extra); } catch (e) { if (e.code !== 'MODULE_NOT_FOUND') throw e; } }
  process.stdout.write(out.join('\n') + '\n');
}
