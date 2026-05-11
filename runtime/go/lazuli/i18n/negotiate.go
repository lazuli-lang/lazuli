package i18n

import (
	"net/http"
	"strings"
)

// NegotiateAcceptLanguage parses the `Accept-Language` header and
// returns the best-matching tag from the contract's supported list.
// Falls back to `LocaleContract.Default` when no header is present or
// no supported tag matches. This is the RFC 4647 "Basic Filtering"
// algorithm, simplified.
//
// Production runtimes plug in `golang.org/x/text/language` for the
// full CLDR-aware matcher; this stub keeps the dependency surface
// small and demonstrates the contract.
func NegotiateAcceptLanguage(contract LocaleContract, header string) string {
	if header == "" {
		return contract.Default
	}
	// Parse `pt-BR,pt;q=0.9,en;q=0.5` into ordered candidates.
	for _, part := range strings.Split(header, ",") {
		tag := strings.TrimSpace(strings.SplitN(part, ";", 2)[0])
		if tag == "" {
			continue
		}
		if contract.IsSupported(tag) {
			return tag
		}
		// Try prefix matching: `en` matches `en-US` etc.
		for _, s := range contract.Supported {
			if strings.HasPrefix(s, tag+"-") || strings.HasPrefix(tag, s+"-") {
				return s
			}
		}
	}
	// Walk fallbacks for the highest-priority requested tag.
	first := strings.TrimSpace(strings.SplitN(strings.SplitN(header, ",", 2)[0], ";", 2)[0])
	if first != "" {
		return contract.Resolve(first)
	}
	return contract.Default
}

// Middleware mounts the negotiation step in front of `next`. It
// reads `Accept-Language`, resolves the tag, and stashes the result
// in the request context under the key returned by `LocaleContextKey`.
func Middleware(contract LocaleContract, next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		tag := NegotiateAcceptLanguage(contract, r.Header.Get("Accept-Language"))
		ctx := WithLocale(r.Context(), tag)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}
