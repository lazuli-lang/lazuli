package realtime

import (
	"crypto/sha1"
	"encoding/base64"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"strings"
)

const webSocketAcceptGUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

const (
	headerConnection           = "Connection"
	headerHost                 = "Host"
	headerOrigin               = "Origin"
	headerUpgrade              = "Upgrade"
	headerSecWebSocketKey      = "Sec-WebSocket-Key"
	headerSecWebSocketProtocol = "Sec-WebSocket-Protocol"
	headerSecWebSocketVersion  = "Sec-WebSocket-Version"
)

const (
	// WebSocketCloseNormalClosure indicates that the purpose for which the
	// connection was established has been fulfilled.
	WebSocketCloseNormalClosure WebSocketCloseCode = 1000
	// WebSocketCloseGoingAway indicates that an endpoint is going away.
	WebSocketCloseGoingAway WebSocketCloseCode = 1001
	// WebSocketCloseProtocolError indicates that an endpoint is terminating
	// the connection due to a protocol error.
	WebSocketCloseProtocolError WebSocketCloseCode = 1002
	// WebSocketCloseUnsupportedData indicates that an endpoint received a type
	// of data it cannot accept.
	WebSocketCloseUnsupportedData WebSocketCloseCode = 1003
	// WebSocketCloseNoStatusReceived is reserved and means no status code was
	// present when one was expected.
	WebSocketCloseNoStatusReceived WebSocketCloseCode = 1005
	// WebSocketCloseAbnormalClosure is reserved and means the connection closed
	// without a close control frame.
	WebSocketCloseAbnormalClosure WebSocketCloseCode = 1006
	// WebSocketCloseInvalidFramePayloadData indicates invalid frame payload
	// data, such as non-UTF-8 text.
	WebSocketCloseInvalidFramePayloadData WebSocketCloseCode = 1007
	// WebSocketClosePolicyViolation indicates that a message violated endpoint
	// policy.
	WebSocketClosePolicyViolation WebSocketCloseCode = 1008
	// WebSocketCloseMessageTooBig indicates that a message was too large to
	// process.
	WebSocketCloseMessageTooBig WebSocketCloseCode = 1009
	// WebSocketCloseMandatoryExtension indicates that the client expected an
	// extension the server did not negotiate.
	WebSocketCloseMandatoryExtension WebSocketCloseCode = 1010
	// WebSocketCloseInternalServerError indicates that the server encountered
	// an unexpected condition.
	WebSocketCloseInternalServerError WebSocketCloseCode = 1011
	// WebSocketCloseTLSHandshake is reserved and means a TLS handshake failed.
	WebSocketCloseTLSHandshake WebSocketCloseCode = 1015
)

var (
	// ErrInvalidWebSocketHandshake is returned when an HTTP request is not a
	// valid RFC 6455 opening handshake.
	ErrInvalidWebSocketHandshake = errors.New("lazuli/realtime: invalid websocket handshake")
)

// WebSocketCloseCode is an RFC 6455 WebSocket close status code.
type WebSocketCloseCode int

// WebSocketHandshakeRequest is the validated RFC 6455 opening handshake data
// needed by a transport adapter.
type WebSocketHandshakeRequest struct {
	// Key is the client's Sec-WebSocket-Key value.
	Key string
	// Accept is the Sec-WebSocket-Accept value derived from Key.
	Accept string
	// Origin is the trimmed Origin header value, if the client supplied one.
	Origin string
	// RequestedSubprotocols are the client requested Sec-WebSocket-Protocol
	// values in client preference order.
	RequestedSubprotocols []string
}

// WebSocketOriginPolicy matches request origins against an allow list.
//
// AllowedOrigins accepts exact origins, wildcard subdomains such as
// "https://*.example.com", "null", or "*" for any syntactically valid origin.
// Wildcard subdomains do not match their apex domain.
type WebSocketOriginPolicy struct {
	AllowedOrigins []string
	AllowMissing   bool
}

