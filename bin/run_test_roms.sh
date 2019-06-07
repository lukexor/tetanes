cargo build
find tests -iname '*.nes' -exec target/debug/rustynes {} \;
