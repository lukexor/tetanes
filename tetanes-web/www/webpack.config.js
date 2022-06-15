const path = require("path");

module.exports = {
  mode: "production",
  entry: "./src/bootstrap.ts",
  module: {
    rules: [
      {
        test: /\.ts$/,
        use: "ts-loader",
        exclude: /node_modules/,
      },
    ],
  },
  resolve: {
    extensions: [".ts", ".js"],
  },
  output: {
    filename: "bundle.js",
    path: path.resolve(__dirname, "public/dist"),
  },
  experiments: {
    asyncWebAssembly: true,
  },
};
