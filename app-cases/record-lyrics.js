const { rec, strings, src } = require('./record');
const lm = src('lyrics-match.js');
for (const s of strings) {
  rec('lyrics_match', 'norm', [s], lm.norm);
  rec('lyrics_match', 'clean_title', [s, false], lm.cleanTitle);
  rec('lyrics_match', 'clean_title', [s, true], lm.cleanTitle);
  rec('lyrics_match', 'clean_artist', [s], lm.cleanArtist);
  rec('lyrics_match', 'split_artists', [s], lm.splitArtists);
}
const pairs = [
  ['Coldplay - Yellow (Official Video)', 'ColdplayVEVO'], ['Yellow', 'Coldplay - Topic'], ['Blinding Lights', 'TheWeekndVEVO'],
  ['The Weeknd - Blinding Lights (Official Audio)', 'The Weeknd'], ['Song - Remastered 2011', 'Some Uploader'],
  ['Daft Punk - Get Lucky ft. Pharrell Williams', 'Daft Punk'], ['Get Lucky', 'Daft Punk & Pharrell Williams'],
  ['Artist - Song | Live at Wembley', 'Random Channel'], ['夜に駆ける', 'YOASOBI'], ['أهواك', 'عبد الحليم حافظ'],
  ['Кино - Группа крови', 'Кино'], ['Hello', ''], ['', 'Adele'], ['A - B - C', 'A'], ['Song (feat. X) [Lyrics]', 'Records Inc'],
  ["Boy's a liar Pt. 2", 'PinkPantheress, Ice Spice'], ['Title — Other', 'Music Channel'], ['  -  ', ' '],
];
for (const s of strings.slice(0, 60)) pairs.push([s, strings[(strings.indexOf(s) * 7) % strings.length]]);
for (const [t, a] of pairs) rec('lyrics_match', 'parse_for_lyrics', [t, a], lm.parseForLyrics);
const items = [
  { trackName: 'Yellow', artistName: 'Coldplay', duration: 266, syncedLyrics: '[00:01]x', plainLyrics: 'x' },
  { trackName: 'Yellow (Live)', artistName: 'Coldplay', duration: 300, plainLyrics: 'x' },
  { trackName: 'Yellow', artistName: 'Other', duration: 267, plainLyrics: 'x' },
  { trackName: 'Yellow', artistName: 'Coldplay', instrumental: true, plainLyrics: '' },
  { name: 'Yellow', artistName: 'Coldplay', syncedLyrics: 'y' }, null, { trackName: 'Blue', artistName: 'Coldplay', plainLyrics: 'z' },
];
for (const want of [lm.parseForLyrics('Coldplay - Yellow (Official Video)', 'ColdplayVEVO'), lm.parseForLyrics('Yellow', 'Nobody'), { titles: [], artists: [] }])
  for (const d of [0, 266, 268, 280, -1]) for (const list of [items, items.slice(1), items.slice(2), []])
    rec('lyrics_match', 'pick_best', [list, want, d], lm.pickBest);
