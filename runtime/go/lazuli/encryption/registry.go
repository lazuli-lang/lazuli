// Package encryption — registry / resolver for the per-binding cipher
// catalog declared in `app.lzi` via `encryption.key @key.<scope>`.
//
// Generated code at boot time calls `Register(binding)` for every entry
// in `EncryptionBindings` (emitted by `crates/lazuli_codegen_go` into
// `dist/go/lazuli_app.gen.go`). Request-time code calls
// `For(ctx, "@key.<scope>")` to obtain the per-tenant (or per-user, or
// per-record, or global) `*Cipher`.
//
// Wire-thin: this file reads env vars and threads bytes into the
// existing AES-256-GCM primitive from `aes_gcm.go`. Zero homegrown
// crypto.
//
// See `docs/proposals/encryption-vocab.md` §Runtime.

package encryption

import (
	"context"
	"encoding/base64"
	"errors"
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync"
)

// Source — closed catalog of where binding bytes live. Mirrors the IR
// `EncryptionSource` shape.
type Source string

const (
	SourceEnv     Source = "env"
	SourceSecrets Source = "secrets"
)

// TemplateAxis — closed catalog of which ctx axis a binding template
// substitutes. Mirrors the IR `EncryptionTemplateAxis`.
type TemplateAxis string

const (
	AxisTenantID TemplateAxis = "tenant_id"
	AxisUserID   TemplateAxis = "user_id"
	AxisRecordID TemplateAxis = "record_id"
)

// Algorithm — closed catalog of the cipher algorithm. v0: only
// AES-256-GCM. New algorithms wait for pilot pressure.
type Algorithm string

const AlgorithmAES256GCM Algorithm = "aes_256_gcm"

// Rotation — closed catalog of rotation strategy. v0: only manual.
type Rotation string

const RotationManual Rotation = "manual"

// Binding pairs a `@key.<scope>` reference with the resolver
// configuration. Codegen emits one literal per app.lzi
// `encryption.key` declaration; `Register` ingests it at boot.
type Binding struct {
	// `@key.<scope>` reference, verbatim with `@key.` prefix.
	Scope string
	// Where the key bytes live.
	Source Source
	// Literal name with template axes preserved verbatim,
	// e.g. "CRYPT_KEY_TENANT_{tenant_id}".
	Template string
	// Which template axes appear in `Template`.
	Axes []TemplateAxis
	// Algorithm + rotation strategy.
	Algorithm Algorithm
	Rotation  Rotation
}

// Errors returned by the resolver. All map to runtime-internal 500
// (doctor should catch the configuration issue at compile time).
var (
	ErrBindingMissing         = errors.New("encryption: no binding declared for scope (declare it in app.lzi `encryption` block)")
	ErrEnvKeyMissing          = errors.New("encryption: env var for binding is unset or empty")
	ErrKeyDecodeFailed        = errors.New("encryption: env value is not a base64-encoded 32-byte key")
	ErrSecretsAdapterMissing  = errors.New("encryption: secrets-source bindings require `@adapter.secrets` (Wave 2; not shipped)")
	ErrTenantIDMissing        = errors.New("encryption: binding references {tenant_id} but ctx carries no tenant")
	ErrUserIDMissing          = errors.New("encryption: binding references {user_id} but ctx carries no user")
	ErrRecordIDMissing        = errors.New("encryption: binding references {record_id} but no record id was supplied to encryption.For")
)

// registry singleton. Per-process; safe for concurrent reads/writes.
// Codegen `init()` registers bindings before `func main` runs.
type registry struct {
	mu             sync.RWMutex
	bindings       map[string]Binding
	resolvedCipher map[string]*Cipher
}

var defaultRegistry = &registry{
	bindings:       map[string]Binding{},
	resolvedCipher: map[string]*Cipher{},
}

// Register installs a binding into the process-global registry. Called
// from generated `init()` blocks; safe to call multiple times — the
// later registration wins per scope. Idempotent for identical inputs.
func Register(b Binding) {
	defaultRegistry.mu.Lock()
	defer defaultRegistry.mu.Unlock()
	defaultRegistry.bindings[b.Scope] = b
	// Invalidate any cached cipher for this scope so a re-Register picks
	// up the new template/algorithm immediately.
	for k := range defaultRegistry.resolvedCipher {
		if strings.HasPrefix(k, b.Scope+"|") {
			delete(defaultRegistry.resolvedCipher, k)
		}
	}
}

// Reset clears every registered binding. Used by tests; not called
// from generated code.
func Reset() {
	defaultRegistry.mu.Lock()
	defer defaultRegistry.mu.Unlock()
	defaultRegistry.bindings = map[string]Binding{}
	defaultRegistry.resolvedCipher = map[string]*Cipher{}
}

// Bindings returns a snapshot of the registered binding catalog. Order
// is unspecified. Useful for introspection / `lazuli inspect`-style
// tools; runtime code should call `For` instead.
func Bindings() []Binding {
	defaultRegistry.mu.RLock()
	defer defaultRegistry.mu.RUnlock()
	out := make([]Binding, 0, len(defaultRegistry.bindings))
	for _, b := range defaultRegistry.bindings {
		out = append(out, b)
	}
	return out
}

// CtxAxes carries the runtime-supplied template substitutions. The
// caller (generated repo code, etc.) populates it from `lazuli.Ctx`
// (or equivalent) plus, for `@key.record`, the row's id at write/read
// time. Zero-valued axes are treated as missing.
type CtxAxes struct {
	TenantID string
	UserID   string
	RecordID string
}

