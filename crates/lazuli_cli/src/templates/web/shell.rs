//! Frontend web shell — HTML index, React root + layout, error
//! boundary, theme provider, package.json + vite + tsconfig + tailwind
//! config, gitignore, the TanStack-router routes index, the Zustand
//! app-store seed, and the W1 dependency picks. Everything the
//! scaffold writes outside the `ui/*` cluster lives here.
//!
//! Wave R7-3 extract: lifted out of `templates/web.rs`.

//! Frontend web (TanStack + Vite) scaffold templates.
//!
//! Verbatim raw-string blobs emitted by `cmd_new_frontends::scaffold_frontend_web`
//! when `lazuli new --frontends web` (or `web,mobile`) is invoked. Each
//! constant carries one file the scaffold writes; doctor rule
//! `VOCAB-CLIENT-SRC-001` keeps the emitted shape in lockstep with
//! `docs/decisions/client_src_canonical_architecture_2026-05-17.md`.
//!
//! Wave R7-3 extract: lifted out of `templates.rs`.

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

/// `app/web/main.tsx` — entry-point seed written by `lazuli new`.
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

/// `app/web/lazuli.ts` — the singleton runtime client + query cache the
/// whole app shares. Every generated SDK hook (`useLoginAccount`,
/// `useListCustomers`, …) reads the `LazuliClient` from
/// `<LazuliProvider client={lazuliClient}>` and the cache from
/// `<QueryClientProvider client={queryClient}>` — both wired in
/// `shell/root.tsx` from THIS module, so there is exactly one client
/// and one cache for the process.
///
/// `baseUrl: ""` → requests hit relative `/api/...`, which Vite proxies
/// to the Go API in dev (see `vite.config.ts`) and serves same-origin in
/// prod. Override with `PUBLIC_API_URL` when the API lives elsewhere.
///
/// Auth: the bearer token is rehydrated from `localStorage` at module
/// load so a refresh keeps the session, and `persistSessionToken` is the
/// one place that writes it. A pilot's hand-wired login route (Lazuli
/// does NOT scaffold one — it's authored from your `.lzx` `view`s) calls
/// `persistSessionToken(result.token)` on `login.mutateAsync(...)`
/// success, and `persistSessionToken(null)` on logout.
pub const FRONTEND_WEB_LAZULI_TS: &str = r#"import { QueryClient } from "@tanstack/react-query";
import { LazuliClient } from "@lazuli/runtime";

/** localStorage key the session bearer token is persisted under. */
const SESSION_TOKEN_KEY = "lazuli.session.token";

function readStoredToken(): string | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage.getItem(SESSION_TOKEN_KEY);
}

/**
 * The one runtime client for the app. `baseUrl: ""` keeps requests
 * relative (`/api/...`) — Vite-proxied in dev, same-origin in prod.
 * Set `PUBLIC_API_URL` to point at a different origin.
 */
export const lazuliClient = new LazuliClient({
  baseUrl: import.meta.env.PUBLIC_API_URL ?? "",
});

// Rehydrate a persisted session so a page refresh stays logged in.
const storedToken = readStoredToken();
if (storedToken) {
  lazuliClient.setAuthToken(storedToken);
}

/**
 * Persist (or clear) the session bearer token. Call with the token
 * returned by your login command on success; call with `null` on logout.
 * Writes both `localStorage` (survives refresh) and the live client.
 */
export function persistSessionToken(token: string | null): void {
  if (typeof localStorage !== "undefined") {
    if (token) {
      localStorage.setItem(SESSION_TOKEN_KEY, token);
    } else {
      localStorage.removeItem(SESSION_TOKEN_KEY);
    }
  }
  lazuliClient.setAuthToken(token);
}

/** The one TanStack Query cache for the app. */
export const queryClient = new QueryClient();
"#;

