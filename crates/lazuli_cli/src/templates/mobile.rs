//! Frontend mobile (Expo Router) scaffold templates.
//!
//! Verbatim raw-string blobs emitted by
//! `cmd_new_frontends::scaffold_frontend_mobile` when `lazuli new
//! --frontends mobile` (or `web,mobile`) is invoked. The 6+6 closed-
//! catalog mobile shape mirrors the web side; both are sourced from
//! `docs/decisions/client_src_canonical_architecture_2026-05-17.md`.
//!
//! Wave R7-3 extract: lifted out of `templates.rs`.

/// Expo Router file-based root layout. User-owned one-line re-export
/// of the regen body that `lazuli generate ts` writes to
/// `dist/ts-mobile/runtime/layout`. The user replaces this wrapper
/// when they want extra providers; Lazuli never overwrites it. See
/// `docs/proposals/mobile-target.md` §5.4.
pub const FRONTEND_MOBILE_APP_LAYOUT_TSX: &str = r#"// Replace with your own wrapper if you need extra providers.
// `lazuli generate ts` will not overwrite this file.
export { default } from "@/dist/ts-mobile/runtime/layout";
"#;

/// Placeholder home for Expo Router. Lists nothing useful yet — the
/// user customizes once a `surface` declares a mobile audience. The
/// per-view files under `app/<audience>/...` are scaffolded once by
/// `lazuli generate ts` and consume the matching generated hooks.
pub const FRONTEND_MOBILE_APP_INDEX_TSX: &str = r#"import { SafeAreaView, Text } from "react-native";

export default function Home() {
  return (
    <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
      <Text>Lazurite mobile scaffold. Add a `surface ... mobile` and run `lazuli generate ts`.</Text>
    </SafeAreaView>
  );
}
"#;

/// Expo manifest. `scheme` matters for deep-linking; users edit `name`
/// and `slug` before publishing.
///
/// Wave G shipped 2026-05-17 — `expo-notifications` plugin entry per
/// M5 pick (see top-of-file Wave G comment block).
pub const FRONTEND_MOBILE_APP_JSON: &str = r#"{
  "expo": {
    "name": "lazuli-app-mobile",
    "slug": "lazuli-app-mobile",
    "version": "0.0.1",
    "orientation": "portrait",
    "scheme": "lazuliapp",
    "plugins": ["expo-router", "expo-notifications"],
    "ios": {
      "supportsTablet": true
    },
    "android": {
      "package": "app.lazuli.mobile"
    }
  }
}
"#;

/// Expo's babel preset includes `expo-router`'s preset transitively;
/// users rarely customize this file.
///
/// Wave G shipped 2026-05-17 — `react-native-reanimated/plugin` listed
/// LAST in `plugins` per M4 pick (Reanimated requires its babel plugin
/// to be the final entry to correctly transform worklets).
pub const FRONTEND_MOBILE_BABEL_CONFIG: &str = r#"module.exports = function (api) {
  api.cache(true);
  return {
    presets: ["babel-preset-expo"],
    plugins: ["react-native-reanimated/plugin"],
  };
};
"#;

/// Default Expo Metro config. Lazuli does not customize Metro today;
/// users may extend this file to add custom resolver/serializer config.
pub const FRONTEND_MOBILE_METRO_CONFIG: &str = r#"const { getDefaultConfig } = require("expo/metro-config");

const config = getDefaultConfig(__dirname);

module.exports = config;
"#;

/// tsconfig.json for the Expo project. Extends `expo/tsconfig.base`
/// (shipped by the `expo` package) and adds a `@/` path alias rooted at
/// the project root so generated `dist/ts-mobile/...` imports resolve
/// cleanly. Users may add more aliases as needed.
pub const FRONTEND_MOBILE_TSCONFIG: &str = r#"{
  "extends": "expo/tsconfig.base",
  "compilerOptions": {
    "strict": true,
    "paths": {
      "@/*": ["../../*"]
    }
  },
  "include": [
    "**/*.ts",
    "**/*.tsx",
    "../../dist/ts-mobile/**/*.ts",
    "../../dist/ts-mobile/**/*.tsx"
  ]
}
"#;

/// `LazuliClient` construction for the mobile shell. The runtime body
/// at `dist/ts-mobile/runtime/layout` imports `client` from here and
/// hands it to `<LazuliProvider>`. Users wire the real API base URL
/// (typically from `process.env.EXPO_PUBLIC_API_URL`) once.
pub const FRONTEND_MOBILE_SHELL_CLIENT_TS: &str = r#"import { LazuliClient } from "@lazuli/runtime";

const baseUrl =
  // EXPO_PUBLIC_* env vars are inlined at build time by Metro.
  // Set this in `.env` or your CI/build config.
  // Example: EXPO_PUBLIC_API_URL=https://api.example.com
  // Falls back to localhost for `expo start --tunnel` flows.
  // Replace with your real default for production builds.
  process.env.EXPO_PUBLIC_API_URL ?? "http://localhost:8080";

export const client = new LazuliClient({ baseUrl });
"#;

/// Project-level `.gitignore` additions for the mobile scaffold. Lazuli
/// appends these alongside the shared web-project ignores so the same
/// repo can host both targets without churn.
pub const FRONTEND_MOBILE_GITIGNORE: &str = r#"# Expo
.expo/
.expo-shared/

# Native build outputs (rarely committed; uncomment if running prebuild)
# ios/
# android/
"#;

/// Mobile `package.json` template.
///
/// Wave G shipped 2026-05-17 — M2-M6 Tier-2 picks anchored at
/// `docs/proposals/lazurite-frontend-stack-mobile-grading-2026-05-17.md`
/// (architect re-grade PASS 8.87). M1 design system DEFERRED per the
/// grading — no design-system library lands on mobile until a pilot
/// drives the pick.
///
/// M2 icons: `lucide-react-native` (inherits W3, unified Lucide major
/// 0.408). M3-state: AsyncStorage (status-quo Tier-1 promotion).
/// M3-secrets: `expo-secure-store`. M4 animation: `react-native-
/// reanimated` (paired with babel plugin in `FRONTEND_MOBILE_BABEL_CONFIG`
/// + nothing in `app.json`). M5 push: `expo-notifications` (paired
/// with `app.json` plugin entry). M6 state: `zustand` (inherits W1).
pub const FRONTEND_MOBILE_PACKAGE_JSON: &str = r#"{
  "name": "lazuli-app-mobile",
  "private": true,
  "main": "expo-router/entry",
  "scripts": {
    "start": "expo start",
    "android": "expo start --android",
    "ios": "expo start --ios"
  },
  "dependencies": {
    "@lazuli/runtime": "workspace:*",
    "@react-native-async-storage/async-storage": "^2.0.0",
    "@tanstack/react-query": "^5.51.0",
    "expo": "~51.0.0",
    "expo-notifications": "~0.28.0",
    "expo-router": "~3.5.0",
    "expo-secure-store": "~13.0.0",
    "expo-status-bar": "~1.12.0",
    "lucide-react-native": "^0.408.0",
    "react": "18.2.0",
    "react-native": "0.74.5",
    "react-native-reanimated": "~3.15.0",
    "react-native-safe-area-context": "4.10.5",
    "react-native-screens": "~3.31.1",
    "zustand": "^4.5.0"
  },
  "devDependencies": {
    "@babel/core": "^7.24.0",
    "@types/react": "~18.2.79",
    "typescript": "~5.3.3"
  }
}
"#;
