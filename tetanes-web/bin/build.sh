#/bin/sh

wasm-pack build
pushd www
yarn install
yarn run build
popd
