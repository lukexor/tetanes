#!/usr/bin/env bash

wasm-pack build $*
pushd www
npm install
npm run build
popd
