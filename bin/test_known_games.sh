cargo build --profile dev-opt
find roms/ -name '*nes' -depth 1 -exec target/dev-opt/tetanes {} \;
