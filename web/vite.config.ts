import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  build: {
    // The entry chunk cannot get under Vite's 500 kB default: React Aria and
    // its supporting packages render the login and browse screens and alone
    // minify to about 512 kB (measured in #216). Screens and the advanced
    // search form are already lazy. The limit sits above the floor so the
    // build reports a size regression, not a condition nobody intends to fix.
    chunkSizeWarningLimit: 1000,
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      // Same-origin /v1 in browser → vault (blank server URL on login).
      "/v1": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
      },
      // Login health light uses GET /health when the server URL field is blank.
      "/health": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
      },
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    // Prefer // @vitest-environment jsdom in *.test.tsx when globs are unavailable.
    environmentMatchGlobs: [
      ["**/*.test.tsx", "jsdom"],
      ["**/*.spec.tsx", "jsdom"],
    ],
    setupFiles: ["src/test/setup.ts"],
  },
});
