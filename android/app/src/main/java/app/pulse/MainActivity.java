package app.pulse;

import android.Manifest;
import android.app.NativeActivity;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Bundle;

/** Hosts the Rust app (libpulse_app.so, see the manifest's lib_name). */
public class MainActivity extends NativeActivity {
    static final int PICK_BACKUP = 41;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        // the music on the phone, and the playback notification (Android 13+ asks for both)
        if (Build.VERSION.SDK_INT >= 33) {
            String[] want = { Manifest.permission.READ_MEDIA_AUDIO, Manifest.permission.POST_NOTIFICATIONS };
            for (String p : want) {
                if (checkSelfPermission(p) != PackageManager.PERMISSION_GRANTED) { requestPermissions(want, 1); break; }
            }
        } else if (checkSelfPermission(Manifest.permission.READ_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[] { Manifest.permission.READ_EXTERNAL_STORAGE }, 1);
        }
    }

    // the Rust side drops the text field's focus, so the keyboard doesn't come back on return
    @Override
    protected void onPause() {
        super.onPause();
        try { MediaBridge.nativeCommand("hidden"); } catch (Throwable ignored) {}
    }

    // the app left the screen / came back: in the background it draws nothing and slows down
    @Override
    protected void onStop() {
        super.onStop();
        try { MediaBridge.nativeCommand("background"); } catch (Throwable ignored) {}
    }

    @Override
    protected void onStart() {
        super.onStart();
        try { MediaBridge.nativeCommand("foreground"); } catch (Throwable ignored) {}
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == PICK_BACKUP && resultCode == RESULT_OK && data != null && data.getData() != null) {
            MediaBridge.backupPicked(this, data.getData());
        }
    }
}
