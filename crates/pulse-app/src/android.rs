//! Android: the NativeActivity entry point and the JNI bridge to the small Java layer
//! (android/app/src/main/java/app/pulse): media session + foreground service, the file picker,
//! saving to Downloads.
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};

use jni::{JNIEnv, NativeMethod};
use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue};

use crate::app::{MediaInfo, Platform};

unsafe extern "C" {
    fn __android_log_write(
        prio: i32,
        tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char,
    ) -> i32;
}

/// Writes to logcat under the tag `pulse` (eprintln goes nowhere on Android).
pub fn log(msg: &str) {
    let text = std::ffi::CString::new(msg.replace('\0', " ")).unwrap_or_default();
    // SAFETY: both pointers are valid NUL-terminated strings for the duration of the call; 6 = ANDROID_LOG_ERROR.
    unsafe {
        __android_log_write(6, c"pulse".as_ptr(), text.as_ptr());
    }
}

static COMMANDS: Mutex<Option<Sender<String>>> = Mutex::new(None);
type Picked = Box<dyn FnOnce(Vec<u8>) + Send>;
static PICKED: Mutex<Option<Picked>> = Mutex::new(None);

struct Java {
    vm: jni::JavaVM,
    activity: GlobalRef,
    bridge: GlobalRef,
    offload: GlobalRef,
}

/// OffloadPlayer.java over JNI.
struct JavaOffload(std::sync::Arc<Java>);

impl JavaOffload {
    fn with<R>(&self, f: impl FnOnce(&mut JNIEnv, &JClass) -> jni::errors::Result<R>) -> Option<R> {
        let mut env = self.0.vm.attach_current_thread().ok()?;
        // freed on return (see `call`)
        let r = env.with_local_frame(16, |env| -> jni::errors::Result<R> {
            let class = JClass::from(env.new_local_ref(&self.0.offload)?);
            f(env, &class)
        });
        match r {
            Ok(r) => Some(r),
            Err(e) => {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
                log(&format!("offload call failed: {e}"));
                None
            }
        }
    }
}

impl crate::app::Offload for JavaOffload {
    fn play(&self, path: &str, start: f64, volume: f32) -> bool {
        self.with(|env, c| {
            let p = env.new_string(path)?;
            env.call_static_method(c, "play", "(Ljava/lang/String;IF)Z", &[JValue::Object(&p), JValue::Int((start * 1000.0) as i32), JValue::Float(volume)])?.z()
        })
        .unwrap_or(false)
    }
    fn pause(&self) {
        self.with(|env, c| env.call_static_method(c, "pause", "()V", &[]).map(|_| ()));
    }
    fn resume(&self) {
        self.with(|env, c| env.call_static_method(c, "resume", "()V", &[]).map(|_| ()));
    }
    fn seek(&self, secs: f64) {
        self.with(|env, c| env.call_static_method(c, "seek", "(I)V", &[JValue::Int((secs * 1000.0) as i32)]).map(|_| ()));
    }
    fn is_playing(&self) -> bool {
        self.with(|env, c| env.call_static_method(c, "isPlaying", "()Z", &[])?.z()).unwrap_or(false)
    }
    fn position(&self) -> Option<f64> {
        let ms = self.with(|env, c| env.call_static_method(c, "position", "()I", &[])?.i())?;
        (ms >= 0).then(|| f64::from(ms) / 1000.0)
    }
    fn release(&self) {
        self.with(|env, c| env.call_static_method(c, "release", "()V", &[]).map(|_| ()));
    }
}

/// Runs `f` with an attached JNI env; failures are logged, never fatal.
fn call(java: &Java, f: impl FnOnce(&mut JNIEnv, &JObject, &JClass) -> jni::errors::Result<()>) {
    let Ok(mut env) = java.vm.attach_current_thread() else {
        return;
    };
    // a local frame: every Java reference made here is freed on return (this thread stays
    // attached, so without it they piled up with each call, until Android ended the app)
    let r = env.with_local_frame(16, |env| -> jni::errors::Result<()> {
        let bridge = JClass::from(env.new_local_ref(&java.bridge)?);
        f(env, java.activity.as_obj(), &bridge)
    });
    if let Err(e) = r {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        log(&format!("java call failed: {e}"));
    }
}

/// App classes have to come from the activity's class loader (FindClass on a native thread sees
/// only the system's).
fn load_class(env: &mut JNIEnv, activity: &JObject, name: &str) -> jni::errors::Result<GlobalRef> {
    let loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
        .l()?;
    let jname = env.new_string(name)?;
    let class = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&jname)],
        )?
        .l()?;
    env.new_global_ref(class)
}

