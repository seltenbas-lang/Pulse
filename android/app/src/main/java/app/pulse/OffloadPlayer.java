package app.pulse;

import android.media.AudioAttributes;
import android.media.MediaPlayer;
import android.os.Build;

/**
 * Plays the downloaded song with Android's own player while the screen is off, so that the phone
 * can decode it on its low-power audio processor (compressed offload) and let the CPU sleep. The
 * Rust app decides when; this only plays, reports the end, and hands the position back.
 */
public final class OffloadPlayer {
    private static MediaPlayer mp;

    private OffloadPlayer() {}

    /** Starts `path` at `startMs`; false when Android can't play it (the app keeps its own engine). */
    public static synchronized boolean play(String path, int startMs, float volume) {
        release();
        try {
            MediaPlayer p = new MediaPlayer();
            p.setAudioAttributes(new AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                .build());
            p.setDataSource(path);
            p.prepare();
            if (startMs > 0) {
                if (Build.VERSION.SDK_INT >= 26) p.seekTo(startMs, MediaPlayer.SEEK_CLOSEST);
                else p.seekTo(startMs);
            }
            p.setVolume(volume, volume);
            p.setOnCompletionListener(x -> notifyApp("offload-ended"));
            p.setOnErrorListener((x, what, extra) -> { notifyApp("offload-error"); return true; });
            p.start();
            mp = p;
            return true;
        } catch (Throwable t) {
            android.util.Log.e("pulse", "offload play failed", t);
            release();
            return false;
        }
    }

    public static synchronized void pause() {
        try { if (mp != null && mp.isPlaying()) mp.pause(); } catch (Throwable ignored) {}
    }

    public static synchronized void resume() {
        try { if (mp != null) mp.start(); } catch (Throwable ignored) {}
    }

    public static synchronized void seek(int ms) {
        try {
            if (mp == null) return;
            if (Build.VERSION.SDK_INT >= 26) mp.seekTo(ms, MediaPlayer.SEEK_CLOSEST);
            else mp.seekTo(ms);
        } catch (Throwable ignored) {}
    }

    public static synchronized boolean isPlaying() {
        try { return mp != null && mp.isPlaying(); } catch (Throwable t) { return false; }
    }

    /** Milliseconds into the song, or -1 when nothing is loaded. */
    public static synchronized int position() {
        try { return mp != null ? mp.getCurrentPosition() : -1; } catch (Throwable t) { return -1; }
    }

    public static synchronized void release() {
        if (mp == null) return;
        try { mp.release(); } catch (Throwable ignored) {}
        mp = null;
    }

    private static void notifyApp(String cmd) {
        try { MediaBridge.nativeCommand(cmd); } catch (Throwable ignored) {}
    }
}
