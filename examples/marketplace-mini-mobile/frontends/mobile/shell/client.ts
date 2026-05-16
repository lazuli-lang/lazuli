import { LazuliClient } from "@lazuli/runtime";

const baseUrl =
  // EXPO_PUBLIC_* env vars are inlined at build time by Metro.
  // Set this in `.env` or your CI/build config.
  // Example: EXPO_PUBLIC_API_URL=https://api.example.com
  // Falls back to localhost for `expo start --tunnel` flows.
  // Replace with your real default for production builds.
  process.env.EXPO_PUBLIC_API_URL ?? "http://localhost:8080";

export const client = new LazuliClient({ baseUrl });
