import { defineConfig } from "oxlint";

export default defineConfig({
  $schema: "./node_modules/oxlint/configuration_schema.json",
  plugins: ["typescript", "unicorn", "oxc"],
  categories: {
    correctness: "error",
  },
  rules: {
    "typescript/no-explicit-any": "error",
    "typescript/no-non-null-assertion": "error",
  },
  env: {
    builtin: true,
  },
  options: {
    typeAware: true,
  },
});
