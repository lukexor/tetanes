//! Bakes the NTSC filter's color lookup table into the binary.
//!
//! Generating it costs ~30 ms of `powf` and `sin_cos`, which used to be paid lazily by whichever
//! frame first used the NTSC filter - i.e. as a visible hitch the moment the filter was switched
//! on. It depends on nothing but constants, so it is computed here instead.

include!("src/video/ntsc_palette.rs");

fn main() {
    println!("cargo::rerun-if-changed=src/video/ntsc_palette.rs");
    println!("cargo::rerun-if-changed=build.rs");

    let out_dir = std::env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo");
    let path = std::path::Path::new(&out_dir).join("ntsc_palette.bin");
    std::fs::write(&path, generate_ntsc_palette())
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
}
