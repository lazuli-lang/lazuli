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

// RefreshHandler is the default `/auth/refresh` command handler wired by
// the auth-refresh codegen when `auth.sessions.rotation` is enabled.
// Stubbed until the per-request refresh-token extraction wire lands
// (tracked as LAZ-84). Override by registering a different handler under
// `<feature>.auth.refresh` in user code.
func RefreshHandler(_ *lazuli.Ctx, _ any) (any, error) {
	return nil, errors.New("auth: RefreshHandler not yet wired (LAZ-84); per-request refresh-token extraction pending")
}
