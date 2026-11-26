package app.pulse;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;

/**
 * Audio focus, the way Android's music apps take turns: Pulse asks for it when a song plays.
 * Another app playing (TikTok, a video, another player) pauses Pulse; a short interruption (a call,
 * a voice message) pauses it and it carries on afterwards; a navigation prompt only lowers it for
 * a moment (Android does that by itself). Headphones unplugged or Bluetooth gone: pause.
 */
final class AudioFocus {
    private static AudioManager am;
    private static AudioFocusRequest request;
    private static boolean held;
    /** paused for an interruption: play again when it is over (a ducking one only if short) */
    private static boolean resumeOnGain;
    private static long lostAt;
    private static boolean lostToDuck;
    /** the interruption was a phone or voice call (after one, the music carries on) */
    private static boolean lostToCall;
    /** the last published state: focus is asked for when playback starts, not on every update
     *  (asking again right after losing it took the sound back from the other app) */
    private static boolean wasPlaying;
    private static boolean noisyRegistered;

    private AudioFocus() {}

    private static final AudioManager.OnAudioFocusChangeListener LISTENER = change -> {
        switch (change) {
            case AudioManager.AUDIOFOCUS_LOSS:
                // another app took over for good: pause, and wait for the person to press play
                held = false;
                resumeOnGain = false;
                report("focus:loss");
                MediaBridge.nativeCommand("pause");
                break;
            case AudioManager.AUDIOFOCUS_LOSS_TRANSIENT:
            case AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK:
                // a call, a voice message, a video app, a navigation prompt: pause (lowering the
                // volume left the music running unheard)
                lostToDuck = change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK;
                lostAt = android.os.SystemClock.elapsedRealtime();
                lostToCall = inCall();
                resumeOnGain = PlaybackService.isPlaying();
                report((lostToDuck ? "focus:duck" : "focus:transient") + (lostToCall ? " (call)" : ""));
                MediaBridge.nativeCommand("pause");
                break;
            case AudioManager.AUDIOFOCUS_GAIN:
                held = true;
                long away = android.os.SystemClock.elapsedRealtime() - lostAt;
                // carry on by itself only after a call, or after something very short (a
                // notification sound, a navigation prompt). Video apps (TikTok, Reels, Shorts) take
                // the sound for each video and hand it back between them: resuming then made the
                // music come back while scrolling, so after those it waits for play.
                boolean resume = resumeOnGain && (lostToCall || inCall() || away < 5_000);
                resumeOnGain = false;
                report("focus:gain after " + (away / 1000) + " s" + (resume ? " (resuming)" : " (staying paused)"));
                if (resume) MediaBridge.nativeCommand("play");
                break;
            default:
                break;
        }
    };

    /** a phone call, or a voice/video call in an app (WhatsApp, Teams…) */
    private static boolean inCall() {
        try {
            int mode = am != null ? am.getMode() : AudioManager.MODE_NORMAL;
            return mode == AudioManager.MODE_IN_CALL || mode == AudioManager.MODE_IN_COMMUNICATION
                || mode == AudioManager.MODE_RINGTONE;
        } catch (Throwable t) {
            return false;
        }
    }

    private static void report(String what) {
        try { MediaBridge.nativeCommand(what); } catch (Throwable ignored) {}
    }

    private static final BroadcastReceiver NOISY = new BroadcastReceiver() {
        @Override public void onReceive(Context c, Intent i) {
            if (AudioManager.ACTION_AUDIO_BECOMING_NOISY.equals(i.getAction()) && PlaybackService.isPlaying()) {
                MediaBridge.nativeCommand("pause");
            }
        }
    };

    /** Called whenever the playing state is published. */
    static synchronized void onState(Context app, boolean playing) {
        try {
            if (am == null) {
                am = app.getSystemService(AudioManager.class);
                request = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                    .setAudioAttributes(new AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                        .build())
                    .setWillPauseWhenDucked(true)
                    .setOnAudioFocusChangeListener(LISTENER)
                    .build();
            }
            if (!noisyRegistered) {
                IntentFilter f = new IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY);
                // Android 13+ wants to know; a system broadcast still arrives with NOT_EXPORTED
                if (android.os.Build.VERSION.SDK_INT >= 33) app.registerReceiver(NOISY, f, Context.RECEIVER_NOT_EXPORTED);
                else app.registerReceiver(NOISY, f);
                noisyRegistered = true;
            }
            boolean started = playing && !wasPlaying;
            wasPlaying = playing;
            if (started) {
                // the person pressed play (again): their choice wins over a pending resume
                resumeOnGain = false;
                if (!held) {
                    held = am.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
                    report(held ? "focus:granted" : "focus:refused");
                }
            }
        } catch (Throwable t) {
            android.util.Log.e("pulse", "audio focus", t);
        }
    }
}