/// The TS app shell that mounts the Lazuli providers + Router. The
/// providers come FIRST so every generated SDK hook in the routed tree
/// finds its `LazuliClient` (via `LazuliProvider`) and its query cache
/// (via `QueryClientProvider`) — without them the first screen throws
/// `useLazuliClient: missing LazuliProvider client`. User edits after
/// scaffold; Lazuli never overwrites.
///
/// The router below is a self-contained placeholder so the scaffold
/// renders + typechecks BEFORE the first `lazuli generate ts`. Swap it
/// for the generated router as marked once you've declared `view`s.
pub const FRONTEND_WEB_ROOT_TSX: &str = r#"import { QueryClientProvider } from "@tanstack/react-query";
import { LazuliProvider } from "@lazuli/runtime/react";
import {
  RouterProvider,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { ThemeProvider } from "@web/theme/theme_provider";
import { lazuliClient, queryClient } from "@web/lazuli";

// --- Router -----------------------------------------------------------
// Placeholder router: renders a welcome screen so a fresh scaffold boots
// and typechecks before any `view` is declared. After `lazuli generate
// ts` emits `dist/ts-web/<audience>/routes.gen.tsx`, replace this whole
// block with the generated factory:
//
//   import { createGeneratedRouter } from "@generated/<audience>/routes.gen";
//   const router = createGeneratedRouter({
//     client: lazuliClient,
//     queryClient,
//     components: { /* one entry per `view`, e.g. signIn: SignInRoute */ },
//   });
//
// The generated router carries the route guards + typed params from your
// `.lzx` audience blocks. (A login/sign-in route is pilot-authored from
// your `view`s — on `login.mutateAsync(input)` success call
// `persistSessionToken(result.token)` from `@web/lazuli`.)
const rootRoute = createRootRoute();
const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => (
    <p>Lazurite app scaffold. Declare a `view` and run `lazuli generate ts`.</p>
  ),
});
const router = createRouter({
  routeTree: rootRoute.addChildren([indexRoute]),
});
// ----------------------------------------------------------------------

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <LazuliProvider client={lazuliClient}>
        <ThemeProvider>
          <RouterProvider router={router} />
        </ThemeProvider>
      </LazuliProvider>
    </QueryClientProvider>
  );
}
"#;

/// `app/web/shell/layout.tsx` — root layout component.
pub const FRONTEND_WEB_LAYOUT_TSX: &str = r#"import { Outlet } from "@tanstack/react-router";

/**
 * App-shell layout. Wraps every routed view rendered through the
 * router emitted at `dist/ts-web/<audience>/routes.gen.tsx`.
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

/// `app/web/shell/error_boundary.tsx` — top-level React error
/// boundary seed.
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
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("[lazuli] error boundary caught", error, info);
  }

  override render() {
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

/// `app/web/theme/globals.css` — Tailwind layer + base reset.
pub const FRONTEND_THEME_GLOBALS_CSS: &str = r#"@import "@generated/design/tokens.css";

/* User-owned. Lazuli emits the tokens; how you globalize them is yours. */
"#;

/// `app/web/theme/theme_provider.tsx` — light/dark theme context.
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

/// `app/web/tailwind.config.ts` — canonical Tailwind v4 config seed.
pub const FRONTEND_TAILWIND_CONFIG_TS: &str = r#"import { lazuliPreset } from "../../dist/ts-web/design/tailwind.gen";

export default {
  presets: [lazuliPreset],
  content: [
    "./index.html",
    "./main.tsx",
    "./shell/**/*.{ts,tsx}",
    "./routes/**/*.{ts,tsx}",
    "./ui/**/*.{ts,tsx}",
    "./theme/**/*.{ts,tsx}",
    "./state/**/*.{ts,tsx}",
    "./cells/**/*.{ts,tsx}",
    "../../app/**/*.{ts,tsx}",
  ],
};
"#;

/// `app/web/tsconfig.json` — strict-mode TypeScript config with path
/// aliases for the canonical scaffold layout.
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
    "types": ["vite/client", "node"],
    "baseUrl": ".",
    "paths": {
      "@app/*": ["../../app/*"],
      "@generated/*": ["../../dist/ts-web/*"],
      "@web/*": ["./*"],
      "@lazuli/runtime": ["../../vendor/lazuli-runtime/dist/index.d.ts"],
      "@lazuli/runtime/react": ["../../vendor/lazuli-runtime/dist/react.d.ts"],
      "@lazuli/runtime/react/tanstack": ["../../vendor/lazuli-runtime/dist/tanstack-adapter.d.ts"],
      "@lazuli/runtime/react/rhf": ["../../vendor/lazuli-runtime/dist/react-rhf.d.ts"],
      "@lazuli/runtime/formatters": ["../../vendor/lazuli-runtime/dist/formatters.d.ts"]
    }
  },
  "include": [
    ".",
    "../../app/**/*.ts",
    "../../app/**/*.tsx",
    "../../dist/ts-web/**/*.ts",
    "../../dist/ts-web/**/*.tsx"
  ],
  "exclude": [
    "node_modules",
    "../../dist/go",
    "../../vendor",
    "tailwind.config.ts"
  ]
}
"#;

/// `app/web/vite.config.ts` — Vite config seed wired against the
/// canonical scaffold + nativewind plugin guard.
pub const FRONTEND_VITE_CONFIG_TS: &str = r#"import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";

