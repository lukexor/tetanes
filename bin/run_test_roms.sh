# Some tests rely on deterministic RAM state
cargo build --release --features no-randomize-ram
find tests/ -iname '*.nes' -exec target/release/tetanes --speed 4 {} \;
