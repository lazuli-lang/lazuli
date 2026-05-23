package lazuli

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strings"
	"sync"
)

// ConstantManifest is the boot-time closed catalog of binding-namespace
// `constants.*` and `secrets.*` names known to the running app. Codegen
// builds the required-name list from `capability constant <name>: <Type>`
// and `capability secret <name>` declarations in registry.lzi (per
// binding-namespace-catalog.md §3.2).
//
// Missing-name panics happen at INSTALL time (boot), not at first use —
// fail-loud-on-deploy beats fail-on-first-request for ops triage.
//
// Two separate APIs (Constant vs Secret) preserve static distinguishability:
// audit / logs / doctor can grep call sites without crossing the
// secret/non-secret boundary.
type ConstantManifest struct {
	constants map[string]string
	secrets   map[string]string
}

var (
	manifestMu sync.RWMutex
	manifest   *ConstantManifest
)

// InstallConstantManifest validates that every required constant + secret
// is populated from the process environment, then registers the manifest
// for the running app. Subsequent Constant / Secret calls read from this
// manifest.
//
// Panics if any required name is missing or empty — the codegen guarantees
// that every BindingPath the IR contains has a corresponding required
// entry, so a missing one means a deploy-time misconfiguration.
//
// Idempotent: calling InstallConstantManifest twice with the same required
// sets is a no-op; calling with different sets panics (the running app's
// IR is frozen post-boot, so the required set should not change).
func InstallConstantManifest(requiredConstants, requiredSecrets []string) *ConstantManifest {
	missing := []string{}
	constants := map[string]string{}
	for _, name := range requiredConstants {
		value := os.Getenv(name)
		if value == "" {
			missing = append(missing, "constants."+name)
			continue
		}
		constants[name] = value
	}
	secrets := map[string]string{}
	for _, name := range requiredSecrets {
		value := os.Getenv(name)
		if value == "" {
			missing = append(missing, "secrets."+name)
			continue
		}
		secrets[name] = value
	}
	if len(missing) > 0 {
		sort.Strings(missing)
		panic(fmt.Sprintf("lazuli: missing required env vars at boot: %s",
			strings.Join(missing, ", ")))
	}

	m := &ConstantManifest{constants: constants, secrets: secrets}

	manifestMu.Lock()
	defer manifestMu.Unlock()
	if manifest != nil {
		if !sameKeys(manifest.constants, constants) || !sameKeys(manifest.secrets, secrets) {
			panic("lazuli: InstallConstantManifest called twice with different required sets")
		}
		return manifest
	}
	manifest = m
	return m
}

// ResolveConstant returns the manifest-resolved value for a non-secret
// configuration name. Panics if the manifest is not installed, or if
// the name was not in the required set passed to InstallConstantManifest.
//
// ctx is reserved for future tracing instrumentation; the current
// implementation does not read from it. Callers MUST pass the request
// ctx so call sites are stable across the manifest evolution.
//
// Naming: the `Resolve` prefix avoids colliding with the existing
// `lazuli.Secret` type alias in types.go used by `@cap.Secret` field
// types. Resolution happens against the boot-time manifest.
func ResolveConstant(_ context.Context, name string) string {
	manifestMu.RLock()
	m := manifest
	manifestMu.RUnlock()
	if m == nil {
		panic("lazuli: ResolveConstant(\"" + name + "\") called before InstallConstantManifest")
	}
	value, ok := m.constants[name]
	if !ok {
		panic("lazuli: ResolveConstant(\"" + name + "\") not in installed manifest — codegen drift?")
	}
	return value
}

// ResolveSecret returns the manifest-resolved secret value for a name.
// The API surface is intentionally distinct from ResolveConstant so
// audit / doctor / future telemetry can distinguish secret-reads from
// constant-reads without parsing arguments.
//
// Implementation is identical to ResolveConstant (env-backed today) —
// the boundary is in the API, not the storage. A future capability
// adapter (`@lazuli/plugin-secrets-vault`, etc.) replaces the storage without
// touching any call site.
//
// Naming: see ResolveConstant. The `Secret` identifier is taken by the
// type alias in types.go; `ResolveSecret` is the function form.
func ResolveSecret(_ context.Context, name string) string {
	manifestMu.RLock()
	m := manifest
	manifestMu.RUnlock()
	if m == nil {
		panic("lazuli: ResolveSecret(\"" + name + "\") called before InstallConstantManifest")
	}
	value, ok := m.secrets[name]
	if !ok {
		panic("lazuli: ResolveSecret(\"" + name + "\") not in installed manifest — codegen drift?")
	}
	return value
}

// resetManifestForTest clears the package-level manifest. Test-only —
// production code must not call this.
func resetManifestForTest() {
	manifestMu.Lock()
	manifest = nil
	manifestMu.Unlock()
}

func sameKeys(a, b map[string]string) bool {
	if len(a) != len(b) {
		return false
	}
	for k := range a {
		if _, ok := b[k]; !ok {
			return false
		}
	}
	return true
}
