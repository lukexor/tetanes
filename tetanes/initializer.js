export default function () {
  const loading = document.getElementById("loading-status");
  const error = document.getElementById("error");
  return {
    onStart: () => {
      console.log("Loading...");
      console.time("initializer");
      loading.classList.remove("hidden");
      error.classList.add("hidden");
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
      loading.classList.add("hidden");
    },
    onSuccess: () => {
      console.log("Loading... successful!");
      error.classList.add("hidden");
    },
    onFailure: (error) => {
      console.error(`Loading... failed! ${error}`);
      loading.classList.add("hidden");
      error.classList.remove("hidden");
      error.innerText = `Loading... failed! ${error}`;
    },
  };
}
