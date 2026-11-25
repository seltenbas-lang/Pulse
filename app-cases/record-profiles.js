const { rec, src } = require('./record');
const pc = src('profiles-core.js');
const keys = ['pulse_theme', 'pulse_profiles', 'pulse_profile_active', 'pulse_fps_cap', 'pulse_secret_x', 'pulse_audio_analysis_abc', 'devsize_history',
  'pulse_p:p2:pulse_theme', 'pulse_p:p2:pulse_fps_cap', 'pulse_p:p3:x', 'pulse_p:', 'pulse_p:default:x', 'other'];
for (const id of ['default', 'p2', 'p3']) for (const k of keys) {
  rec('profiles', 'raw_key', [id, k], pc.rawKey);
  rec('profiles', 'own_key', [id, k], pc.ownKey);
}
for (const k of keys) rec('profiles', 'is_device', [k], pc.isDevice);