// SecWebSocketAccept returns the RFC 6455 Sec-WebSocket-Accept value for key.
func SecWebSocketAccept(key string) string {
	sum := sha1.Sum([]byte(strings.TrimSpace(key) + webSocketAcceptGUID))
	return base64.StdEncoding.EncodeToString(sum[:])
}

// ValidateWebSocketHandshakeRequest validates the RFC 6455 HTTP opening
// handshake fields and returns the parsed values needed to write a 101 response.
func ValidateWebSocketHandshakeRequest(r *http.Request) (WebSocketHandshakeRequest, error) {
	if r == nil {
		return WebSocketHandshakeRequest{}, fmt.Errorf("%w: request is nil", ErrInvalidWebSocketHandshake)
	}
	if r.Method != http.MethodGet {
		return WebSocketHandshakeRequest{}, fmt.Errorf("%w: method must be GET", ErrInvalidWebSocketHandshake)
	}
	if r.ProtoMajor != 1 || r.ProtoMinor < 1 {
		return WebSocketHandshakeRequest{}, fmt.Errorf("%w: HTTP protocol must be 1.1", ErrInvalidWebSocketHandshake)
	}
	if strings.TrimSpace(r.Host) == "" {
		return WebSocketHandshakeRequest{}, fmt.Errorf("%w: %s header required", ErrInvalidWebSocketHandshake, headerHost)
	}
	if !headerTokenContains(r.Header, headerUpgrade, "websocket") {
		return WebSocketHandshakeRequest{}, fmt.Errorf("%w: %s header must include websocket", ErrInvalidWebSocketHandshake, headerUpgrade)
	}
	if !headerTokenContains(r.Header, headerConnection, "upgrade") {
		return WebSocketHandshakeRequest{}, fmt.Errorf("%w: %s header must include Upgrade", ErrInvalidWebSocketHandshake, headerConnection)
	}

	key, ok := singleHeaderValue(r.Header, headerSecWebSocketKey)
	if !ok || !validWebSocketKey(key) {
		return WebSocketHandshakeRequest{}, fmt.Errorf("%w: %s header must be a 16-byte base64 nonce", ErrInvalidWebSocketHandshake, headerSecWebSocketKey)
	}
	version, ok := singleHeaderValue(r.Header, headerSecWebSocketVersion)
	if !ok || version != "13" {
		return WebSocketHandshakeRequest{}, fmt.Errorf("%w: %s header must be 13", ErrInvalidWebSocketHandshake, headerSecWebSocketVersion)
	}

	requested, ok := webSocketSubprotocols(r.Header)
	if !ok {
		return WebSocketHandshakeRequest{}, fmt.Errorf("%w: %s header contains an invalid token", ErrInvalidWebSocketHandshake, headerSecWebSocketProtocol)
	}

	return WebSocketHandshakeRequest{
		Key:                   key,
		Accept:                SecWebSocketAccept(key),
		Origin:                strings.TrimSpace(r.Header.Get(headerOrigin)),
		RequestedSubprotocols: requested,
	}, nil
}

// Allows reports whether origin matches the policy.
func (p WebSocketOriginPolicy) Allows(origin string) bool {
	origin = strings.TrimSpace(origin)
	if origin == "" {
		return p.AllowMissing
	}

	canonical, ok := canonicalWebSocketOrigin(origin)
	if !ok {
		return false
	}

	for _, allowed := range p.AllowedOrigins {
		allowed = strings.TrimSpace(allowed)
		if allowed == "" {
			continue
		}
		if allowed == "*" {
			return true
		}
		if canonicalAllowed, ok := canonicalWebSocketOrigin(allowed); ok && canonicalAllowed == canonical {
			return true
		}
		if wildcard, ok := parseWebSocketWildcardOrigin(allowed); ok && wildcard.matches(canonical) {
			return true
		}
	}
	return false
}

// SelectWebSocketSubprotocol returns the first requested subprotocol also
// present in supported. Matching is case-sensitive.
func SelectWebSocketSubprotocol(requested []string, supported []string) string {
	if len(requested) == 0 || len(supported) == 0 {
		return ""
	}

	supportedSet := make(map[string]struct{}, len(supported))
	for _, protocol := range supported {
		if isHTTPToken(protocol) {
			supportedSet[protocol] = struct{}{}
		}
	}
	for _, protocol := range requested {
		if _, ok := supportedSet[protocol]; ok {
			return protocol
		}
	}
	return ""
}

