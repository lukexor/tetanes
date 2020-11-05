#/bin/sh

wasm-pack build

if [ $? -eq 0 ]; then
    pushd www
    yarn install
    yarn run start
    popd
fi
