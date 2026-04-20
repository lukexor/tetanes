fn main() {
    if let Ok(target) = std::env::var("TARGET") {
        println!("cargo:rustc-env=DEFAULT_TARGET={target}");
    }
}
