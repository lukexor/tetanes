cargo build --release
find roms -name '*nes' -depth 1 -exec target/release/rustynes {} \;
