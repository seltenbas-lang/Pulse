//! now-playing.js: what the system media controls are told, and when to resume after an interruption.

/// Milliseconds after the last sound in which a pause still counts as the interruption's.
pub const INTERRUPT_GRACE_MS: f64 = 3000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionState {
    pub duration: f64,
    pub position: f64,
    pub playback_rate: f64,
}

fn positive(x: f64) -> Option<f64> {
    (x.is_finite() && x > 0.0).then_some(x)
}

/// Media-session position for a player at `current_time` of `duration` (falls back to
/// `fallback_duration`); `None` while the length is unknown.
pub fn position_state(
    duration: f64,
    current_time: f64,
    playback_rate: f64,
    fallback_duration: f64,
) -> Option<PositionState> {
    let duration = positive(duration).or(positive(fallback_duration))?;
    let position = positive(current_time).unwrap_or(0.0).min(duration);
    Some(PositionState {
        duration,
        position,
        playback_rate: positive(playback_rate).unwrap_or(1.0),
    })
}

pub fn was_playing_at_interruption(
    paused: bool,
    ended: bool,
    paused_by_user: bool,
    last_playing_at: f64,
    now: f64,
) -> bool {
    if paused_by_user || ended {
        return false;
    }
    if !paused {
        return true;
    }
    last_playing_at > 0.0 && now - last_playing_at <= INTERRUPT_GRACE_MS
}

pub fn resume_after_interruption(
    was_playing: bool,
    should_resume: bool,
    paused_by_user: bool,
    paused: bool,
    ended: bool,
    has_track: bool,
) -> bool {
    was_playing && should_resume && !paused_by_user && paused && !ended && has_track
}
