#[allow(dead_code)]
pub static DEFAULT_TEMPLATE: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../lazurite/templates/default");

// ---------------------------------------------------------------
// Frontend scaffold templates (L0 #1 §6.1).
// Activated when `lazuli new --frontends web|mobile|web,mobile`.
//
// Newlines are LITERAL `\n` — these strings are written verbatim by
// `cmd_new_frontends::scaffold_frontend_*`. Cross-platform: emitted
// files use LF on every host (Lazuli runs on Windows too).
// ---------------------------------------------------------------

pub const FRONTEND_WEB_INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Lazuli App</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/main.tsx"></script>
  </body>
</html>
"#;

pub const FRONTEND_WEB_MAIN_TSX: &str = r#"import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "@web/shell/root";
import "@web/theme/globals.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
"#;

/// The TS app shell that mounts Lazuli providers + Router. User
/// edits after scaffold; Lazuli never overwrites.
pub const FRONTEND_WEB_ROOT_TSX: &str = r#"import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { LazuliClient } from "@lazuli/runtime";
import { LazuliProvider } from "@lazuli/runtime/react";
import { ThemeProvider } from "@web/theme/theme_provider";

const queryClient = new QueryClient();
const client = new LazuliClient({
  baseUrl: import.meta.env.VITE_API_URL ?? "/api",
});

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <LazuliProvider client={client}>
        <ThemeProvider>
          {/* <RouterProvider router={router} /> — wire after first `lazuli generate ts` */}
          <p>Lazurite app scaffold. Run `lazuli generate ts` and wire the router here.</p>
        </ThemeProvider>
      </LazuliProvider>
    </QueryClientProvider>
  );
}
"#;

pub const FRONTEND_WEB_LAYOUT_TSX: &str = r#"import { Outlet } from "@tanstack/react-router";

/**
 * App-shell layout. Wraps every routed view rendered through the
 * router emitted at `dist/ts-web/routes.gen.ts`.
 *
 * User-owned. Add nav, header, footer, sidebars, etc. here.
 */
export default function Layout() {
  return (
    <div className="lazuli-app-shell">
      <Outlet />
    </div>
  );
}
"#;

pub const FRONTEND_WEB_ERROR_BOUNDARY_TSX: &str = r#"import { Component } from "react";
import type { ErrorInfo, ReactNode } from "react";

interface Props {
  children: ReactNode;
  fallback?: (error: Error) => ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Top-level error boundary. Catches render-time exceptions in the
 * app tree below `root.tsx`. User-owned — extend with telemetry,
 * branded fallback UI, or i18n as needed.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("[lazuli] error boundary caught", error, info);
  }

  render() {
    if (this.state.error) {
      if (this.props.fallback) {
        return this.props.fallback(this.state.error);
      }
      return (
        <div role="alert">
          <h1>Something went wrong.</h1>
          <pre>{this.state.error.message}</pre>
        </div>
      );
    }
    return this.props.children;
  }
}
"#;

pub const FRONTEND_THEME_GLOBALS_CSS: &str = r#"@import "@generated/design/tokens.css";

/* User-owned. Lazuli emits the tokens; how you globalize them is yours. */
"#;

pub const FRONTEND_THEME_PROVIDER_TSX: &str = r#"import { createContext, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";

type Theme = "light" | "dark";

interface ThemeContextValue {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

/**
 * Minimal `data-theme` provider. Sets the attribute on `<html>` so
 * token CSS variables emitted at `dist/ts-web/design/tokens.css`
 * resolve to the right palette. User-owned: replace with system-
 * preference detection, persistence, or scoped theming as needed.
 */
export function ThemeProvider({
  children,
  defaultTheme = "light",
}: {
  children: ReactNode;
  defaultTheme?: Theme;
}) {
  const [theme, setTheme] = useState<Theme>(defaultTheme);

  useEffect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("data-theme", theme);
    }
  }, [theme]);

  const value = useMemo<ThemeContextValue>(
    () => ({
      theme,
      setTheme,
      toggleTheme: () => setTheme((t) => (t === "light" ? "dark" : "light")),
    }),
    [theme],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used inside <ThemeProvider>");
  }
  return ctx;
}
"#;

pub const FRONTEND_TAILWIND_CONFIG_TS: &str = r#"import { lazuliPreset } from "../../dist/ts-web/design/tailwind.gen";

export default {
  presets: [lazuliPreset],
  content: [
    "./index.html",
    "./main.tsx",
    "./shell/**/*.{ts,tsx}",
    "./theme/**/*.{ts,tsx}",
    "./ui/**/*.{ts,tsx}",
    "./hooks/**/*.{ts,tsx}",
    "./lib/**/*.{ts,tsx}",
    "../../app/**/*.{ts,tsx}",
  ],
};
"#;

