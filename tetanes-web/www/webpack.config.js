module.exports = {
  mode: "development",
  entry: "./src/bootstrap.js",
  output: {
    filename: "bundle.js",
    clean: true,
  },
  devServer: {
    hot: false,
  },
  experiments: {
    asyncWebAssembly: true,
  },
  stats: {
    errorDetails: true,
  },
};
