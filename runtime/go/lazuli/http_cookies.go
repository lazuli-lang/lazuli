package lazuli

import (
	"errors"
	"net/http"
	"time"
)

// ErrCookieMissing is returned by GetCookie when the request has no cookie
// with the requested name.
var ErrCookieMissing = errors.New("lazuli: cookie missing")

// CookieOpts controls the cookie attributes used by SetCookie.
type CookieOpts struct {
	TTL      time.Duration
	Path     string
	Domain   string
	AllowJS  bool
	Secure   bool
	SameSite http.SameSite
}

// SameSiteOrDefault returns the configured SameSite value, or Strict when no
// value was configured.
func (o CookieOpts) SameSiteOrDefault() http.SameSite {
	if o.SameSite == 0 {
		return http.SameSiteStrictMode
	}
	return o.SameSite
}

// SetCookie sets a Lazuli session-style cookie: HttpOnly, Secure when opts
// requests it, and SameSite=Strict by default.
//
//	lazuli.SetCookie(w, "session", token, lazuli.CookieOpts{TTL: 7 * 24 * time.Hour})
func SetCookie(w http.ResponseWriter, name, value string, opts CookieOpts) {
	cookie := &http.Cookie{
		Name:     name,
		Value:    value,
		Path:     stringOr(opts.Path, "/"),
		Domain:   opts.Domain,
		Expires:  time.Now().Add(opts.TTL),
		MaxAge:   int(opts.TTL.Seconds()),
		HttpOnly: !opts.AllowJS,
		Secure:   opts.Secure,
		SameSite: opts.SameSiteOrDefault(),
	}
	http.SetCookie(w, cookie)
}

// GetCookie reads the named cookie or returns "" and ErrCookieMissing.
func GetCookie(r *http.Request, name string) (string, error) {
	cookie, err := r.Cookie(name)
	if errors.Is(err, http.ErrNoCookie) {
		return "", ErrCookieMissing
	}
	if err != nil {
		return "", err
	}
	return cookie.Value, nil
}

// DeleteCookie sets MaxAge=-1 so the browser drops the default-path cookie.
func DeleteCookie(w http.ResponseWriter, name string) {
	http.SetCookie(w, &http.Cookie{
		Name:     name,
		Value:    "",
		Path:     "/",
		MaxAge:   -1,
		Expires:  time.Unix(0, 0),
		HttpOnly: true,
		SameSite: http.SameSiteStrictMode,
	})
}

func stringOr(value, fallback string) string {
	if value == "" {
		return fallback
	}
	return value
}
