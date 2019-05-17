cargo build --release
find tests -iname '*.nes' -exec target/release/rustynes {} \;
