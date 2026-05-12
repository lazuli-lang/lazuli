package auth

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"errors"

	"golang.org/x/oauth2"
	"golang.org/x/oauth2/google"

	"lazuli.dev/runtime/lazuli"
)

// OAuthContract is the lowered `auth oauth <provider>` block. One
// instance is emitted per declared provider. The runtime resolves
// `AdapterRef` against the boot-time adapter registry to get a
// concrete oauth2 client.
type OAuthContract struct {
	// Provider is the closed-catalog provider id (`google`, `github`,
	// `microsoft`, `apple` in v0).
	Provider string
	// AdapterRef is the `@adapter.<name>` reference the language
	// emits; the runtime resolves it at boot.
	AdapterRef string
	// ClientID / ClientSecret are populated by the adapter at boot
	// from environment / secret manager. The contract carries them
	// because the runtime needs them to build *oauth2.Config; the
	// language never names secrets directly.
	ClientID     string
	ClientSecret string
	// RedirectURL is the callback URL the IdP must redirect back to
	// after consent. Populated from `urls.web` + the route the
	// codegen mounts for the callback.
	RedirectURL string
	// Scopes are the OAuth2 scopes requested at consent time.
	Scopes []string
}

// Typed errors returned by the oauth capability. Mapped to
// `expose client` HTTP status codes:
//
//	ErrOAuthStateMismatch         → 400 auth.oauth_state_invalid
//	ErrOAuthAdapterUnregistered   → 500 auth.oauth_adapter_unbound
var (
	ErrOAuthStateMismatch       = errors.New("auth: oauth state mismatch")
	ErrOAuthAdapterUnregistered = errors.New("auth: oauth adapter unregistered")
	ErrOAuthProviderUnknown     = errors.New("auth: oauth provider unknown")
)

// providerEndpoint returns the `oauth2.Endpoint` for a closed-catalog
// provider. Built-in providers (`google` in v0) wire the canonical
// endpoint from `golang.org/x/oauth2/<provider>`; everything else
// must be supplied via an adapter that overrides `OAuthEndpoint`.
func providerEndpoint(provider string) (oauth2.Endpoint, error) {
	switch provider {
	case "google":
		return google.Endpoint, nil
	default:
		return oauth2.Endpoint{}, ErrOAuthProviderUnknown
	}
}

// BuildOAuthConfig assembles the `*oauth2.Config` from the contract.
// Exported so adapters can stack additional configuration (e.g.
// custom endpoints for `@plugin/...` providers) without
// re-implementing the wiring.
func BuildOAuthConfig(contract OAuthContract) (*oauth2.Config, error) {
	endpoint, err := providerEndpoint(contract.Provider)
	if err != nil {
		return nil, err
	}
	return &oauth2.Config{
		ClientID:     contract.ClientID,
		ClientSecret: contract.ClientSecret,
		RedirectURL:  contract.RedirectURL,
		Scopes:       contract.Scopes,
		Endpoint:     endpoint,
	}, nil
}

// GenerateOAuthState returns a URL-safe base64 state token. 32 bytes
// of `crypto/rand` entropy is the OAuth2 / PKCE recommended size.
// Exported so the transport layer (which owns the cookie that
// persists the state) can mint and stash the token alongside
// rendering the redirect URL from OAuthRedirect.
func GenerateOAuthState() (string, error) {
	buf := make([]byte, 32)
	if _, err := rand.Read(buf); err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(buf), nil
}

// OAuthRedirect returns the redirect URL the transport layer must
// send the user agent to. The transport mints a state token via
// GenerateOAuthState, persists it in a same-site cookie, and embeds
// it via `OAuthRedirectWithState` if it needs a single call. The
// canonical Ctx-aware path mints the state inline and stashes it in
// the Ctx for the transport to read.
func OAuthRedirect(ctx *lazuli.Ctx, contract OAuthContract) (string, error) {
	cfg, err := BuildOAuthConfig(contract)
	if err != nil {
		return "", err
	}
	state, err := GenerateOAuthState()
	if err != nil {
		return "", err
	}
	StashOAuthState(ctx, contract.Provider, state)
	return cfg.AuthCodeURL(state, oauth2.AccessTypeOnline), nil
}

// OAuthCallback validates the state cookie, exchanges the code with
// the provider, and issues a session bound to the matching identity.
// Returns the cookie value the transport layer must set.
//
// The `state` argument is the value pulled from the inbound request
// (typically a query param); the trusted "expected" state set by
// OAuthRedirect comes from `ctx` (the transport propagates it from
// the cookie). When the trusted value is missing the call fails with
// `ErrOAuthStateMismatch`.
//
// Issuing the session cookie sits with IssueSession (session.go)
// once a DB-backed session adapter lands; for now this returns the
// access token marshalled as the placeholder cookie value so callers
// can wire end-to-end tests against a stubbed IdP.
func OAuthCallback(
	ctx *lazuli.Ctx,
	contract OAuthContract,
	sessions SessionsContract,
	code, state string,
) (string, error) {
	_ = sessions
	expected := LoadOAuthState(ctx, contract.Provider)
	if expected == "" || state == "" || state != expected {
		return "", ErrOAuthStateMismatch
	}
	cfg, err := BuildOAuthConfig(contract)
	if err != nil {
		return "", err
	}
	token, err := cfg.Exchange(ctxOrBackground(ctx), code)
	if err != nil {
		return "", err
	}
	// Placeholder cookie value: real session issuance lands with
	// IssueSession once the db bucket wires pgx. Until then we
	// return the access token so the transport can roundtrip.
	return token.AccessToken, nil
}

// ctxOrBackground returns the embedded context.Context, falling back
// to context.Background() when the *lazuli.Ctx is nil or its embedded
// Context is unset (e.g. in tests that build Ctx directly).
func ctxOrBackground(ctx *lazuli.Ctx) context.Context {
	if ctx != nil && ctx.Context != nil {
		return ctx.Context
	}
	return context.Background()
}

// oauthStateKey is the context-value key the transport uses to stash
// the state token between OAuthRedirect (mint) and OAuthCallback
// (verify). The key type stays package-private; callers go through
// StashOAuthState / LoadOAuthState rather than poking at the Ctx
// directly.
type oauthStateKey struct{ Provider string }

// StashOAuthState persists `state` in `ctx` keyed by provider. The
// canonical web flow uses this to thread the state token from
// cookie → Ctx before calling OAuthCallback. Mutates `ctx.Context`
// because the Ctx is a request-scoped pointer; callers must not
// share the same Ctx across requests.
func StashOAuthState(ctx *lazuli.Ctx, provider, state string) {
	if ctx == nil {
		return
	}
	parent := ctx.Context
	if parent == nil {
		parent = context.Background()
	}
	ctx.Context = context.WithValue(parent, oauthStateKey{Provider: provider}, state)
}

// LoadOAuthState returns the state token previously stashed in `ctx`
// for `provider`. Returns "" when no state was stashed; OAuthCallback
// treats an empty trusted state as `ErrOAuthStateMismatch`.
func LoadOAuthState(ctx *lazuli.Ctx, provider string) string {
	if ctx == nil || ctx.Context == nil {
		return ""
	}
	v, _ := ctx.Context.Value(oauthStateKey{Provider: provider}).(string)
	return v
}