// @lazuli/vite reads Lazurite.toml at config-load time and computes
// the @lazuli/runtime/* + @lazuli/plugin-* alias array on the actual
// host. No absolute filesystem paths in this file — works on CI /
// fresh clones / any OS as long as the canonical sibling layout
// holds (project + lazuli as siblings; see Lazurite.toml [lazuli] path).
import { lazuliAliases } from "@lazuli/vite";

const rootDir = fileURLToPath(new URL(".", import.meta.url));
const projectRoot = path.resolve(rootDir, "../..");

export default defineConfig({
  root: __dirname,
  // .env lives at the Lazurite project root (`projectRoot`) so the
  // backend (Go API) and the frontend (vite) share a single env file.
  envDir: projectRoot,
  // Expose both `VITE_*` and `PUBLIC_*` to the client bundle. `PUBLIC_*`
  // is the canonical shape across Lazuli pilots — same name on the
  // server (Go reads os.Getenv) and the client (import.meta.env), so a
  // value like `PUBLIC_GOOGLE_MAPS_API_KEY` is one var, not two.
  envPrefix: ["VITE_", "PUBLIC_"],
  plugins: [react()],
  resolve: {
    alias: [
      // Lazuli runtime + every @lazuli/plugin-* alias. Spread FIRST so
      // the more-specific subpaths (e.g. @lazuli/runtime/react/tanstack)
      // get matched before any later catch-all entry.
      ...lazuliAliases({ projectRoot }),
      { find: "@app", replacement: path.resolve(projectRoot, "app") },
      { find: "@generated", replacement: path.resolve(projectRoot, "dist/ts-web") },
      { find: "@web", replacement: __dirname },
    ],
  },
  build: {
    outDir: path.resolve(projectRoot, "dist/web"),
    emptyOutDir: true,
  },
});
"#;

/// Web `package.json` template.
///
/// Wave G shipped 2026-05-17 — W1-W7 Tier-2 picks anchored at
/// `docs/proposals/lazurite-frontend-stack-web-grading-2026-05-17.md`
/// (architect re-grade PASS 9.05). See top-of-file comment block for
/// the proposal-pending status discipline.
///
/// W1 state: Zustand. W2 design system: Radix Primitives + Tailwind +
/// Shadcn-compose toolkit (class-variance-authority, clsx,
/// tailwind-merge) seeded copy-paste, NOT a `shadcn-ui` package.
/// W3 icons: Lucide. W4 date: date-fns. W5 toast: Sonner. W6 testing:
/// Vitest + Playwright + React Testing Library. W7 lint/format: Biome
/// (no eslint/prettier).
pub const FRONTEND_PACKAGE_JSON: &str = r#"{
  "name": "lazuli-app-web",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "pnpm -w lazuli:generate && vite",
    "build": "pnpm -w lazuli:generate && vite build",
    "preview": "vite preview",
    "typecheck": "pnpm -w lazuli:generate && tsc --noEmit",
    "test": "vitest run",
    "test:unit": "vitest run",
    "test:e2e": "playwright test",
    "verify:scaffold": "tsc --noEmit && vite build",
    "lint": "biome check .",
    "format": "biome format --write ."
  },
  "dependencies": {
    "@hookform/resolvers": "^3.9.0",
    "@lazuli/runtime": "workspace:*",
    "@radix-ui/react-dialog": "^1.1.0",
    "@radix-ui/react-slot": "^1.1.0",
    "@tanstack/react-query": "^5.51.0",
    "@tanstack/react-router": "^1.45.0",
    "class-variance-authority": "^0.7.0",
    "clsx": "^2.1.0",
    "date-fns": "^3.6.0",
    "lucide-react": "^0.408.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-hook-form": "^7.52.0",
    "search-query-parser": "^1.6.0",
    "sonner": "^1.5.0",
    "tailwind-merge": "^2.5.0",
    "tailwindcss": "^3.4.0",
    "zod": "^3.23.0",
    "zustand": "^4.5.0"
  },
  "devDependencies": {
    "@biomejs/biome": "^1.8.0",
    "@lazuli/vite": "workspace:*",
    "@playwright/test": "^1.46.0",
    "@testing-library/react": "^16.0.0",
    "@testing-library/user-event": "^14.5.0",
    "@types/node": "^22.0.0",
    "@types/react": "^18.3.0",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.0",
    "@vitest/ui": "^2.0.0",
    "jsdom": "^25.0.0",
    "typescript": "^5.5.0",
    "vite": "^5.3.0",
    "vitest": "^2.0.0"
  }
}
"#;

