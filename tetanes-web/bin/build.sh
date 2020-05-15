#/bin/sh

wasm-pack build;
pushd www;
npm run build;
popd;
