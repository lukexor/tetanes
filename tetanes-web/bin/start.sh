#/bin/sh

wasm-pack build --release;

if [ $? -eq 0 ]; then
    pushd www;
    npm run start;
    popd;
fi
