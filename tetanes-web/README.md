# TetaNES Web

## Summary

<p align="center">
  <img src="../static/tetanes.png" width="800">
</p>

> photo credit for background: [Zsolt Palatinus](https://unsplash.com/@sunitalap) on [unsplash](https://unsplash.com/photos/pEK3AbP8wa4)

`TetaNES Web` is a [WebAssembly][wasm] version of `TetaNES` that runs in
a modern web browser. See the main `TetaNES` [README][readme] for more details.
`TetaNES Web` specific differences will be outlined below.

## Dependencies

* [Rust][rust]
* [SDL2][sdl2]
* [Wasm][wasm]

## Building

To build `TetaNES Web`, run `sh bin/build.sh` which will output the necessary
bundle and wasm files.

## Running Locally

Running `sh bin/start.sh` or `sh bin/start_dev.sh` will build necessary
resources and boot up a local server.

[wasm]: https://webassembly.org/
[readme]: https://github.com/lukexor/tetanes#readme
