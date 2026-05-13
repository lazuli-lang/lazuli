package realtime

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"reflect"
	"testing"
)

func TestSecWebSocketAcceptUsesRFCExample(t *testing.T) {
	t.Parallel()

	got := SecWebSocketAccept("dGhlIHNhbXBsZSBub25jZQ==")
	if got != "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=" {
		t.Fatalf("SecWebSocketAccept() = %q, want RFC example", got)
	}
}

func TestValidateWebSocketHandshakeRequest(t *testing.T) {
	t.Parallel()

	req := newValidWebSocketRequest()
	req.Header.Set(headerOrigin, " https://APP.example.com ")
	req.Header.Add(headerSecWebSocketProtocol, "chat, superchat")
	req.Header.Add(headerSecWebSocketProtocol, "updates.v1")

	got, err := ValidateWebSocketHandshakeRequest(req)
	if err != nil {
		t.Fatalf("ValidateWebSocketHandshakeRequest() error = %v", err)
	}
	if got.Key != "dGhlIHNhbXBsZSBub25jZQ==" {
		t.Fatalf("Key = %q, want request key", got.Key)
	}
	if got.Accept != "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=" {
		t.Fatalf("Accept = %q, want RFC example", got.Accept)
	}
	if got.Origin != "https://APP.example.com" {
		t.Fatalf("Origin = %q, want trimmed origin", got.Origin)
	}
	wantProtocols := []string{"chat", "superchat", "updates.v1"}
	if !reflect.DeepEqual(got.RequestedSubprotocols, wantProtocols) {
		t.Fatalf("RequestedSubprotocols = %v, want %v", got.RequestedSubprotocols, wantProtocols)
	}
}

func TestValidateWebSocketHandshakeRequestRejectsInvalidRequests(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		mutate func(*http.Request)
	}{
		{
			name: "wrong method",
			mutate: func(r *http.Request) {
				r.Method = http.MethodPost
			},
		},
		{
			name: "old protocol",
			mutate: func(r *http.Request) {
				r.ProtoMajor = 1
				r.ProtoMinor = 0
			},
		},
		{
			name: "missing host",
			mutate: func(r *http.Request) {
				r.Host = ""
			},
		},
		{
			name: "missing upgrade",
			mutate: func(r *http.Request) {
				r.Header.Del(headerUpgrade)
			},
		},
		{
			name: "missing connection upgrade token",
			mutate: func(r *http.Request) {
				r.Header.Set(headerConnection, "keep-alive")
			},
		},
		{
			name: "bad key",
			mutate: func(r *http.Request) {
				r.Header.Set(headerSecWebSocketKey, "not-a-16-byte-key")
			},
		},
		{
			name: "duplicate key",
			mutate: func(r *http.Request) {
				r.Header.Add(headerSecWebSocketKey, "dGhlIHNhbXBsZSBub25jZQ==")
			},
		},
		{
			name: "wrong version",
			mutate: func(r *http.Request) {
				r.Header.Set(headerSecWebSocketVersion, "12")
			},
		},
		{
			name: "bad subprotocol token",
			mutate: func(r *http.Request) {
				r.Header.Set(headerSecWebSocketProtocol, "chat, bad protocol")
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			req := newValidWebSocketRequest()
			tt.mutate(req)

			if _, err := ValidateWebSocketHandshakeRequest(req); !errors.Is(err, ErrInvalidWebSocketHandshake) {
				t.Fatalf("ValidateWebSocketHandshakeRequest() error = %v, want ErrInvalidWebSocketHandshake", err)
			}
		})
	}
}

func TestValidateWebSocketHandshakeRequestRejectsNilRequest(t *testing.T) {
	t.Parallel()

	if _, err := ValidateWebSocketHandshakeRequest(nil); !errors.Is(err, ErrInvalidWebSocketHandshake) {
		t.Fatalf("ValidateWebSocketHandshakeRequest(nil) error = %v, want ErrInvalidWebSocketHandshake", err)
	}
}

