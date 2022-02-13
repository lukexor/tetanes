# Some tests rely on deterministic RAM state
cargo build --profile dev-opt
find test_roms/ -iname '*.nes' -exec target/dev-opt/tetanes --speed 4 --consistent_ram {} \;
