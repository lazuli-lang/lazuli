import { defineConfig } from "vitest/config";

// Minimal vitest setup — happy-dom gives us localStorage, StorageEvent, and
// DOM keydown for the helper tests without spinning a full jsdom. RTL's
// renderHook runs against this same env.
export default defineConfig({
  test: {
    environment: "happy-dom",
    globals: false,
    include: [
      "src/**/*.test.ts",
      "src/**/*.test.tsx",
      "tests/**/*.test.ts",
      "tests/**/*.test.tsx",
    ],
  },
});
