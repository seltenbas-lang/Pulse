fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("material-dark".into());
    // INFALLIBLE: a broken .slint file should stop the build with the compiler's message.
    slint_build::compile_with_config("ui/app.slint", config).unwrap_or_else(|e| panic!("{e}"));
}
