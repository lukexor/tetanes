#!/bin/sh

wasm-pack build --target web && python -m SimpleHTTPServer 8080
