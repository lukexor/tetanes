---
paths:
  - "tetanes-core/src/common.rs"
  - "tetanes-core/test_roms/**/*"
  - "tetanes-core/test_results/**/*"
---

# ROM snapshot tests

Most of `tetanes-core`'s tests run a ROM and compare a frame hash. The harness lives in
`tetanes-core/src/common.rs` (`mod tests`).

The `test_roms!` macro declares one `#[test]` per named ROM. Expectations come from
`test_roms/<dir>/tests.json` (frame number to frame-buffer or audio hash, plus optional `Action`s to
inject), and rendered PNGs land in `tetanes-core/test_results/{pass,fail}/`.

Adding a ROM test means three things: the ROM, a `tests.json` entry, and a name in the relevant
`test_roms!` invocation at the bottom of `common.rs`.

```sh
cargo nextest run -p tetanes-core --all-features           # everything
cargo nextest run -p tetanes-core nestest                  # substring filter for one test
cargo nextest run -p tetanes-core common::tests::cpu::     # a whole ROM-test group
cargo make update-snapshots -- <test>                      # rewrite expected frame hashes
```

`cargo make update-snapshots` rewrites `tests.json` in place. Only use it when a hash change is
intentional and the resulting PNG has been eyeballed. It is `UPDATE_SNAPSHOT=1` plus
`--test-threads=1`. The harness merges its own entry into the file under a lock either way, so the
raw env var is safe too, but serialising keeps the diff readable.

The `tetanes` crate has its own tests in a separate CI job, covering the audio-rate control loop and
the rewind ring.
