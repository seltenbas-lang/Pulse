const { rec, strings, src } = require('./record');
const bc = src('backup-core.js');
// pinned: ids are pl_<base36(clock)>i, the clock counting from 1000 per call; ISO time fixed
const pinned = (f) => (...args) => {
  const [now, rnd, iso] = [Date.now, Math.random, Date.prototype.toISOString];
  let n = 1000;
  Date.now = () => n++; Math.random = () => 0.5; Date.prototype.toISOString = () => '2026-11-21T00:00:00.000Z';
  try { return f(...args); } finally { Date.now = now; Math.random = rnd; Date.prototype.toISOString = iso; }
};
const shared = [
  { title: 'Yellow', artist: 'Coldplay', id: 'dQw4w9WgXcQ', duration: '266.4', thumbnail: 'https://i.example/x.jpg' },
  { name: 'Song', artistName: 'A', url: 'https://youtu.be/dQw4w9WgXcQ' }, { trackName: 'T', author: 'B', url: 'https://m.youtube.com/watch?v=dQw4w9WgXcQ&t=3' },
  { title: 'S', url: 'https://www.youtube.com/shorts/dQw4w9WgXcQ?feature=x' }, { title: 'SC', url: 'https://soundcloud.com/artist/track', id: 12345, artworkUrl: 'https://a/b.png' },
  { title: 'SC root', url: 'https://soundcloud.com/' }, { title: 'Local', id: 'local_abc', path: '/x' }, { title: 'Local2', platform: 'LOCAL' }, { platform: 'local' },
  { title: 'Plain', artist: 'P', durationMs: 180000, cover: 'http://insecure/x.jpg', image: 'https://ok/x.jpg' }, { title: 'x\u0001y\u007f  ', duration_ms: '90000' },
  { title: 'Long', duration: 90000 }, { id: 'dQw4w9WgXcQ', platform: 'soundcloud' }, { toString: 1, title: { a: 1 } }, null, [], 'str', { title: 'T'.repeat(400) },
  { title: 'Bad url', url: 'not a url' }, { title: 'U', url: 'https://www.youtube.com/watch?v=short' }, { title: 'Up', url: 'HTTPS://YOUTU.BE/dQw4w9WgXcQ' },
  { title: 'Emb', url: 'https://music.youtube.com/embed/dQw4w9WgXcQ' }, { title: 'Sp', url: 'https://open.spotify.com/track/x', thumbnail: 'https://i/x' },
];
for (const t of shared) {
  rec('backup', 'clean_shared_track', [t], bc.cleanSharedTrack);
  rec('backup', 'export_track', [t], bc.exportTrack);
  rec('backup', 'clean_stored_track', [t], bc.cleanStoredTrack);
}
const stored = [{ id: 'a', title: 'x', url: 'file:///sdcard/a.mp3', thumbnail: 'data:image/png;base64,AAAA', cover: 'javascript:alert(1)', platform: 'deezer', n: Infinity, ok: true, obj: { a: 1 }, '9bad': 1, toString: 'k', __proto__x: 1 },
  { title: 'p', platform: 'local', isLocal: true, id: 'local_1' }, { id: 0 }, { url: 'http://x/y', localThumbnail: 'https://t' }];
for (const t of stored) { rec('backup', 'clean_stored_track', [t], bc.cleanStoredTrack); rec('backup', 'export_track', [t], bc.exportTrack); }
const files = [
  JSON.stringify({ app: 'Pulse', playlists: [{ name: 'Mix', description: 'd', cover: 'https://c/x.jpg', tracks: shared }, { title: 'Riff list', tracks: [] }, { tracks: 'no' }, 'x'] }),
  JSON.stringify(shared), JSON.stringify([{ name: 'A', tracks: [shared[0]] }, { foo: 1 }]), JSON.stringify({ tracks: shared.slice(0, 3), name: '' }),
  '﻿' + JSON.stringify({ tracks: [shared[1]] }), '', '   ', 'not json', '[1,2,3]', '{"playlists": 5}', JSON.stringify([{ nothing: 1 }]),
];
for (const f of files) rec('backup', 'parse_playlist_file', [f], bc.parsePlaylistFile);
rec('backup', 'playlist_file', [[{ name: 'Mine', tracks: [...shared, ...stored], description: 'desc', cover: 'https://x/y' }, { name: '' }, null, 'x'], '1.3.10'], pinned((p, v) => bc.playlistFile(p, { appVersion: v })));
for (const n of ['My Playlist', 'a/b:c*?', '...hidden', '', 'x'.repeat(80), 'tab\there', 'ok.name']) rec('backup', 'file_name', [n, 'json'], bc.fileName);
const keys = ['pulse_theme', 'devsize_playlists', 'pulse_sync_link', 'pulse_p:1', 'pulse_secret_x', 'pulse_profile', 'pulse_profiles_list', 'pulse_profilesx', 'pulse_debug_log',
  'pulse_custom_css_x', 'pulse_bg_url', 'pulse_bg_urlx', 'pulse_banner_media', 'pulse_lastfm_enabled', 'pulse_lastfm_session', 'pulse_lastfm_enabledx', 'riff_theme', 'pulse_', 'pulse_a b', 'pulse_' + 'x'.repeat(81), 'pulse_device_id'];
for (const k of keys) {
  rec('backup', 'backup_key_allowed', [k], (k) => bc.backupKeyAllowed(k));
  rec('backup', 'backup_key_allowed_device', [k], (k) => bc.backupKeyAllowed(k, (x) => x.includes('device')));
}
rec('backup', 'make_backup', [keys.map((k, i) => [k, i % 3 ? `v${i}` : i]), 'My Profile Name That Is Long', '1.3.10'], pinned((e, n, v) => bc.makeBackup(e, { profileName: n, appVersion: v, isDevice: (x) => x.includes('device') })));
const playlists = [{ id: 'p1', name: 'A', tracks: stored, cover: 'https://c', extra: 'e', num: 3, bad: {}, toString: 'x', constructor: 'y' }, { id: 'p1', name: 'Dup id' }, { name: 'No id', tracks: 'x' }, { id: 'bad id!', name: '' }, 5];
rec('backup', 'clean_stored_playlists', [playlists], pinned(bc.cleanStoredPlaylists));
rec('backup', 'clean_stored_playlists', [{}], pinned(bc.cleanStoredPlaylists));
const backups = [
  { kind: 'pulse-backup', profile: { name: 'Me' }, data: { pulse_theme: 'dark', devsize_playlists: JSON.stringify(playlists), devsize_favorites: JSON.stringify(stored), devsize_history: 'not json', pulse_sync_x: '1', pulse_num: 5, pulse_obj: { a: [1, 2.5, 1e21] }, pulse_null: null, devsize_history2: '[]' } },
  { app: 'Riff', localStorage: { riff_theme: 'light', riff_lastfm_session: 's', devsize_favorites: [{ id: 'x' }], other: '1', pulse_kept: 'y' } },
  { app: 'Other', localStorage: {} }, { kind: 'pulse-backup', data: [] }, null, [], { kind: 'pulse-backup', data: { devsize_playlists: '{"a":1}' }, profile: 'x' },
];
for (const b of backups) rec('backup', 'read_backup', [b], pinned((b) => bc.readBackup(b, { isDevice: (k) => k.includes('device') })));
