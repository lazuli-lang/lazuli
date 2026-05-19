package lazuli

import (
	"embed"

	"lazuli.dev/runtime/lazuli/i18n"
)

// Re-exports of the error-vocab contract types so codegen-emitted Go
// (`dist/go/<feature>/errors.gen.go`, `dist/go/<feature>/command.gen.go`,
// `dist/go/app/error_resolution.gen.go`) can reference everything as
// `lazuli.<Name>` without depending on the `i18n` subpackage path.
// Implementation lives under `lazuli/i18n` to avoid an import cycle
// with the resolver; these aliases are the public surface authors and
// codegen consume. Proposal §4 §5.1.

// ErrorKeys binds per-command translation keys per resolution-chain
// layer 1 (proposal §2.A). Codegen emits a literal of this type for
// every command declaring `policy ... when_denied @translation.<key>`.
type ErrorKeys = i18n.ErrorKeys

// FeatureErrorContract is the lowered `feature.errors` block per
// resolution-chain layer 3 (proposal §2.C). Codegen emits one literal
// per feature with an `errors` block.
type FeatureErrorContract = i18n.FeatureErrorContract

// ErrorExposureDefault is the default exposure setting for a feature's
// error wire payload. Mirrors `ir::ErrorExposureDefault`.
type ErrorExposureDefault = i18n.ErrorExposureDefault

// AppErrorResolverRegistry is the app-level map from feature name to
// `FeatureErrorContract`. Held process-globally; populated by repeated
// `RegisterFeatureErrors` calls from generated `init()` blocks.
type AppErrorResolverRegistry = i18n.AppErrorResolverRegistry

// ExposureHide is the default: only fields listed in
// `expose client 4xx <fields>` reach the wire. See §2.G.
const ExposureHide = i18n.ExposureHide

// ExposureExpose is the dev-mode-friendly default: every envelope
// field is exposed unless explicitly suppressed.
const ExposureExpose = i18n.ExposureExpose

// RegisterFeatureErrors installs a feature's error-vocab contract into
// the process-global registry. Codegen's `dist/go/app/error_resolution
// .gen.go` calls this once per feature at boot (proposal §4.1.3). The
// resolver merges all registered contracts with the built-in PT-BR /
// en-US catalog before serving requests.
//
// Safe to call repeatedly — each invocation overwrites the prior entry
// for the same feature. A nil registry map is lazily allocated.
//
// DB-INTEGRITY-CATALOG-EXT (2026-05-19): the contract is now also
// mirrored into the process-global resolver's Registry so the L2
// feature-level catch-all (proposal §2.E step 3) actually fires for
// codes the runtime emits without a per-command override. Without
// this mirror, db-integrity codes like `unique_violation` would always
// fall through to the L3 builtin floor, dropping authored
// feature-level overrides (e.g. `errors unique_violation message
// @translation.account_email_already_registered`) on the floor.
func RegisterFeatureErrors(feature string, contract FeatureErrorContract) {
	if appErrorRegistry.Features == nil {
		appErrorRegistry.Features = map[string]FeatureErrorContract{}
	}
	appErrorRegistry.Features[feature] = contract

	// Mirror into the resolver's own Registry so the L2 walk fires.
	if r, ok := i18n.Default().(*i18n.DefaultResolver); ok {
		if r.Registry.Features == nil {
			r.Registry.Features = map[string]FeatureErrorContract{}
		}
		r.Registry.Features[feature] = contract
	}
}

// RegisterFeatureTranslationCatalog merges a feature's authored
// `translation` block (lowered by codegen into per-locale JSON files
// embedded in `fs`) into the process-global default resolver's
// `Catalogs` map. Codegen's `<feature>/translation.gen.go` calls this
// once at boot per feature (Wave 3.5). Without it the authored
// per-feature keys never reach the resolver's L1/L2 layers and the
// chain falls through to the built-in L3 floor — that's the gap this
// re-export closes.
//
// Catalog naming convention: `<basePath>/<feature>.<locale>.json`.
// Bare JSON keys get qualified to `<feature>.<bare_key>` before
// insertion so `MessageRef.Qualified()` lookups hit the inserted text.
// Re-registering the same feature replaces its prior entries.
//
// Errors propagate from JSON parse / FS read failures so the codegen-
// emitted init() can `panic` and fail the build loudly when a catalog
// file is malformed.
func RegisterFeatureTranslationCatalog(feature string, fs embed.FS, basePath string) error {
	return i18n.RegisterFeatureTranslationCatalog(feature, fs, basePath)
}
