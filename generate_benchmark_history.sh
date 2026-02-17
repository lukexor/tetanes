#!/usr/bin/env bash

set -euxo pipefail

cp -a rust-toolchain.toml /tmp
cp -a tetanes-core/benches/clock_frame.rs /tmp
# cp -a tetanes-core/benches/frame_time.rs /tmp

tags=(
  tetanes-core-v0.10.0
  tetanes-core-v0.11.0
  tetanes-core-v0.12.2
  tetanes-core-v0.13.0
  main
)

git stash

for tag in "${tags[@]}"
do
  git checkout "$tag"
  cp -a /tmp/rust-toolchain.toml .
  cp -a /tmp/clock_frame.rs tetanes-core/benches/clock_frame.rs
  cargo bench --bench clock_frame
  git restore .
done

git stash pop
