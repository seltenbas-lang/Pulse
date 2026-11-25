const { rec, strings, src } = require('./record');
const lp = src('library-pages.js');
const rc = src('recommend-core.js');
for (const s of strings) {
  rec('library_pages', 'name_key', [s], lp.nameKey);
  rec('library_pages', 'is_explicit_text', [s], lp.isExplicitText);
  rec('recommend', 'parse_length', [s], rc.parseLength);
  rec('recommend', 'clean_video_title', [s, strings[(strings.indexOf(s) * 3 + 1) % strings.length]], rc.cleanVideoTitle);
}
for (const s of ['3:45', '1:02:03', '0:5', '12', '1:2:3:4', ' 4:00 ', '٣:٤٥', '10:61']) rec('recommend', 'parse_length', [s], rc.parseLength);
const vt = [['Radiohead - No Surprises (Official Video)', 'RadioheadVEVO'], ['Pixies - Pixies - Where Is My Mind?', 'Pixies'], ['In The End - Linkin Park', 'Linkin Park - Topic'],
  ['Oasis - Wonderwall', 'Random Uploader'], ['Song - ', 'X'], ['Title [4K Remaster] (Lyrics)', 'Artist Official'], ['', ''], ['A - B - C', 'Z']];
for (const [t, a] of vt) rec('recommend', 'clean_video_title', [t, a], rc.cleanVideoTitle);
const albums = [{ id: 1, title: 'OK Computer', artist: 'Radiohead' }, { id: '1', title: 'x' }, { id: 'a2', title: 'ok computer', artist: 'RADIOHEAD - Topic', _tracks: [1, 2, 3], type: 'EP', year: 1997, source: 'itunes' },
  { title: 'Kid A', artist: 'Radiohead', trackCount: '10', cover: 'c' }, { title: '', artist: 'x' }, null];
for (const a of albums) for (const b of albums) rec('library_pages', 'same_album', [a, b], lp.sameAlbum);
for (const a of albums.filter(Boolean)) {
  rec('library_pages', 'album_entry', [a, 1000], lp.albumEntry);
  rec('library_pages', 'toggle_saved_album', [albums.slice(0, 2), a, 1000], lp.toggleSavedAlbum);
}
const followed = [{ name: 'Queen', id: 'UC1' }, { name: 'Muse' }, { name: 'muse', id: 'UC9' }];
const pages = [{ name: 'Queen', id: 'UC1' }, { name: 'Queen', id: 'UC2' }, { name: 'Queen' }, { name: 'MUSE', ids: ['UC9'], avatar: 'p', fans: '12' }, { name: '  ' }, { name: 'New', id: 'UC5', avatar: 'a', fans: 3 }, { name: 'Muse', fans: 0, avatar: '' }];
for (const p of pages) {
  for (const e of followed) rec('library_pages', 'follow_matches', [e, p], lp.followMatches);
  rec('library_pages', 'toggle_follow_artist', [followed, p, 77], lp.toggleFollowArtist);
  rec('library_pages', 'refresh_followed', [followed, p], lp.refreshFollowed);
}
const seeds = [];
const names = ['Queen', 'Muse', 'ABBA', 'Björk', 'björk', 'Kino', 'Coldplay', 'Adele', 'Oasis', 'Blur', 'Pulp'];
names.forEach((n, i) => { for (let j = 0; j < 3; j++) seeds.push({ id: `${i}-${j}`, title: `Song ${j}`, artist: n, weight: (i * 7 + j * 3) % 10 }); });
seeds.push({ title: 'x' }, null, { artist: '', title: 'y' });
for (const n of [0, 3, 4, 8, 12, seeds.length]) for (const max of [5, 2]) rec('library_pages', 'plan_mixes', [seeds.slice(0, n), '2026-11-21', max], (s, day, max) => lp.planMixes(s, { day, max }));
const fbs = [undefined, null, [], { likedArtists: ['Queen', ' ', 3], likedTracks: [{ id: 'a', key: 'k' }, 'x'], ratingsByMix: { m1: 'like' } }, { ratingsByMix: [] }];
const mixes = [{ id: 'm2', artists: ['Queen', 'Muse'], tracks: [{ id: 'a', title: 'Bohemian', artist: 'Queen' }, { title: 'Uprising', artist: 'Muse' }, { title: 'Hey', artist: 'Pixies' }, { id: 'z' }] }, { id: 'm1', artists: [], tracks: [{ id: 'q', title: 'T' }] }, {}, null];
for (const f of fbs) {
  rec('library_pages', 'clean_feedback', [f], lp.cleanFeedback);
  rec('library_pages', 'feedback_avoid', [f], lp.feedbackAvoid);
  for (const id of ['m1', 'm2', '']) rec('library_pages', 'is_mix_rated', [f, id], lp.isMixRated);
  for (const m of mixes) for (const like of [true, false]) {
    rec('library_pages', 'rate_mix', [f, m, like], lp.rateMix);
    rec('library_pages', 'feedback_avoid', [lp.rateMix(f, m, like)], lp.feedbackAvoid);
  }
}
const many = { ratingsByMix: Object.fromEntries(Array.from({ length: 61 }, (_, i) => [`mix-${i}`, 'like'])) };
rec('library_pages', 'rate_mix', [many, { id: 'mix-3', tracks: [] }, false], lp.rateMix);
rec('library_pages', 'rate_mix', [many, { id: 'new', tracks: [] }, true], lp.rateMix);
const tracks = [{ title: 'Song (Explicit)' }, { title: 'Song [E]' }, { title: 'x', album: 'Album (Dirty)' }, { title: 'Песня без цензуры' }, { title: 'x', explicit: 1 },
  { title: 'x', explicit_content_lyrics: 1 }, { title: 'x', isExplicit: true }, { title: 'Clean' }, null, { title: 'Song [18+]' }, { title: 'inexplicit' }];
