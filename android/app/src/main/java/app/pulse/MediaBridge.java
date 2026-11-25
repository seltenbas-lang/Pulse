package app.pulse;

import android.app.Activity;
import android.content.ContentValues;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.provider.MediaStore;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/** What the Rust side calls (static methods), and the two native callbacks into it. */
public final class MediaBridge {
    static Activity activity;

    static native void nativeCommand(String cmd);
    static native void nativeBackupPicked(byte[] data);

    public static void init(Activity a) { activity = a; }

    /** What plays now: starts or updates the foreground service and its media notification. */
    public static void update(String title, String artist, String artworkUrl, boolean playing, double position, double duration) {
        Activity a = activity;
        if (a == null) return;
        PlaybackService.State s = new PlaybackService.State(title, artist, artworkUrl, playing, position, duration);
        a.runOnUiThread(() -> PlaybackService.publish(a, s));
    }

    /** Opens the system file picker for a Pulse backup or Riff account file. */
    public static void pickBackup() { pickFile("*/*"); }

    /** Settings > Theme > Background: a picture from the phone (the same answer path as a backup). */
    public static void pickImage() { pickFile("image/*"); }

    private static void pickFile(String type) {
        Activity a = activity;
        if (a == null) return;
        a.runOnUiThread(() -> {
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            i.setType(type);
            a.startActivityForResult(i, MainActivity.PICK_BACKUP);
        });
    }

    static void backupPicked(Activity a, Uri uri) {
        new Thread(() -> {
            try (InputStream in = a.getContentResolver().openInputStream(uri)) {
                ByteArrayOutputStream out = new ByteArrayOutputStream();
                byte[] buf = new byte[65536];
                int n;
                long total = 0;
                while ((n = in.read(buf)) > 0) {
                    total += n;
                    if (total > 30_000_000L) return; // backup-core.js LIMITS.fileBytes
                    out.write(buf, 0, n);
                }
                nativeBackupPicked(out.toByteArray());
            } catch (Throwable t) {
                // an Error here (UnsatisfiedLinkError, OutOfMemoryError) used to close the whole app
                android.util.Log.e("pulse", "reading the picked file failed", t);
            }
        }).start();
    }

    /**
     * Opens Android's installer for a downloaded update (in files/updates). The first time, Android
     * wants "Install unknown apps" allowed for Pulse: its settings page opens instead ("permission").
     */
    public static String installApk(String path) {
        Activity a = activity;
        if (a == null) return "no activity";
        try {
            if (!a.getPackageManager().canRequestPackageInstalls()) {
                Intent s = new Intent(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:" + a.getPackageName()));
                a.runOnUiThread(() -> a.startActivity(s));
                return "permission";
            }
            String name = new File(path).getName();
            Uri uri = Uri.parse("content://" + ApkProvider.AUTHORITY + "/" + name);
            Intent i = new Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_ACTIVITY_NEW_TASK);
            a.runOnUiThread(() -> a.startActivity(i));
            return "ok";
        } catch (Throwable t) {
            return String.valueOf(t.getMessage());
        }
    }

    /** Saves a file to Downloads; returns where, or null. */
    public static String saveToDownloads(String name, byte[] data) {
        Activity a = activity;
        if (a == null) return null;
        try {
            if (Build.VERSION.SDK_INT >= 29) {
                ContentValues v = new ContentValues();
                v.put(MediaStore.Downloads.DISPLAY_NAME, name);
                v.put(MediaStore.Downloads.MIME_TYPE, "application/json");
                Uri uri = a.getContentResolver().insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, v);
                if (uri == null) return null;
                try (OutputStream out = a.getContentResolver().openOutputStream(uri)) { out.write(data); }
                return "Downloads/" + name;
            }
            File dir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS);
            dir.mkdirs();
            File f = new File(dir, name);
            try (FileOutputStream out = new FileOutputStream(f)) { out.write(data); }
            return f.getAbsolutePath();
        } catch (Exception e) {
            return null;
        }
    }
}
