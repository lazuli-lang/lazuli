package auth

import (
	"errors"

	"lazuli.dev/runtime/lazuli"
)

// LoginHandler is the default `/auth/login` handler wired by the
// auth-bucket codegen. The user-side `command login` body provides the
// real implementation through `@fn.login`; the runtime ships a stub so
// generated code compiles before the user has supplied a handler.
//
// Override by passing a different handler in the generated
// `Api.Handler` slot, or by overriding the auth route registration in
// the per-feature `auth.gen.go`.
func LoginHandler(_ *lazuli.Ctx, _ any) (any, error) {
	return nil, errors.New("auth: LoginHandler not implemented; provide @fn.login")
}

// SignupHandler is the default `/auth/signup` handler wired by the
// auth-bucket codegen. See LoginHandler for override semantics.
func SignupHandler(_ *lazuli.Ctx, _ any) (any, error) {
	return nil, errors.New("auth: SignupHandler not implemented; provide @fn.signup")
}

// LogoutHandler is the default `/auth/logout` handler wired by the
// auth-bucket codegen. See LoginHandler for override semantics.
func LogoutHandler(_ *lazuli.Ctx, _ any) (any, error) {
	return nil, errors.New("auth: LogoutHandler not implemented; provide @fn.logout")
}

// RefreshInput is the empty body expected by the auth.refresh command.
// The refresh token rides on a cookie / header, not in the request body.
type RefreshInput struct{}

// RefreshOutput is the JSON payload returned by RefreshHandler. The
// new access token is included in the body so the TS SDK's
// auto-refresh interceptor can pick it up (the refresh token rides on
// the HttpOnly cookie the runtime sets via Ctx.SetRefreshCookie).
type RefreshOutput struct {
	Access string `json:"access"`
}

// RefreshHandler is the default `<feature>.auth.refresh` command
// handler wired by the auth-refresh codegen when
// `auth.sessions.rotation` is enabled. Reads the long-lived refresh
// token from `ctx.RefreshToken` (populated by the production middleware
// from the `lazuli_refresh` cookie or `X-Lazuli-Refresh` header), calls
// `RotateSession` to issue a new access+refresh pair, sets the new
// refresh cookie, and returns the new access token in the response
// body. Override by registering a different handler under
// `<feature>.auth.refresh` in user code.
func RefreshHandler(ctx *lazuli.Ctx, _ RefreshInput) (RefreshOutput, error) {
	if ctx == nil || ctx.RefreshToken == "" {
		return RefreshOutput{}, refreshError(lazuli.CodeRefreshInvalid, ErrRefreshInvalid)
	}
	access, refresh, err := RotateSession(ctx, ctx.RefreshToken)
	if err != nil {
		// Theft detection forces session-family logout — clear the
		// caller's refresh cookie so the next request lands anonymous.
		if errors.Is(err, ErrTheftDetected) {
			ctx.ClearRefreshCookie()
		}
		return RefreshOutput{}, err
	}
	ctx.SetRefreshCookie(refresh, 0)
	return RefreshOutput{Access: access}, nil
}