for (const t of tracks) rec('library_pages', 'is_track_explicit', [t], lp.isTrackExplicit);
const blocks = [{ artists: ['Queen', 'muse - Topic'], songs: [{ id: 'v1', title: 'x' }, { title: 'Yellow', artist: '' }, { title: 'Creep', artist: 'Radiohead' }] }, { artists: [], songs: [] }, null];
const cands = [{ id: 'v1', title: 'whatever' }, { title: 'Under Pressure', artist: 'Queen & David Bowie' }, { title: 'Yellow', artist: 'Coldplay' }, { title: 'Creep', artist: 'Radiohead' },
  { title: 'Creep', artist: 'TLC' }, { title: 'Hysteria', artist: 'MUSE' }, { title: 'x', artist: 'Queens of the Stone Age' }, null, { title: 'A', artist: 'Queen x Muse' }];
for (const b of blocks) for (const t of cands) rec('library_pages', 'is_track_blocked', [b, t], lp.isTrackBlocked);
for (const a of ['Queen', 'queen', '  ', null, 'x'.repeat(250), 'New']) rec('library_pages', 'toggle_blocked_artist', [['Queen', 'Muse'], a], lp.toggleBlockedArtist);
for (const t of cands) rec('library_pages', 'toggle_blocked_song', [blocks[0].songs, t], lp.toggleBlockedSong);
const run = (text, extra = {}) => ({ text, ...extra });
const item = (id, title, byline, len, extra = {}) => ({ playlistPanelVideoRenderer: { videoId: id, title: { runs: [run(title)] }, longBylineText: { runs: byline }, lengthText: { runs: [run(len)] }, ...extra } });
const watch = { contents: { singleColumnMusicWatchNextResultsRenderer: { tabbedRenderer: { watchNextTabbedResultsRenderer: { tabs: [{ tabRenderer: { content: { musicQueueRenderer: { content: { playlistPanelRenderer: { contents: [
  item('seedseedsee', 'Seed', [run('A')], '3:00'),
  item('aaaaaaaaaaa', 'No Surprises', [run('Radiohead', { navigationEndpoint: { browseEndpoint: { browseId: 'UCq19-LqvG35A-30oyAiPiqA' } } }), run(' • '), run('OK Computer')], '3:49', { thumbnail: { thumbnails: [{ url: 's' }, { url: 'big' }] } }),
  item('bbbbbbbbbbb', 'Oasis - Wonderwall', [run('Uploader', { navigationEndpoint: { browseEndpoint: { browseId: 'UCq19-LqvG35A-30oyAiPiqB' } } })], '4:18'),
  item('ccccccccccc', 'Long Mix', [run('DJ')], '1:02:00'), item('ddddddddddd', 'Short', [run('X')], '0:10'),
  item('eeeeeeeeeee', 'Band Interview', [run('X')], '5:00'), item('aaaaaaaaaaa', 'Dup', [run('Y')], '3:00'), item('bad', 'Bad id', [run('Y')], '3:00'),
  { playlistPanelVideoWrapperRenderer: { primaryRenderer: { playlistPanelVideoRenderer: { videoId: 'fffffffffff', title: { runs: [run('Wrapped')] }, shortBylineText: { runs: [run('Kino - Topic')] } } } } },
  item('ggggggggggg', '', [run('X')], '3:00'), null,
] } } } } } }] } } } } };
for (const seed of ['seedseedsee', 'aaaaaaaaaaa', '']) rec('recommend', 'parse_watch_playlist', [watch, seed], rc.parseWatchPlaylist);
rec('recommend', 'parse_watch_playlist', [{}, 'x'], rc.parseWatchPlaylist);
rec('recommend', 'parse_watch_playlist', [null, 'x'], rc.parseWatchPlaylist);
const sim = [{ similartracks: { track: [{ name: ' Song ', artist: { name: 'A' }, match: '0.5', duration: '200' }, { name: 'B', artist: { '#text': 'C' }, match: 3 }, { name: '', artist: { name: 'x' } }, { name: 'n', match: -1 }] } },
  { similartracks: { track: { name: 'One', artist: { name: 'Solo' } } } }, {}, null];
for (const j of sim) rec('recommend', 'parse_similar_tracks', [j], rc.parseSimilarTracks);
for (const j of [{ similarartists: { artist: [{ name: 'X', match: '0.9' }, { name: '' }, { name: 'Y', match: 'abc' }] } }, { similarartists: { artist: { name: 'Z' } } }, {}])
  rec('recommend', 'parse_similar_artists', [j], rc.parseSimilarArtists);
for (const j of [{ toptracks: { track: [{ name: 'T', artist: { name: 'A' } }, { name: 'U' }, { name: '' }] } }, { toptracks: { track: { name: 'Only' } } }])
  for (const fb of ['', 'Fallback']) rec('recommend', 'parse_top_tracks', [j, fb], rc.parseTopTracks);
const pool = [{ id: 'v1', title: 'whatever' }, { id: 'v2', title: 'Song', artist: 'A' }, { id: 'v3', title: 'song', artist: 'a' }, { title: 'Under Pressure', artist: 'Queen & David Bowie' }, { id: 'v2', title: 'Other' }, { title: '' }, null, { id: 9, title: 'Nine' }];
for (const opts of [{}, { blocks: blocks[0] }, { recentKeys: ['a::other'], recentIds: ['9'] }, { blocks: blocks[0], recentIds: ['v2'] }])
  rec('recommend', 'drop_unwanted', [pool, opts], rc.dropUnwanted);
