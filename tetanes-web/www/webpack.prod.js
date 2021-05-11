const merge = require("webpack-merge");
const common = require("./webpack.common.js");

module.exports = merge(common, {
  mode: "production",
  devtool: "source-map",
  devServer: {
    hot: false,
  },
  output: merge(common.output, {
    publicPath: "/tetanes/dist/",
  }),
});
