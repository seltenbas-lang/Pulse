fn main() {
    if let Err(e) = pulse_app::run(pulse_app::Platform::desktop()) {
        eprintln!("pulse: {e}");
        std::process::exit(1);
    }
}
