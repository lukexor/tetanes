cargo build
find ../roms/ -name '*nes' -exec "$CARGO_TARGET_DIR"/debug/tetanes -c {} \;
