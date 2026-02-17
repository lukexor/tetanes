cargo build
find ../roms/ -maxdepth 1 -name '*nes' -exec "$CARGO_TARGET_DIR"/debug/tetanes -c {} \;
