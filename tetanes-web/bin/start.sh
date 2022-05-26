#/bin/sh

wasm-pack build

if [ $? -eq 0 ]; then
    pushd www
    npm install
    npm start
    popd
fi
