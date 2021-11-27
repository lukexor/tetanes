#/bin/sh

wasm-pack build --dev

if [ $? -eq 0 ]; then
    pushd www
    npm install
    npm run start_dev
    popd
fi
