const { rec, strings, src } = require('./record');
const tm = src('track-match.js');
const ts = src('track-sort.js');
for (const s of strings) {
  rec('track_match', 'match_norm', [s], tm.matchNorm);
  rec('track_match', 'split_artists', [s], tm.splitArtists);
  rec('track_match', 'clean_title', [s, []], tm.cleanTitle);
  rec('track_match', 'clean_title', [s, tm.splitArtists(s.split(' - ')[0])], tm.cleanTitle);
}
for (let i = 0; i < strings.length; i++) rec('track_match', 'title_similarity', [strings[i], strings[(i * 5 + 3) % strings.length]], tm.titleSimilarity);
const songs = [
  { title: 'Yellow', artist: 'Coldplay', duration: 266 }, { title: 'Get Lucky (feat. Pharrell Williams)', artist: 'Daft Punk, Pharrell Williams', duration: 248 },
  { title: 'Группа крови', artist: 'Кино', duration: 285 }, { title: 'Blinding Lights - Remastered 2020', artist: 'The Weeknd', duration: 200 },
  { title: 'Song (Remix)', artist: 'Someone', duration: 180 }, { title: 'Hello', artist: '', duration: 0 }, { title: 'Live Forever', artist: 'Oasis' },
];
const results = [
  { title: 'Coldplay - Yellow (Official Video)', artist: 'ColdplayVEVO', duration: 269 }, { title: 'Yellow (Live)', artist: 'Coldplay', duration: 300 },
  { title: 'Yellow', artist: 'Coldplay - Topic', duration: 266 }, { title: 'Yellow Cover', artist: 'Random', duration: 250 },
  { title: 'Daft Punk - Get Lucky ft. Pharrell Williams', artist: 'Daft Punk', duration: 250 }, { title: 'Get Lucky (Sped Up)', artist: 'nightcore guy', duration: 200 },
  { title: 'Kino - Gruppa krovi', artist: 'Kino Official', duration: 287 }, { title: 'The Weeknd - Blinding Lights', uploader: 'Some Reuploader', duration: 201 },
  { title: 'Song (Remix)', artist: 'Someone', duration: 181 }, { title: 'Song', artist: 'Someone', duration: 180 }, { title: 'Hello', artist: 'Adele', duration: 295 },
  { title: 'Live Forever (Remastered)', artist: 'OasisVEVO' }, { title: '', artist: 'x' },
];
for (const sp of songs) {
  for (const r of results) rec('track_match', 'score_candidate', [sp, r], tm.scoreCandidate);
  rec('track_match', 'pick_best_match', [sp, results], tm.pickBestMatch);
  rec('track_match', 'match_queries', [sp], tm.matchQueries);
}
const tracks = [
  { id: 'a', title: 'Zebra', artist: 'Beta', album: 'B', disc: 1, no: 2, duration: 200, addedAt: 5 },
  { id: 'b', title: 'apple', artist: 'alpha', album: 'B', disc: 1, no: 1, duration: 100, addedAt: 3 },
  { id: 'c', title: 'Äpfel', artist: 'Ålpha', album: 'A', duration: 0, addedAt: 9 },
  { id: 'd', title: 'Track 10', artist: '', album: '', duration: 300 }, { id: 'e', title: 'Track 2', artist: 'Ω', duration: 'x', addedAt: 1 },
  { id: 'f', title: '  ', artist: 'Gamma', album: 'A', no: 3, addedAt: 2 }, { id: 'g', title: 'track 2', album: 'A', no: 1, addedAt: 4 },
  { id: 'h', title: 'Ёлка', artist: 'Kino', album: 'B', disc: 2, no: 1, addedAt: 7 }, { id: 'i', title: '日本', artist: 'Z', addedAt: 6 },
  { id: 'j', title: 'éclair', artist: 'eve', duration: 150, addedAt: 8 },
];
for (const mode of [...ts.TRACK_SORT_MODES, 'bogus']) rec('track_sort', 'sort_tracks', [tracks, mode], ts.sortTracks);
const albums = [{ title: 'B', artist: 'x', year: '1999' }, { title: 'a', artist: 'y', year: 2005 }, { title: 'C', year: 'abc' }, { title: 'a', artist: 'b', year: '2005-01' }, null, { title: '', artist: 'z' }];
const playlists = [{ name: 'Rock', tracks: [1, 2] }, { name: 'jazz', tracks: [] }, { name: 'Pop 10', tracks: [1] }, { name: 'Pop 9', tracks: [1, 2] }, { name: '' }];
const artists = [{ name: 'Beta', plays: 3 }, { name: 'alpha', plays: 3 }, { name: 'Gamma' }, { name: 'delta', plays: 10 }];
for (const [kind, items] of [['albums', albums], ['playlists', playlists], ['artists', artists], ['other', albums]])
  for (const mode of ['custom', 'title-asc', 'artist-asc', 'year-desc', 'name-asc', 'name-desc', 'size-desc', 'plays-desc', 'nope']) {
    rec('track_sort', 'sort_library', [kind, items, mode], (k, it, m) => ts.sortLibrary(k, it, m));
    rec('track_sort', 'sort_library_plays_field', [kind, items, mode], (k, it, m) => ts.sortLibrary(k, it, m, { plays: (x) => x.plays }));
    rec('track_sort', 'library_sort_mode', [kind, mode], ts.librarySortMode);
  }
const favs = [
  { id: 'dQw4w9WgXcQ', title: 'Never Gonna Give You Up', artist: 'Rick Astley', duration: 213 },
  { id: 'yt:dQw4w9WgXcQ', platform: 'youtube', title: 'Never Gonna Give You Up (Official Video)', artist: 'Rick Astley - Topic' },
  { url: 'https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=x', platform: 'ytmusic', title: 'x' },
  { id: 'alb_1_3', title: 'Never Gonna Give You Up', artist: 'Rick Astley', duration: 214 },
  { id: 'sc:123', platform: 'soundcloud', title: 'Song', artist: 'A' }, { id: 123, platform: 'SoundCloud', title: 'Song', artist: 'A', duration: 400 },
  { title: 'Song', artist: 'A feat. B', duration: 199.5 }, { title: 'Song', artist: 'A', duration: -0.4 }, { title: 'Song', artist: 'A', platform: 'local' },
  { url: 'https://youtu.be/abcdefghijk', title: 'Other' }, { title: '', artist: '' }, null, { id: 'x', platform: 'spotify', title: 'Song', artist: 'A', duration: 190 },
];
for (const f of favs) rec('track_sort', 'favorite_track_key', [f], ts.favoriteTrackKey);
for (const a of favs) for (const b of favs) rec('track_sort', 'is_same_song', [a, b], ts.isSameSong);
rec('track_sort', 'dedupe_favorite_tracks', [favs], ts.dedupeFavoriteTracks);
rec('track_sort', 'dedupe_favorite_tracks', [favs.slice().reverse()], ts.dedupeFavoriteTracks);
rec('track_sort', 'backfill_added_at_fixed', [tracks, true], (t, n) => { const real = Date.now; Date.now = () => 1e12; try { return ts.backfillAddedAt(t, n); } finally { Date.now = real; } });
rec('track_sort', 'backfill_added_at_fixed', [tracks, false], (t, n) => { const real = Date.now; Date.now = () => 1e12; try { return ts.backfillAddedAt(t, n); } finally { Date.now = real; } });
