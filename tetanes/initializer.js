export default function () {
  return {
    onStart: () => {
      console.log("Loading...");
      console.time("initializer");
    },
    onProgress: ({ current, total }) => {
      if (!total) {
        console.log(`Loading... ${current} bytes`);
      } else {
        console.log(`Loading... ${Math.round(current / total) * 100}%`);
      }
    },
    onComplete: () => {
      console.log("Loading... done!");
      console.timeEnd("initializer");
    },
    onSuccess: (wasm) => {
      console.log("Loading... successful!");
      console.log("WebAssembly: ", wasm);
    },
    onFailure: (error) => {
      console.error(`Loading... failed! ${error}`);
    },
  };
}
