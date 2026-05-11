package auth

import (
	"errors"

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
}

// Typed errors returned by the oauth capability. Mapped to
// `expose client` HTTP status codes:
//
//	ErrOAuthStateMismatch         → 400 auth.oauth_state_invalid
//	ErrOAuthAdapterUnregistered   → 500 auth.oauth_adapter_unbound
var (
	ErrOAuthStateMismatch       = errors.New("auth: oauth state mismatch")
	ErrOAuthAdapterUnregistered = errors.New("auth: oauth adapter unregistered")
)

// OAuthRedirect returns the redirect URL the transport layer must
// send the user agent to. The adapter generates the state cookie +
// PKCE challenge as appropriate.
func OAuthRedirect(ctx *lazuli.Ctx, contract OAuthContract) (string, error) {
	_ = ctx
	_ = contract
	return "", errors.New("auth: OAuthRedirect not yet implemented")
}

// OAuthCallback validates the state cookie, exchanges the code with
// the provider, and issues a session bound to the matching identity.
// Returns the cookie value the transport layer must set.
func OAuthCallback(
	ctx *lazuli.Ctx,
	contract OAuthContract,
	sessions SessionsContract,
	code, state string,
) (string, error) {
	_ = ctx
	_ = contract
	_ = sessions
	_ = code
	_ = state
	return "", errors.New("auth: OAuthCallback not yet implemented")
}
