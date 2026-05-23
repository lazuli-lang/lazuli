/**
 * `@lazuli/vite` — vite alias resolver for Lazuli-driven apps.
 *
 * Reads `Lazurite.toml` at vite-config load time and computes the
 * alias array consumers spread into `resolve.alias`. Eliminates
 * absolute filesystem paths (`c:/Users/...`) from `vite.config.ts`
 * — every alias resolves on the actual host at config-load time,
 * so CI / cross-developer / cross-OS / prod builds work identically.
 *
 * Replaces the codegen-emitted `dist/ts-web/lazurite.vite.mjs`
 * bundle. That approach worked but created a backwards dependency
 * (source config importing from a build artifact) and a chicken-
 * and-egg on fresh checkouts (`pnpm dev` failed until
 * `lazuli generate ts .` had run at least once). This package
 * resolves the same data on demand.
 *
 * Usage:
 *
 * ```ts
 * // vite.config.ts
 * import { lazuliAliases } from '@lazuli/vite';
 * import { resolve } from 'node:path';
 * import { fileURLToPath } from 'node:url';
 *
 * const rootDir = fileURLToPath(new URL('.', import.meta.url));
 *
 * export default defineConfig({
 *   resolve: {
 *     alias: [
 *       ...lazuliAliases({ projectRoot: resolve(rootDir, '../../..') }),
 *       // ... other aliases
 *     ],
 *   },
 * });
 * ```
 */

import { existsSync, readFileSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import { parse as parseToml } from 'smol-toml';

export interface Alias {
  find: string;
  replacement: string;
}

export interface LazuliAliasesOptions {
  /**
   * Absolute path to the directory containing `Lazurite.toml`. The
   * caller is responsible for computing this from
   * `import.meta.url` — `@lazuli/vite` never guesses, so a typo'd
   * `projectRoot` fails loudly instead of silently emitting bad
   * aliases.
   */
  projectRoot: string;
}

/**
 * Compute the alias array for the project at `projectRoot`. Mirrors
 * the Rust emitter at `crates/lazuli_codegen_ts/src/vite_aliases.rs`
 * (the codegen path that previously produced
 * `dist/ts-web/lazurite.vite.mjs`).
 *
 * Returns an empty array when `Lazurite.toml` declares neither
 * `[lazuli] path` nor any `[plugins]`. Throws when `projectRoot`
 * is not absolute (callers must resolve relative paths themselves).
 */
export function lazuliAliases(opts: LazuliAliasesOptions): Alias[] {
  const root = opts.projectRoot;
  if (!isAbsolute(root)) {
    throw new Error(
      `@lazuli/vite: projectRoot must be an absolute path; got ${JSON.stringify(root)}. ` +
        'Resolve from import.meta.url in your vite.config (e.g. `resolve(fileURLToPath(new URL(".", import.meta.url)), "../../..")`).'
    );
  }
  const manifestPath = resolve(root, 'Lazurite.toml');
  if (!existsSync(manifestPath)) {
    throw new Error(
      `@lazuli/vite: Lazurite.toml not found at ${manifestPath}. ` +
        'Check the projectRoot option.'
    );
  }
  const manifest = parseToml(readFileSync(manifestPath, 'utf-8')) as LazuriteManifest;

  const aliases: Alias[] = [];

  // Lazuli runtime aliases — only when `[lazuli] path = "..."` is
  // declared. Apps consuming `@lazuli/runtime` via npm (no local
  // source checkout) skip this block and rely on regular package
  // resolution. Order matters: vite array-form alias matches in
  // declaration order, so the most-specific prefix must precede
  // the bare `@lazuli/runtime` mapping.
  const lazuliPath = manifest.lazuli?.path;
  if (lazuliPath) {
    const runtimeSrc = resolve(root, lazuliPath, 'runtime/ts/lazuli/src');
    aliases.push(
      { find: '@lazuli/runtime/react/tanstack', replacement: resolve(runtimeSrc, 'tanstack-adapter.ts') },
      { find: '@lazuli/runtime/react/rhf', replacement: resolve(runtimeSrc, 'react-rhf.ts') },
      { find: '@lazuli/runtime/react', replacement: resolve(runtimeSrc, 'react.web.ts') },
      { find: '@lazuli/runtime', replacement: resolve(runtimeSrc, 'index.ts') }
    );
  }

  // Plugin aliases — one entry per declared plugin (Lazurite.toml
  // `[plugins]`). `dev.plugin_paths` overrides take precedence over
  // the plugin's base `path`/`module`. Plugins declared as remote
  // (`module = "..."`) without a local override are skipped — they
  // resolve via normal npm package resolution.
  const plugins = manifest.plugins ?? {};
  const devOverrides = manifest.dev?.plugin_paths ?? {};
  const namespaces = Object.keys(plugins).sort();
  for (const namespace of namespaces) {
    const entry = plugins[namespace]!;
    const overridePath = devOverrides[namespace];
    const pluginPath = overridePath ?? localPluginPath(entry);
    if (!pluginPath) continue;
    const pluginRoot = resolve(root, pluginPath);
    const srcDir = resolve(pluginRoot, 'src');

    // Subpath conventions FIRST (e.g. `@lazuli/plugin-viacep/react`
    // before the bare `@lazuli/plugin-viacep`) so vite's prefix-match
    // picks the most-specific entry. The list comes from each plugin's
    // package.json `exports`, with a known-namespace fallback for
    // plugins that haven't declared exports yet.
    for (const sub of pluginSubpaths(pluginRoot, namespace)) {
      aliases.push({
        find: `${namespace}/${sub}`,
        replacement: resolve(srcDir, `${sub}.ts`),
      });
    }
    aliases.push({
      find: namespace,
      replacement: resolve(srcDir, 'index.ts'),
    });
  }

  return aliases;
}

/**
 * Enumerate subpath exports the plugin advertises. Prefers the
 * plugin's `package.json` `exports` field (npm-standard) and falls
 * back to a small hardcoded convention table for plugins that
 * haven't published their subpaths there yet. Mirrors the Rust
 * emitter's `plugin_subpath_conventions` table for parity.
 */
function pluginSubpaths(pluginRoot: string, namespace: string): string[] {
  const pkgPath = resolve(pluginRoot, 'package.json');
  if (existsSync(pkgPath)) {
    try {
      const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8')) as {
        exports?: Record<string, unknown>;
      };
      if (pkg.exports && typeof pkg.exports === 'object') {
        const subs = Object.keys(pkg.exports)
          .filter((k) => k.startsWith('./') && k !== './')
          .map((k) => k.slice(2));
        if (subs.length > 0) return subs;
      }
    } catch {
      // package.json malformed — fall through to convention table.
    }
  }
  return knownSubpathConventions(namespace);
}

function knownSubpathConventions(namespace: string): string[] {
  switch (namespace) {
    case '@lazuli/plugin-viacep':
      return ['react'];
    default:
      return [];
  }
}

function localPluginPath(entry: PluginEntry): string | null {
  if (typeof entry === 'string') return entry;
  if (entry && typeof entry === 'object' && typeof entry.path === 'string') {
    return entry.path;
  }
  return null;
}

// Narrow types for the slice of Lazurite.toml we read. Everything is
// optional because plugins-less / lazuli-less projects are valid (the
// resolver simply returns an empty array).
interface LazuriteManifest {
  lazuli?: { path?: string };
  plugins?: Record<string, PluginEntry>;
  dev?: { plugin_paths?: Record<string, string> };
}

type PluginEntry = string | { path?: string; module?: string; version?: string };