// For returns the *Cipher resolved for the given `@key.<scope>` against
// the supplied axes. Caches per resolved-template so repeated calls
// within the same tenant/user/record reuse the same cipher (key
// derivation is paid once per (scope, resolved template) pair per
// process).
//
// The Lazuli runtime exposes an overload via the package-level
// `ForCtx(ctx, scope)` that lifts axes from `lazuli.Ctx` when the
// generated code does not need to override record_id.
func For(scope string, axes CtxAxes) (*Cipher, error) {
	defaultRegistry.mu.RLock()
	b, ok := defaultRegistry.bindings[scope]
	defaultRegistry.mu.RUnlock()
	if !ok {
		return nil, fmt.Errorf("%w: scope=%s", ErrBindingMissing, scope)
	}

	resolvedName, err := resolveTemplate(b.Template, b.Axes, axes)
	if err != nil {
		return nil, err
	}
	cacheKey := scope + "|" + resolvedName

	defaultRegistry.mu.RLock()
	cached, hit := defaultRegistry.resolvedCipher[cacheKey]
	defaultRegistry.mu.RUnlock()
	if hit {
		return cached, nil
	}

	keyBytes, err := readKeyBytes(b.Source, resolvedName)
	if err != nil {
		return nil, err
	}
	cipher, err := NewCipher(keyBytes)
	if err != nil {
		return nil, err
	}

	defaultRegistry.mu.Lock()
	defaultRegistry.resolvedCipher[cacheKey] = cipher
	defaultRegistry.mu.Unlock()
	return cipher, nil
}

// ForCtx is the request-time helper that lifts the tenant axis from a
// context value. The Lazuli `lazuli.Ctx` shape stamps Tenant/User onto
// `context.Context` via embedding, but this package keeps a small
// adapter so the encryption package doesn't import `lazuli`
// (avoiding a cycle).
//
// `recordID` is the value substituted for `{record_id}` templates;
// callers without a record id pass `""`.
func ForCtx(ctx context.Context, scope, recordID string) (*Cipher, error) {
	return For(scope, CtxAxes{
		TenantID: TenantIDFromContext(ctx),
		UserID:   UserIDFromContext(ctx),
		RecordID: recordID,
	})
}

// TenantIDFromContext / UserIDFromContext — minimal shim. The Lazuli
// `lazuli` package wires `Ctx.Tenant.OrgID` / `Ctx.User.ID` here via
// the `RegisterCtxAxes` hook so the encryption package stays
// dependency-free. Generated code typically calls `ForCtx`; tests
// pass `For(scope, CtxAxes{...})` directly.
type ctxAxesProvider func(ctx context.Context) (tenantID, userID string)

var ctxAxesHook ctxAxesProvider = func(_ context.Context) (string, string) { return "", "" }

// RegisterCtxAxes installs the runtime hook that reads tenant/user
// identity from a `context.Context`. The Lazuli runtime calls this
// from its boot code; tests may override the hook for isolation.
func RegisterCtxAxes(provider ctxAxesProvider) {
	if provider == nil {
		return
	}
	ctxAxesHook = provider
}

func TenantIDFromContext(ctx context.Context) string {
	tenantID, _ := ctxAxesHook(ctx)
	return tenantID
}

func UserIDFromContext(ctx context.Context) string {
	_, userID := ctxAxesHook(ctx)
	return userID
}

// resolveTemplate substitutes every `{axis}` brace expression in the
// template literal with the matching value from `CtxAxes`. Unknown
// axes are left verbatim (the catalog walk validated them at
// parse-time; doctor will have refused unknown axes before runtime).
func resolveTemplate(template string, axes []TemplateAxis, supplied CtxAxes) (string, error) {
	out := template
	for _, axis := range axes {
		brace := "{" + string(axis) + "}"
		value := axisValue(axis, supplied)
		if value == "" {
			return "", missingAxisError(axis)
		}
		out = strings.ReplaceAll(out, brace, value)
	}
	return out, nil
}

func axisValue(axis TemplateAxis, supplied CtxAxes) string {
	switch axis {
	case AxisTenantID:
		return supplied.TenantID
	case AxisUserID:
		return supplied.UserID
	case AxisRecordID:
		return supplied.RecordID
	}
	return ""
}

func missingAxisError(axis TemplateAxis) error {
	switch axis {
	case AxisTenantID:
		return ErrTenantIDMissing
	case AxisUserID:
		return ErrUserIDMissing
	case AxisRecordID:
		return ErrRecordIDMissing
	}
	return fmt.Errorf("encryption: unknown axis %q in template", axis)
}

func readKeyBytes(source Source, resolvedName string) ([]byte, error) {
	switch source {
	case SourceEnv:
		raw := strings.TrimSpace(os.Getenv(resolvedName))
		if raw == "" {
			return nil, fmt.Errorf("%w: name=%s", ErrEnvKeyMissing, resolvedName)
		}
		decoded, err := base64.StdEncoding.DecodeString(raw)
		if err != nil {
			return nil, fmt.Errorf("%w: name=%s err=%v", ErrKeyDecodeFailed, resolvedName, err)
		}
		if len(decoded) != 32 {
			return nil, fmt.Errorf("%w: name=%s len=%d", ErrKeyDecodeFailed, resolvedName, len(decoded))
		}
		return decoded, nil
	case SourceSecrets:
		return nil, ErrSecretsAdapterMissing
	}
	return nil, fmt.Errorf("encryption: unknown source %q", source)
}

// formatUint is a small helper that callers wiring `lazuli.Ctx.Tenant.OrgID`
// (an integer ID) into `RegisterCtxAxes` can use to stringify axis
// values uniformly. Keeps the runtime hook minimal.
func FormatID(value uint64) string {
	return strconv.FormatUint(value, 10)
}