pub fn main(app: slint::android::AndroidApp) {
    // a panic would otherwise end the app without a word
    std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}"))));
    let data_dir = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/data/app.pulse/files"));
    // SAFETY: android-activity hands out the process's JavaVM and the activity's jobject; both outlive the app.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
    let java = vm.and_then(|vm| {
        let activity = {
            let mut env = vm.attach_current_thread()?;
            // SAFETY: as above, a valid local reference to the NativeActivity object.
            let obj = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
            let activity = env.new_global_ref(&obj)?;
            let bridge = load_class(&mut env, &obj, "app.pulse.MediaBridge")?;
            // NativeActivity loads this library itself, not through System.loadLibrary, so Java
            // can't find MediaBridge's native methods by name: calling one threw
            // UnsatisfiedLinkError and closed the app (a picked file, a notification button).
            // Registering them binds them directly.
            let registered = env.register_native_methods(
                &bridge,
                &[
                    NativeMethod {
                        name: "nativeCommand".into(),
                        sig: "(Ljava/lang/String;)V".into(),
                        fn_ptr: Java_app_pulse_MediaBridge_nativeCommand as *mut std::ffi::c_void,
                    },
                    NativeMethod {
                        name: "nativeBackupPicked".into(),
                        sig: "([B)V".into(),
                        fn_ptr: Java_app_pulse_MediaBridge_nativeBackupPicked as *mut std::ffi::c_void,
                    },
                ],
            );
            match registered {
                Ok(()) => log("native methods registered"),
                Err(e) => {
                    let _ = env.exception_clear();
                    log(&format!("registering native methods failed: {e}"));
                }
            }
            env.call_static_method(
                &bridge,
                "init",
                "(Landroid/app/Activity;)V",
                &[JValue::Object(&obj)],
            )?;
            let offload = load_class(&mut env, &obj, "app.pulse.OffloadPlayer")?;
            (activity, bridge, offload)
        };
        Ok(Java {
            vm,
            activity: activity.0,
            bridge: activity.1,
            offload: activity.2,
        })
    });
    let java = match java {
        Ok(j) => Some(std::sync::Arc::new(j)),
        Err(e) => {
            log(&format!("no Java bridge: {e}"));
            None
        }
    };
    if let Err(e) = slint::android::init(app) {
        log(&format!("slint init failed: {e}"));
        return;
    }
    let (tx, rx) = mpsc::channel();
    *COMMANDS.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    let (j1, j2, j3, j4, j5) = (java.clone(), java.clone(), java.clone(), java.clone(), java.clone());
    let platform = Platform {
        offload: java.clone().map(|j| Box::new(JavaOffload(j)) as Box<dyn crate::app::Offload>),
        data_dir,
        music_dirs: vec![
            PathBuf::from("/storage/emulated/0/Music"),
            PathBuf::from("/storage/emulated/0/Download"),
        ],
        pick_backup: Box::new(move |done| {
            *PICKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(done);
            if let Some(j) = &j1 {
                call(j, |env, _, bridge| {
                    env.call_static_method(bridge, "pickBackup", "()V", &[])
                        .map(|_| ())
                });
            }
        }),
        pick_image: Box::new(move |done| {
            *PICKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(done);
            if let Some(j) = &j4 {
                call(j, |env, _, bridge| {
                    env.call_static_method(bridge, "pickImage", "()V", &[])
                        .map(|_| ())
                });
            }
        }),
        install_apk: Box::new(move |path| {
            let Some(j) = &j5 else {
                return "no Java bridge".into();
            };
            let mut out = "couldn't open the installer".to_string();
            call(j, |env, _, bridge| {
                let p = env.new_string(path.to_string_lossy())?;
                let r = env
                    .call_static_method(bridge, "installApk", "(Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(&p)])?
                    .l()?;
                if !r.is_null() {
                    out = env.get_string(&JString::from(r))?.into();
                }
                Ok(())
            });
            out
        }),
        save_file: Box::new(move |name, bytes| {
            let Some(j) = &j2 else {
                return Err("no file access".into());
            };
            let mut out = Err("couldn't save".to_string());
            call(j, |env, _, bridge| {
                let jname = env.new_string(name)?;
                let data = env.byte_array_from_slice(bytes)?;
                let r = env
                    .call_static_method(
                        bridge,
                        "saveToDownloads",
                        "(Ljava/lang/String;[B)Ljava/lang/String;",
                        &[JValue::Object(&jname), JValue::Object(&data)],
                    )?
                    .l()?;
                if !r.is_null() {
                    out = Ok(env.get_string(&JString::from(r))?.into());
                }
                Ok(())
            });
            out
        }),
        media: Box::new(move |m: &MediaInfo| {
            let Some(j) = &j3 else { return };
            call(j, |env, _, bridge| {
                let (t, a, art) = (
                    env.new_string(&m.title)?,
                    env.new_string(&m.artist)?,
                    env.new_string(&m.artwork_url)?,
                );
                env.call_static_method(
                    bridge,
                    "update",
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;ZDD)V",
                    &[
                        JValue::Object(&t),
                        JValue::Object(&a),
                        JValue::Object(&art),
                        JValue::Bool(u8::from(m.playing)),
                        JValue::Double(m.position),
                        JValue::Double(m.duration),
                    ],
                )
                .map(|_| ())
            });
        }),
        media_commands: Some(rx),
    };
    if let Err(e) = crate::app::run(platform) {
        log(&format!("startup failed: {e}"));
    }
}

/// MediaBridge.nativeCommand: play, pause, toggle, next, prev, seek:<seconds>.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_pulse_MediaBridge_nativeCommand(
    mut env: JNIEnv,
    _class: JClass,
    cmd: JString,
) {
    let Ok(cmd) = env.get_string(&cmd).map(String::from) else {
        return;
    };
    if let Some(tx) = COMMANDS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = tx.send(cmd);
    }
    // handled right away: in the background the app's heartbeat may be stopped
    crate::app::wake();
}

/// MediaBridge.nativeBackupPicked: the bytes of the file the person chose.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_pulse_MediaBridge_nativeBackupPicked(
    env: JNIEnv,
    _class: JClass,
    data: JByteArray,
) {
    let Ok(bytes) = env.convert_byte_array(&data) else {
        return;
    };
    let done = PICKED.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(done) = done {
        // a panic must not unwind into Java (that aborts the process)
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| done(bytes))).is_err() {
            log("picked file: the handler panicked");
        }
    }
}
