//! The Pulse app: Slint UI over pulse-core / pulse-net / pulse-audio / pulse-store.
slint::include_modules!();
#[cfg(target_os = "android")]
mod android;
mod app;

pub use app::{Offload, Platform, run};

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(a: slint::android::AndroidApp) {
    android::main(a);
}
