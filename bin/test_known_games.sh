cargo build --profile dev-opt
find roms/ -name '*nes' -depth 1 -exec $CARGO_TARGET_DIR/dev-opt/tetanes {} \;
