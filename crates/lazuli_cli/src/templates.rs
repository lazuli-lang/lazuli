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

/// The TS app shell that mounts Lazuli providers + Router. User
/// edits after scaffold; Lazuli never overwrites.
pub const FRONTEND_WEB_ROOT_TSX: &str = r#"import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { LazuliProvider, LazuliClient } from "@lazuli/runtime/react";
import { ThemeProvider } from "@/app/theme/theme_provider";
import "@/app/theme/globals.css";

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

pub const FRONTEND_THEME_GLOBALS_CSS: &str = r#"@import "@/dist/ts-web/design/tokens.css";

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

pub const FRONTEND_TAILWIND_CONFIG_TS: &str = r#"import { lazuliPreset } from "@/dist/ts-web/design/tailwind.gen";

export default {
  presets: [lazuliPreset],
  content: ["./features/**/*.tsx", "./app/**/*.tsx", "./index.html"],
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
      "@/*": ["./*"]
    }
  },
  "include": ["features", "app", "dist/ts-web", "vite.config.ts", "tailwind.config.ts"]
}
"#;

pub const FRONTEND_VITE_CONFIG_TS: &str = r#"import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "."),
    },
  },
});
"#;

pub const FRONTEND_PACKAGE_JSON: &str = r#"{
  "name": "lazuli-app",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
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
dist/
.lazuli/
"#;

/// Mobile shell (Expo/RN root). `@lazuli/runtime-native` is a future
/// package — comment marks the placeholder so users see what to wire
/// once it ships.
pub const FRONTEND_MOBILE_ROOT_TSX: &str = r#"import { registerRootComponent } from "expo";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { SafeAreaView, Text } from "react-native";
// NOTE: `@lazuli/runtime-native` is not yet shipped. Once available,
// import { LazuliProvider, LazuliClient } from it and wrap the tree
// the same way as `app/shell/web/root.tsx`.

const queryClient = new QueryClient();

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
        <Text>Lazurite mobile scaffold. Wire your navigator after `lazuli generate ts`.</Text>
      </SafeAreaView>
    </QueryClientProvider>
  );
}

registerRootComponent(App);
"#;

pub const FRONTEND_MOBILE_PACKAGE_JSON: &str = r#"{
  "name": "lazuli-app-mobile",
  "private": true,
  "main": "app/shell/mobile/root.tsx",
  "scripts": {
    "start": "expo start",
    "android": "expo start --android",
    "ios": "expo start --ios"
  },
  "dependencies": {
    "@tanstack/react-query": "^5.51.0",
    "expo": "~51.0.0",
    "expo-status-bar": "~1.12.0",
    "react": "18.2.0",
    "react-native": "0.74.5",
    "react-native-safe-area-context": "4.10.5"
  },
  "devDependencies": {
    "@babel/core": "^7.24.0",
    "@types/react": "~18.2.79",
    "typescript": "~5.3.3"
  }
}
"#;

/// `lazurite.toml [frontends.<x>]` snippet appended when --frontends flag set
pub const FRONTEND_MANIFEST_WEB_SNIPPET: &str = r#"
[frontends.web]
target = "vite-react"
out = "dist/ts-web"
audiences = ["admin", "public"]
"#;

pub const FRONTEND_MANIFEST_MOBILE_SNIPPET: &str = r#"
[frontends.mobile]
target = "expo"
out = "dist/ts-mobile"
audiences = ["mobile"]
"#;