func validWebSocketKey(key string) bool {
	decoded, err := base64.StdEncoding.DecodeString(key)
	return err == nil && len(decoded) == 16
}

func singleHeaderValue(header http.Header, name string) (string, bool) {
	values := header.Values(name)
	if len(values) != 1 {
		return "", false
	}
	value := strings.TrimSpace(values[0])
	if value == "" || strings.Contains(value, ",") {
		return "", false
	}
	return value, true
}

func headerTokenContains(header http.Header, name string, token string) bool {
	for _, value := range header.Values(name) {
		for _, part := range strings.Split(value, ",") {
			if strings.EqualFold(strings.TrimSpace(part), token) {
				return true
			}
		}
	}
	return false
}

func webSocketSubprotocols(header http.Header) ([]string, bool) {
	var protocols []string
	for _, value := range header.Values(headerSecWebSocketProtocol) {
		for _, part := range strings.Split(value, ",") {
			protocol := strings.TrimSpace(part)
			if protocol == "" {
				return nil, false
			}
			if !isHTTPToken(protocol) {
				return nil, false
			}
			protocols = append(protocols, protocol)
		}
	}
	return protocols, true
}

func isHTTPToken(value string) bool {
	if value == "" {
		return false
	}
	for _, r := range value {
		if r > 127 || !isHTTPTokenRune(byte(r)) {
			return false
		}
	}
	return true
}

func isHTTPTokenRune(b byte) bool {
	switch {
	case b >= '0' && b <= '9':
		return true
	case b >= 'A' && b <= 'Z':
		return true
	case b >= 'a' && b <= 'z':
		return true
	default:
		switch b {
		case '!', '#', '$', '%', '&', '\'', '*', '+', '-', '.', '^', '_', '`', '|', '~':
			return true
		default:
			return false
		}
	}
}

type webSocketWildcardOrigin struct {
	scheme string
	suffix string
	port   string
}

func (w webSocketWildcardOrigin) matches(origin string) bool {
	u, err := url.Parse(origin)
	if err != nil {
		return false
	}
	host := strings.ToLower(u.Hostname())
	return strings.ToLower(u.Scheme) == w.scheme &&
		u.Port() == w.port &&
		len(host) > len(w.suffix) &&
		strings.HasSuffix(host, w.suffix)
}

func canonicalWebSocketOrigin(origin string) (string, bool) {
	origin = strings.TrimSpace(origin)
	if origin == "null" {
		return origin, true
	}

	u, err := url.Parse(origin)
	if err != nil ||
		u.Scheme == "" ||
		u.Host == "" ||
		u.User != nil ||
		u.Path != "" ||
		u.RawQuery != "" ||
		u.Fragment != "" {
		return "", false
	}
	if host := u.Hostname(); host == "" || strings.Contains(host, "*") {
		return "", false
	}

	return strings.ToLower(u.Scheme) + "://" + strings.ToLower(u.Host), true
}

func parseWebSocketWildcardOrigin(origin string) (webSocketWildcardOrigin, bool) {
	u, err := url.Parse(strings.TrimSpace(origin))
	if err != nil ||
		u.Scheme == "" ||
		u.Host == "" ||
		u.User != nil ||
		u.Path != "" ||
		u.RawQuery != "" ||
		u.Fragment != "" {
		return webSocketWildcardOrigin{}, false
	}

	host := strings.ToLower(u.Hostname())
	if !strings.HasPrefix(host, "*.") || strings.Count(host, "*") != 1 {
		return webSocketWildcardOrigin{}, false
	}

	suffix := strings.TrimPrefix(host, "*")
	if suffix == "." {
		return webSocketWildcardOrigin{}, false
	}

	return webSocketWildcardOrigin{
		scheme: strings.ToLower(u.Scheme),
		suffix: suffix,
		port:   u.Port(),
	}, true
}