func TestWebSocketOriginPolicyAllowsConfiguredOrigins(t *testing.T) {
	t.Parallel()

	policy := WebSocketOriginPolicy{
		AllowedOrigins: []string{
			"https://app.example.com",
			"https://*.tenant.example.com",
			"https://*.with-port.example.com:8443",
			"null",
		},
	}

	allowed := []string{
		"https://APP.example.com",
		"https://api.tenant.example.com",
		"https://api.with-port.example.com:8443",
		"null",
	}
	for _, origin := range allowed {
		if !policy.Allows(origin) {
			t.Fatalf("Allows(%q) = false, want true", origin)
		}
	}

	denied := []string{
		"",
		"https://tenant.example.com",
		"https://api.with-port.example.com",
		"https://evil.example.com",
		"https://app.example.com/path",
	}
	for _, origin := range denied {
		if policy.Allows(origin) {
			t.Fatalf("Allows(%q) = true, want false", origin)
		}
	}
}

func TestWebSocketOriginPolicyAllowsWildcardAndMissing(t *testing.T) {
	t.Parallel()

	policy := WebSocketOriginPolicy{
		AllowedOrigins: []string{"*"},
		AllowMissing:   true,
	}

	if !policy.Allows("https://any.example.com") {
		t.Fatal("Allows(valid origin) = false, want true")
	}
	if !policy.Allows("") {
		t.Fatal("Allows(empty origin) = false, want true")
	}
	if policy.Allows("://not an origin") {
		t.Fatal("Allows(invalid origin) = true, want false")
	}
}

func TestSelectWebSocketSubprotocol(t *testing.T) {
	t.Parallel()

	requested := []string{"updates.v2", "updates.v1", "chat"}
	supported := []string{"chat", "updates.v1"}
	if got := SelectWebSocketSubprotocol(requested, supported); got != "updates.v1" {
		t.Fatalf("SelectWebSocketSubprotocol() = %q, want client-preferred match", got)
	}
	if got := SelectWebSocketSubprotocol([]string{"Chat"}, []string{"chat"}); got != "" {
		t.Fatalf("SelectWebSocketSubprotocol() = %q, want no case-insensitive match", got)
	}
	if got := SelectWebSocketSubprotocol([]string{"chat"}, []string{"bad protocol", "chat"}); got != "chat" {
		t.Fatalf("SelectWebSocketSubprotocol() = %q, want valid supported protocol", got)
	}
}

func TestWebSocketCloseCodeConstants(t *testing.T) {
	t.Parallel()

	tests := []struct {
		code WebSocketCloseCode
		want int
	}{
		{WebSocketCloseNormalClosure, 1000},
		{WebSocketCloseGoingAway, 1001},
		{WebSocketCloseProtocolError, 1002},
		{WebSocketCloseUnsupportedData, 1003},
		{WebSocketCloseNoStatusReceived, 1005},
		{WebSocketCloseAbnormalClosure, 1006},
		{WebSocketCloseInvalidFramePayloadData, 1007},
		{WebSocketClosePolicyViolation, 1008},
		{WebSocketCloseMessageTooBig, 1009},
		{WebSocketCloseMandatoryExtension, 1010},
		{WebSocketCloseInternalServerError, 1011},
		{WebSocketCloseTLSHandshake, 1015},
	}

	for _, tt := range tests {
		if int(tt.code) != tt.want {
			t.Fatalf("close code = %d, want %d", tt.code, tt.want)
		}
	}
}

func newValidWebSocketRequest() *http.Request {
	req := httptest.NewRequest(http.MethodGet, "http://example.com/realtime", nil)
	req.Header.Set(headerUpgrade, "websocket")
	req.Header.Set(headerConnection, "keep-alive, Upgrade")
	req.Header.Set(headerSecWebSocketKey, "dGhlIHNhbXBsZSBub25jZQ==")
	req.Header.Set(headerSecWebSocketVersion, "13")
	return req
}
