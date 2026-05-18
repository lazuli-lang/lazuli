package lazuli

import "lazuli.dev/runtime/lazuli/i18n"

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
func RegisterFeatureErrors(feature string, contract FeatureErrorContract) {
	if appErrorRegistry.Features == nil {
		appErrorRegistry.Features = map[string]FeatureErrorContract{}
	}
	appErrorRegistry.Features[feature] = contract
}
