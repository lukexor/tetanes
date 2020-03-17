# Some tests rely on deterministic RAM state
cargo build --features no-randomize-ram
find tests -iname '*.nes' -exec target/debug/rustynes {} \;
