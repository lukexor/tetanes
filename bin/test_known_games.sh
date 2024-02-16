cargo build
find roms/ -name '*nes' -depth 1 -exec $CARGO_TARGET_DIR/debug/tetanes {} \;
