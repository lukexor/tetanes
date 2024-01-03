module.exports = {
  env: {
    browser: true,
    es2020: true,
    commonjs: true,
    node: true,
  },
  extends: [
    "eslint:recommended",
    "plugin:@typescript-eslint/eslint-recommended",
    "plugin:@typescript-eslint/recommended",
    "plugin:prettier/recommended",
    "plugin:tailwindcss/recommended",
  ],
  parser: "@typescript-eslint/parser",
  parserOptions: {
    ecmaVersion: 11,
    sourceType: "script",
  },
  plugins: ["@typescript-eslint", "tailwindcss", "simple-import-sort"],
  rules: {
    // Allow unused vars only if they start with an underscore
    "@typescript-eslint/no-unused-vars": [
      "warn",
      {
        argsIgnorePattern: "^_",
        varsIgnorePattern: "^_",
        caughtErrorsIgnorePattern: "^_",
      },
    ],
    // 2 space indent including switch case statements
    indent: [
      "warn",
      2,
      {
        SwitchCase: 1,
        // Ignore extra indentation when using conditional expressions like
        // template literal strings with expressions
        ignoredNodes: ["ConditionalExpression *"],
      },
    ],
    // Ensure consistent line endings for cross-platform development
    "linebreak-style": ["warn", "unix"],
    quotes: ["warn", "double"],
    semi: ["warn", "always"],
    "simple-import-sort/imports": "warn",
    "simple-import-sort/exports": "warn",
  },
};
