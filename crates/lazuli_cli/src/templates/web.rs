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
    "./routes/**/*.{ts,tsx}",
    "./ui/**/*.{ts,tsx}",
    "./theme/**/*.{ts,tsx}",
    "./state/**/*.{ts,tsx}",
    "./cells/**/*.{ts,tsx}",
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
    "lint": "biome check .",
    "format": "biome format --write ."
  },
  "dependencies": {
    "@hookform/resolvers": "^3.9.0",
    "@lazuli/runtime": "^0.1.0",
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
    "sonner": "^1.5.0",
    "tailwind-merge": "^2.5.0",
    "tailwindcss": "^3.4.0",
    "zod": "^3.23.0",
    "zustand": "^4.5.0"
  },
  "devDependencies": {
    "@biomejs/biome": "^1.8.0",
    "@lazuli/vite": "^0.1.0",
    "@playwright/test": "^1.46.0",
    "@testing-library/react": "^16.0.0",
    "@testing-library/user-event": "^14.5.0",
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

// ---------------------------------------------------------------
// Wave K — W2 Shadcn-seed primitives shipped 2026-05-17.
//
// Per W2 grading (anchor: docs/proposals/
// lazurite-frontend-stack-web-grading-2026-05-17.md §W2), the design-
// system pick is "Radix Primitives + Tailwind, with Shadcn-compose
// toolkit (CVA + clsx + tailwind-merge) as deps, Shadcn primitives
// copy-pasted into the scaffold as SEED FILES THE PRODUCT OWNS".
//
// The constants below ship the v0 closed set: ONE essential primitive
// per `ui/` kind (6 total) + the `cn()` helper. Each emits with a
// scaffold-seed banner so users know they own the file and Lazuli will
// never overwrite it on re-scaffold (idempotency guard in
// `cmd_new_frontends::write_if_absent`).
//
// Closed-set discipline (do NOT expand here without a fresh grading
// cycle): v0 = 6 primitives. Pilots may need more for v0.2; that's a
// separate proposal.
// ---------------------------------------------------------------

/// `cn()` helper used by every CVA-driven primitive. Standard Shadcn
/// recipe — `clsx` for conditional composition + `tailwind-merge` for
/// last-write-wins conflict resolution between Tailwind utility classes.
/// User-owned scaffold seed.
pub const FRONTEND_WEB_THEME_CN_TS: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Compose Tailwind class strings with conditional support and conflict
 * resolution. `cn("px-2 px-4")` -> `"px-4"`.
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
"#;

/// `ui/forms/Button.tsx` — CVA variants (default/destructive/outline/
/// ghost) + sizes (sm/md/lg/icon) + Radix `Slot` for `asChild`.
pub const FRONTEND_WEB_UI_BUTTON_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { Slot } from "@radix-ui/react-slot";
import { type VariantProps, cva } from "class-variance-authority";
import { forwardRef } from "react";
import type { ButtonHTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        default:
          "bg-primary text-primary-foreground hover:bg-primary/90",
        destructive:
          "bg-destructive text-destructive-foreground hover:bg-destructive/90",
        outline:
          "border border-input bg-background hover:bg-accent hover:text-accent-foreground",
        ghost: "hover:bg-accent hover:text-accent-foreground",
      },
      size: {
        sm: "h-8 px-3",
        md: "h-10 px-4 py-2",
        lg: "h-11 px-8",
        icon: "h-10 w-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "md",
    },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { buttonVariants };
"#;

/// `ui/forms/Input.tsx` — closed-set props mirroring native + Tailwind
/// variants. Refs forwarded.
pub const FRONTEND_WEB_UI_INPUT_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { forwardRef } from "react";
import type { InputHTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

export type InputProps = InputHTMLAttributes<HTMLInputElement>;

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        className={cn(
          "flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
          className,
        )}
        ref={ref}
        {...props}
      />
    );
  },
);
Input.displayName = "Input";
"#;

/// `ui/feedback/Toast.tsx` — Sonner wrapper. Re-exports `Toaster` for
/// shell mount + `toast()` API for call sites.
pub const FRONTEND_WEB_UI_TOAST_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { Toaster as SonnerToaster, toast } from "sonner";
import type { ComponentProps } from "react";

type ToasterProps = ComponentProps<typeof SonnerToaster>;

/**
 * App-shell Toaster. Mount once at the root (e.g. inside `shell/root.tsx`):
 *
 *     <Toaster richColors />
 *
 * Call sites use the re-exported `toast` API:
 *
 *     toast.success("Saved");
 *     toast.error("Something went wrong");
 */
export function Toaster(props: ToasterProps) {
  return <SonnerToaster richColors closeButton {...props} />;
}

export { toast };
"#;

/// `ui/display/Card.tsx` — Card + CardHeader + CardTitle +
/// CardDescription + CardContent + CardFooter sub-components per
/// Shadcn idiom.
pub const FRONTEND_WEB_UI_CARD_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { forwardRef } from "react";
import type { HTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

export const Card = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        "rounded-lg border bg-card text-card-foreground shadow-sm",
        className,
      )}
      {...props}
    />
  ),
);
Card.displayName = "Card";

export const CardHeader = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn("flex flex-col space-y-1.5 p-6", className)}
    {...props}
  />
));
CardHeader.displayName = "CardHeader";

