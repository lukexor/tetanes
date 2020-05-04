const path = require('path');

module.exports = {
  entry: './bootstrap.js',
  output: {
    filename: 'bundle.js',
    path: path.resolve(__dirname, 'dist'),
  },
};