/// Project-root `.gitignore` additions written by the web scaffold
/// (covers `dist/`, `node_modules/`, and the typical
/// editor/build caches).
pub const FRONTEND_GITIGNORE: &str = r#"node_modules/
frontends/*/node_modules/
.vite/
frontends/*/.vite/
dist/
.lazuli/
"#;

/// Placeholder route at `app/web/routes/index.tsx`. User-owned. Lazuli
/// never overwrites; the file is a starter that confirms the route
/// folder is wired into the canonical 6-folder closed catalog per
/// `[[client_src_canonical_architecture_2026-05-17]]` §3.
pub const FRONTEND_WEB_ROUTES_INDEX_TSX: &str = r#"/**
 * Default landing route. User-owned. v0.2 codegen will emit a
 * `routes.gen.tsx` table from `.lzx` `view ... at "<route>"` declarations
 * — until then, the canonical `routes/` folder hosts handcrafted entries.
 */
export default function Index() {
  return <p>Welcome to Lazurite</p>;
}
"#;

/// Placeholder Zustand store at `app/web/state/app_store.ts`. User-
/// owned. W1 pick is Zustand 4.x (see top-of-file Wave G comment).
/// Cross-feature client state lives here; feature-local state stays
/// co-located in `app/features/<f>/cells/`.
pub const FRONTEND_WEB_STATE_APP_STORE_TS: &str = r#"import { create } from "zustand";

/**
 * Cross-feature client state. v0 ships a theme-toggle skeleton; extend
 * with modal stack, command palette, etc. as your app grows. Server
 * state belongs in TanStack Query (DSL-driven); feature-local state
 * stays co-located in `app/features/<f>/cells/`.
 */
interface AppState {
  theme: "light" | "dark";
  setTheme: (theme: "light" | "dark") => void;
}

export const useAppStore = create<AppState>((set) => ({
  theme: "light",
  setTheme: (theme) => set({ theme }),
}));
"#;

/// `app/web/__smoke__/scaffold.smoke.test.tsx` — render smoke emitted
/// by `lazuli new --frontends web`. Mounting `<App/>` inside the live
/// provider tree proves the WHOLE runtime-import surface
/// (`@lazuli/runtime`, `@lazuli/runtime/react`) resolves AND compiles
/// under the client tsconfig + vendored `dist/*.d.ts` paths. This is
/// the test whose ABSENCE let the scaffold ship a red `tsc` gate to two
/// pilots (see spec 0027). User-owned after scaffold; Lazuli never
/// overwrites.
pub const FRONTEND_WEB_SMOKE_TEST_TSX: &str = r#"import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { App } from "@web/shell/root";

// Render smoke: if `@lazuli/runtime` / `@lazuli/runtime/react` did not
// resolve + compile under this client's tsconfig (the defect spec 0027
// fixed), this file would not typecheck and `<App/>` would not mount.
describe("scaffold render smoke", () => {
  it("mounts <App/> with the Lazuli provider tree", () => {
    const { container } = render(<App />);
    expect(container).toBeTruthy();
  });
});
"#;

/// `app/web/__smoke__/generated-sdk.smoke.test.ts` — generated-SDK
/// import smoke emitted by `lazuli new --frontends web`. Generated SDK
/// files (`dist/ts-web/<feature>/<feature>.react.gen.ts`) import their
/// transport + hook factories from `@lazuli/runtime` /
/// `@lazuli/runtime/react`; this smoke imports those same entry symbols
/// directly so the SDK's import surface is proven to resolve under the
/// client tsconfig EVEN BEFORE the first `lazuli generate ts` (when no
/// `@generated/*` file exists yet). Once generated SDK files land,
/// extend this with a concrete `@generated/<feature>` import.
pub const FRONTEND_WEB_SDK_SMOKE_TEST_TS: &str = r#"import { describe, expect, it } from "vitest";

// The exact symbols every generated `*.react.gen.ts` SDK file pulls
// from the runtime. If these don't resolve under the client tsconfig,
// no generated SDK would compile — this smoke catches that wiring
// regression at `tsc --noEmit` time.
import { LazuliClient, defineQuery, defineCommand } from "@lazuli/runtime";
import { useLazuliQuery, useLazuliCommand } from "@lazuli/runtime/react";

describe("generated-SDK import smoke", () => {
  it("resolves the runtime symbols generated SDK files import", () => {
    expect(LazuliClient).toBeTypeOf("function");
    expect(defineQuery).toBeTypeOf("function");
    expect(defineCommand).toBeTypeOf("function");
    expect(useLazuliQuery).toBeTypeOf("function");
    expect(useLazuliCommand).toBeTypeOf("function");
  });
});
"#;