pub const FRONTEND_TSCONFIG_JSON: &str = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noImplicitOverride": true,
    "esModuleInterop": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "baseUrl": ".",
    "paths": {
      "@app/*": ["../../app/*"],
      "@generated/*": ["../../dist/ts-web/*"],
      "@web/*": ["./*"]
    }
  },
  "include": [
    ".",
    "../../app/**/*.ts",
    "../../app/**/*.tsx",
    "../../dist/ts-web/**/*.ts",
    "../../dist/ts-web/**/*.tsx"
  ],
  "exclude": ["node_modules", "../../dist/go"]
}
"#;

pub const FRONTEND_VITE_CONFIG_TS: &str = r#"import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

const projectRoot = path.resolve(__dirname, "../..");

export default defineConfig({
  root: __dirname,
  plugins: [react()],
  resolve: {
    alias: {
      "@app": path.resolve(projectRoot, "app"),
      "@generated": path.resolve(projectRoot, "dist/ts-web"),
      "@web": path.resolve(projectRoot, "frontends/web"),
    },
  },
  build: {
    outDir: path.resolve(projectRoot, "dist/web"),
    emptyOutDir: true,
  },
});
"#;

pub const FRONTEND_PACKAGE_JSON: &str = r#"{
  "name": "lazuli-app",
  "private": true,
  "type": "module",
  "scripts": {
    "lazuli:check": "lazuli check ../..",
    "lazuli:generate:go": "lazuli generate go ../.. -o ../../dist/go",
    "lazuli:generate:ts": "lazuli generate ts ../..",
    "lazuli:generate": "pnpm lazuli:generate:go && pnpm lazuli:generate:ts",
    "build:go": "go -C ../../dist/go build .",
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "dev:go": "go -C ../../dist/go run .",
    "test:go": "go -C ../../dist/go test ./..."
  },
  "dependencies": {
    "@hookform/resolvers": "^3.9.0",
    "@lazuli/runtime": "^0.1.0",
    "@tanstack/react-query": "^5.51.0",
    "@tanstack/react-router": "^1.45.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-hook-form": "^7.52.0",
    "tailwindcss": "^3.4.0",
    "zod": "^3.23.0"
  },
  "devDependencies": {
    "@types/react": "^18.3.0",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.0",
    "typescript": "^5.5.0",
    "vite": "^5.3.0"
  }
}
"#;

pub const FRONTEND_GITIGNORE: &str = r#"node_modules/
frontends/*/node_modules/
.vite/
frontends/*/.vite/
dist/
.lazuli/
"#;

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
pub const FRONTEND_MOBILE_APP_JSON: &str = r#"{
  "expo": {
    "name": "lazuli-app-mobile",
    "slug": "lazuli-app-mobile",
    "version": "0.0.1",
    "orientation": "portrait",
    "scheme": "lazuliapp",
    "plugins": ["expo-router"],
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
pub const FRONTEND_MOBILE_BABEL_CONFIG: &str = r#"module.exports = function (api) {
  api.cache(true);
  return {
    presets: ["babel-preset-expo"],
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
    "@react-native-async-storage/async-storage": "1.23.1",
    "@tanstack/react-query": "^5.51.0",
    "expo": "~51.0.0",
    "expo-router": "~3.5.0",
    "expo-status-bar": "~1.12.0",
    "react": "18.2.0",
    "react-native": "0.74.5",
    "react-native-safe-area-context": "4.10.5",
    "react-native-screens": "~3.31.1"
  },
  "devDependencies": {
    "@babel/core": "^7.24.0",
    "@types/react": "~18.2.79",
    "typescript": "~5.3.3"
  }
}
"#;

/// `Lazurite.toml [frontends.<x>]` snippet appended when --frontends flag set.
/// Paths follow the canonical layout in `docs/project-structure.md`:
/// default web client at `app/web/`, mobile (different runtime) at
/// `app/clients/mobile/`.
pub const FRONTEND_MANIFEST_WEB_SNIPPET: &str = r#"
[frontends.web]
target = "tanstack-vite"
source = "app/web"
out = "dist/ts-web"
audiences = ["admin", "public"]
"#;

pub const FRONTEND_MANIFEST_MOBILE_SNIPPET: &str = r#"
[frontends.mobile]
target = "expo"
source = "app/clients/mobile"
out = "dist/ts-mobile"
audiences = ["mobile"]
"#;
