#/bin/sh

wasm-pack build --dev

if [ $? -eq 0 ]; then
    pushd www
    yarn install
    yarn run start_dev
    popd
fi