export const CardTitle = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLHeadingElement>
>(({ className, ...props }, ref) => (
  <h3
    ref={ref}
    className={cn(
      "text-2xl font-semibold leading-none tracking-tight",
      className,
    )}
    {...props}
  />
));
CardTitle.displayName = "CardTitle";

export const CardDescription = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLParagraphElement>
>(({ className, ...props }, ref) => (
  <p
    ref={ref}
    className={cn("text-sm text-muted-foreground", className)}
    {...props}
  />
));
CardDescription.displayName = "CardDescription";

export const CardContent = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div ref={ref} className={cn("p-6 pt-0", className)} {...props} />
));
CardContent.displayName = "CardContent";

export const CardFooter = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn("flex items-center p-6 pt-0", className)}
    {...props}
  />
));
CardFooter.displayName = "CardFooter";
"#;

/// `ui/overlays/Dialog.tsx` — Radix `@radix-ui/react-dialog` wrapper.
/// Closed-set sub-components per Shadcn idiom: Dialog, DialogTrigger,
/// DialogContent, DialogHeader, DialogFooter, DialogTitle,
/// DialogDescription, DialogClose.
pub const FRONTEND_WEB_UI_DIALOG_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { forwardRef } from "react";
import type { ComponentPropsWithoutRef, ElementRef, HTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogPortal = DialogPrimitive.Portal;
export const DialogClose = DialogPrimitive.Close;

export const DialogOverlay = forwardRef<
  ElementRef<typeof DialogPrimitive.Overlay>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Overlay
    ref={ref}
    className={cn(
      "fixed inset-0 z-50 bg-black/80 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
      className,
    )}
    {...props}
  />
));
DialogOverlay.displayName = DialogPrimitive.Overlay.displayName;

export const DialogContent = forwardRef<
  ElementRef<typeof DialogPrimitive.Content>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Content>
>(({ className, children, ...props }, ref) => (
  <DialogPortal>
    <DialogOverlay />
    <DialogPrimitive.Content
      ref={ref}
      className={cn(
        "fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border bg-background p-6 shadow-lg duration-200 sm:rounded-lg",
        className,
      )}
      {...props}
    >
      {children}
      <DialogPrimitive.Close className="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none">
        <X className="h-4 w-4" />
        <span className="sr-only">Close</span>
      </DialogPrimitive.Close>
    </DialogPrimitive.Content>
  </DialogPortal>
));
DialogContent.displayName = DialogPrimitive.Content.displayName;

export function DialogHeader({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "flex flex-col space-y-1.5 text-center sm:text-left",
        className,
      )}
      {...props}
    />
  );
}
DialogHeader.displayName = "DialogHeader";

export function DialogFooter({
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "flex flex-col-reverse sm:flex-row sm:justify-end sm:space-x-2",
        className,
      )}
      {...props}
    />
  );
}
DialogFooter.displayName = "DialogFooter";

export const DialogTitle = forwardRef<
  ElementRef<typeof DialogPrimitive.Title>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Title>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Title
    ref={ref}
    className={cn(
      "text-lg font-semibold leading-none tracking-tight",
      className,
    )}
    {...props}
  />
));
DialogTitle.displayName = DialogPrimitive.Title.displayName;

export const DialogDescription = forwardRef<
  ElementRef<typeof DialogPrimitive.Description>,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Description
    ref={ref}
    className={cn("text-sm text-muted-foreground", className)}
    {...props}
  />
));
DialogDescription.displayName = DialogPrimitive.Description.displayName;
"#;

/// `ui/layout/Stack.tsx` — flex layout with gap variants. Pure Tailwind,
/// no Radix needed. Closed-set props: `direction`, `gap`, `align`,
/// `justify`, `wrap`.
pub const FRONTEND_WEB_UI_STACK_TSX: &str = r#"// Scaffold seed (Wave K 2026-05-17). User owns this file. Edit freely.
// Lazuli never overwrites.
import { type VariantProps, cva } from "class-variance-authority";
import { forwardRef } from "react";
import type { HTMLAttributes } from "react";

import { cn } from "@web/theme/cn";

const stackVariants = cva("flex", {
  variants: {
    direction: {
      row: "flex-row",
      col: "flex-col",
    },
    gap: {
      none: "gap-0",
      sm: "gap-2",
      md: "gap-4",
      lg: "gap-6",
      xl: "gap-8",
    },
    align: {
      start: "items-start",
      center: "items-center",
      end: "items-end",
      stretch: "items-stretch",
    },
    justify: {
      start: "justify-start",
      center: "justify-center",
      end: "justify-end",
      between: "justify-between",
      around: "justify-around",
    },
    wrap: {
      true: "flex-wrap",
      false: "flex-nowrap",
    },
  },
  defaultVariants: {
    direction: "col",
    gap: "md",
    align: "stretch",
    justify: "start",
    wrap: false,
  },
});

export interface StackProps
  extends HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof stackVariants> {}

export const Stack = forwardRef<HTMLDivElement, StackProps>(
  ({ className, direction, gap, align, justify, wrap, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        stackVariants({ direction, gap, align, justify, wrap }),
        className,
      )}
      {...props}
    />
  ),
);
Stack.displayName = "Stack";

export { stackVariants };
"#;
