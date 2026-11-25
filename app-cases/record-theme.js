// Records app.js generateM3Palette / hslToHex / analyzeCover outputs for crates/pulse-core/tests/theme_parity.rs
const fs = require('fs');
const src = fs.readFileSync(process.argv[2], 'utf8').split('\n');
const take = (a, b) => src.slice(a - 1, b).join('\n');
let tokens = {}; let intensityBg = false; const state = { paletteStyle: 'classic' };
const document = { documentElement: { style: { setProperty: (k, v) => { tokens[k] = v; } } } };
const window = {}; const CSS = undefined;
const localStorage = { getItem: (k) => (k === 'pulse_adv_intensity_bg' ? String(intensityBg) : null) };
eval(take(783, 822) + '\n' + take(834, 1054) + '\n' + 'globalThis.gen = generateM3Palette; globalThis.hsl = hslToHex; globalThis.rgbhsl = rgbToHsl;');
const out = [];
let seed = 7; const rnd = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
const accents = ['#B69DF8', '#7AA7FF', '#56C5C5', '#6DD58C', '#F5B83D', '#F28B82', '#F48FB1', '#6750A4', '#000000', '#ffffff', '#888888', '#abc'];
for (let i = 0; i < 600; i++) accents.push('#' + Math.floor(rnd() * 0xffffff).toString(16).padStart(6, '0'));
const styles = ['classic', 'tonal-spot', 'expressive', 'vibrant', 'neutral'];
for (const a of accents) {
  for (let j = 0; j < 4; j++) {
    const mode = rnd() < 0.5 ? 'dark' : 'light';
    const intensity = [0, 4, 7, 8, 30, 50, 85, 100][Math.floor(rnd() * 8)];
    const brightness = Math.floor(rnd() * 101);
    state.paletteStyle = styles[Math.floor(rnd() * 5)];
    intensityBg = rnd() < 0.3;
    tokens = {};
    gen(a, mode, intensity, brightness);
    const t = {};
    for (const [k, v] of Object.entries(tokens)) if (k.startsWith('--md-sys-color-') && !k.endsWith('-solid')) t[k.slice(15)] = v;
    out.push(JSON.stringify({ f: 'palette', a, mode, intensity, brightness, style: state.paletteStyle, bg: intensityBg, out: t }));
  }
}
for (let i = 0; i < 2000; i++) {
  const h = Math.floor(rnd() * 360), s = Math.floor(rnd() * 101), l = Math.floor(rnd() * 101);
  out.push(JSON.stringify({ f: 'hsl', h, s, l, out: hsl(h, s, l) }));
}
fs.writeFileSync(process.argv[3], out.join('\n') + '\n');
console.log(out.length, 'cases');
