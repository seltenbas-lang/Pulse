package app.pulse;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.drawable.Icon;
import android.media.MediaMetadata;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;
import android.os.Build;
import android.os.IBinder;
import android.os.SystemClock;

import java.io.InputStream;
import java.net.URI;

/**
 * Keeps the music going with the screen off and carries the media notification and session
 * (lock screen, headphones, Bluetooth). The Rust app decides; this only shows and forwards.
 */
public class PlaybackService extends Service {
    static final String CHANNEL = "pulse_playback";
    static final int ID = 1;

    static final class State {
        final String title, artist, artworkUrl;
        final boolean playing;
        final double position, duration;
        State(String title, String artist, String artworkUrl, boolean playing, double position, double duration) {
            this.title = title; this.artist = artist; this.artworkUrl = artworkUrl;
            this.playing = playing; this.position = position; this.duration = duration;
        }
    }

    static State state;
    static MediaSession session;
    static Bitmap art;
    static String artFor = "";
    static PlaybackService running;

    static void publish(Context ctx, State s) {
        state = s;
        Context app = ctx.getApplicationContext();
        ensureSession(app);
        AudioFocus.onState(app, s.playing);
        updateSession(s);
        if (!s.artworkUrl.equals(artFor)) loadArt(app, s.artworkUrl);
        if (running != null) running.show();
        else if (s.playing) {
            Intent i = new Intent(app, PlaybackService.class);
            app.startForegroundService(i);
        }
    }

    static boolean isPlaying() {
        State s = state;
        return s != null && s.playing;
    }

    static void ensureSession(Context app) {
        if (session != null) return;
        session = new MediaSession(app, "Pulse");
        session.setCallback(new MediaSession.Callback() {
            @Override public void onPlay() { MediaBridge.nativeCommand("play"); }
            @Override public void onPause() { MediaBridge.nativeCommand("pause"); }
            @Override public void onStop() { MediaBridge.nativeCommand("pause"); }
            @Override public void onSkipToNext() { MediaBridge.nativeCommand("next"); }
            @Override public void onSkipToPrevious() { MediaBridge.nativeCommand("prev"); }
            @Override public void onSeekTo(long pos) { MediaBridge.nativeCommand("seek:" + (pos / 1000.0)); }
        });
        session.setActive(true);
    }

    static void updateSession(State s) {
        MediaMetadata.Builder m = new MediaMetadata.Builder()
            .putString(MediaMetadata.METADATA_KEY_TITLE, s.title)
            .putString(MediaMetadata.METADATA_KEY_ARTIST, s.artist)
            .putLong(MediaMetadata.METADATA_KEY_DURATION, (long) (s.duration * 1000));
        if (art != null) m.putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, art);
        session.setMetadata(m.build());
        session.setPlaybackState(new PlaybackState.Builder()
            .setActions(PlaybackState.ACTION_PLAY | PlaybackState.ACTION_PAUSE | PlaybackState.ACTION_PLAY_PAUSE
                | PlaybackState.ACTION_SKIP_TO_NEXT | PlaybackState.ACTION_SKIP_TO_PREVIOUS | PlaybackState.ACTION_SEEK_TO)
            .setState(s.playing ? PlaybackState.STATE_PLAYING : PlaybackState.STATE_PAUSED, (long) (s.position * 1000), s.playing ? 1f : 0f, SystemClock.elapsedRealtime())
            .build());
    }

    static void loadArt(Context app, String url) {
        artFor = url;
        if (url == null || !url.startsWith("https://")) { art = null; return; }
        new Thread(() -> {
            try (InputStream in = URI.create(url).toURL().openStream()) {
                Bitmap b = BitmapFactory.decodeStream(in);
                if (url.equals(artFor)) {
                    art = b;
                    State s = state;
                    if (s != null && session != null) { updateSession(s); if (running != null) running.show(); }
                }
            } catch (Exception ignored) {}
        }).start();
    }

    @Override public void onCreate() {
        super.onCreate();
        running = this;
        NotificationManager nm = getSystemService(NotificationManager.class);
        {
            NotificationChannel ch = new NotificationChannel(CHANNEL, "Playback", NotificationManager.IMPORTANCE_LOW);
            ch.setShowBadge(false);
            nm.createNotificationChannel(ch);
        }
    }

    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        // a notification button: hand it to the app (the service is already in the foreground)
        if (intent != null && intent.getAction() != null) {
            MediaBridge.nativeCommand(intent.getAction());
            return START_NOT_STICKY;
        }
        Notification n = build();
        if (Build.VERSION.SDK_INT >= 29) startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK);
        else startForeground(ID, n);
        return START_NOT_STICKY;
    }

    void show() {
        getSystemService(NotificationManager.class).notify(ID, build());
        State s = state;
        // paused: the notification may be swiped away, the service stops being foreground
        if (s != null && !s.playing && Build.VERSION.SDK_INT >= 24) stopForeground(STOP_FOREGROUND_DETACH);
        else if (s != null && s.playing) {
            if (Build.VERSION.SDK_INT >= 29) startForeground(ID, build(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK);
            else startForeground(ID, build());
        }
    }

    PendingIntent action(String cmd, int code) {
        Intent i = new Intent(this, PlaybackService.class).setAction(cmd);
        return PendingIntent.getService(this, code, i, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    Notification build() {
        State s = state != null ? state : new State("Pulse", "", "", false, 0, 0);
        Notification.Builder b = new Notification.Builder(this, CHANNEL);
        Intent open = new Intent(this, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        b.setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle(s.title)
            .setContentText(s.artist)
            .setLargeIcon(art)
            .setOngoing(s.playing)
            .setShowWhen(false)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setContentIntent(PendingIntent.getActivity(this, 0, open, PendingIntent.FLAG_IMMUTABLE))
            .addAction(new Notification.Action.Builder(Icon.createWithResource(this, android.R.drawable.ic_media_previous), "Previous", action("prev", 1)).build())
            .addAction(new Notification.Action.Builder(Icon.createWithResource(this, s.playing ? android.R.drawable.ic_media_pause : android.R.drawable.ic_media_play), s.playing ? "Pause" : "Play", action("toggle", 2)).build())
            .addAction(new Notification.Action.Builder(Icon.createWithResource(this, android.R.drawable.ic_media_next), "Next", action("next", 3)).build())
            .setStyle(new Notification.MediaStyle().setMediaSession(session != null ? session.getSessionToken() : null).setShowActionsInCompactView(0, 1, 2));
        return b.build();
    }

    @Override public void onDestroy() { running = null; super.onDestroy(); }
    @Override public IBinder onBind(Intent intent) { return null; }
}
