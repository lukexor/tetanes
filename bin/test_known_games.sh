cargo build
find roms -name '*nes' -depth 1 -exec target/debug/rustynes {} \;
