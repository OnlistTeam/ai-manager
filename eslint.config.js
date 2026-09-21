// @ts-check
import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import jsxA11y from "eslint-plugin-jsx-a11y";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "dist/",
      "release/",
      "coverage/",
      "node_modules/",
      ".worktrees/",
      "test-packages/",
      // Rust crate: owned by cargo fmt/clippy, and its target/ holds generated bundles.
      "src-tauri/",
    ],
  },

  js.configs.recommended,

  // Node tooling: build scripts, release scripts, and their `node --test` suites.
  {
    files: ["scripts/**/*.mjs", "*.js"],
    languageOptions: {
      globals: globals.node,
    },
  },
  {
    files: ["**/*.cjs"],
    languageOptions: {
      sourceType: "commonjs",
      globals: globals.node,
    },
  },

  // Renderer and test sources.
  {
    files: ["**/*.{ts,tsx}"],
    extends: [tseslint.configs.recommended],
    languageOptions: {
      parserOptions: {
        ecmaFeatures: { jsx: true },
      },
      globals: globals.browser,
    },
    rules: {
      // AI_RULES.md forbids `any` in new TypeScript; narrow with `unknown` + Zod instead.
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
          destructuredArrayIgnorePattern: "^_",
        },
      ],
    },
  },

  // The download page: plain browser JavaScript, served as-is with no build
  // step, so it is neither renderer TypeScript nor Node tooling.
  {
    files: ["website/**/*.js"],
    languageOptions: {
      globals: globals.browser,
    },
  },

  // Vite and Vitest configs run in Node.
  {
    files: ["*.config.ts"],
    languageOptions: {
      globals: globals.node,
    },
  },

  // React rules only apply where components and hooks live.
  {
    files: ["src/**/*.{ts,tsx}", "tests/**/*.{ts,tsx}"],
    plugins: { "react-hooks": reactHooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
    },
  },

  {
    files: ["src/**/*.tsx"],
    extends: [jsxA11y.flatConfigs.recommended],
    rules: {
      // Labels here wrap their control and nest the text one level deeper for layout.
      "jsx-a11y/label-has-associated-control": ["error", { depth: 3 }],
      "jsx-a11y/no-noninteractive-element-interactions": [
        "error",
        {
          ...jsxA11y.flatConfigs.recommended.rules[
            "jsx-a11y/no-noninteractive-element-interactions"
          ][1],
          // `<form onKeyDown>` is the submit-on-Enter pattern; its fields stay interactive.
          form: ["onKeyDown", "onKeyUp", "onKeyPress"],
        },
      ],
    },
  },

  // Tests: vitest globals are enabled in vitest.config.ts.
  {
    files: ["tests/**/*.{ts,tsx}"],
    languageOptions: {
      globals: { ...globals.browser, ...globals.node, ...globals.vitest },
    },
  },

  // Must stay last: turns off every rule Prettier already decides.
  prettier,
);
