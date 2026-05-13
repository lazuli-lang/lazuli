package lazuli

import (
	"errors"
	"net/http"
	"testing"
	"time"
)

func TestHTTPServerTimeoutProfileByNameReturnsCanonicalProfiles(t *testing.T) {
	tests := []struct {
		name     string
		wantName string
	}{
		{name: " default ", wantName: HTTPServerTimeoutProfileDefault},
		{name: "API", wantName: HTTPServerTimeoutProfileDefault},
		{name: "edge", wantName: HTTPServerTimeoutProfileStrict},
		{name: "long-polling", wantName: HTTPServerTimeoutProfileStreaming},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			profile, ok := HTTPServerTimeoutProfileByName(tt.name)
			if !ok {
				t.Fatal("HTTPServerTimeoutProfileByName returned ok=false")
			}
			if profile.Name != tt.wantName {
				t.Fatalf("Name = %q, want %q", profile.Name, tt.wantName)
			}
			if err := profile.Validate(); err != nil {
				t.Fatalf("profile Validate returned %v, want nil", err)
			}
		})
	}
}

func TestHTTPServerTimeoutProfilesReturnsIndependentSlice(t *testing.T) {
	profiles := HTTPServerTimeoutProfiles()
	profiles[0].Name = "mutated"

	next := HTTPServerTimeoutProfiles()

	if next[0].Name != HTTPServerTimeoutProfileDefault {
		t.Fatalf("profile name = %q, want %q", next[0].Name, HTTPServerTimeoutProfileDefault)
	}
}

func TestDefaultHTTPServerTimeoutProfileMatchesServerDefaults(t *testing.T) {
	profile := DefaultHTTPServerTimeoutProfile()

	if profile.Name != HTTPServerTimeoutProfileDefault {
		t.Fatalf("Name = %q, want %q", profile.Name, HTTPServerTimeoutProfileDefault)
	}
	if profile.ReadTimeout != defaultReadTimeout {
		t.Fatalf("ReadTimeout = %s, want %s", profile.ReadTimeout, defaultReadTimeout)
	}
	if profile.ReadHeaderTimeout != defaultReadHeaderTimeout {
		t.Fatalf("ReadHeaderTimeout = %s, want %s", profile.ReadHeaderTimeout, defaultReadHeaderTimeout)
	}
	if profile.WriteTimeout != defaultWriteTimeout {
		t.Fatalf("WriteTimeout = %s, want %s", profile.WriteTimeout, defaultWriteTimeout)
	}
	if profile.IdleTimeout != defaultIdleTimeout {
		t.Fatalf("IdleTimeout = %s, want %s", profile.IdleTimeout, defaultIdleTimeout)
	}
	if !profile.HTTP2.Enabled {
		t.Fatal("HTTP2.Enabled = false, want true")
	}
}

func TestApplyHTTPServerTimeoutProfileAppliesTimeoutsAndHTTP2(t *testing.T) {
	profile := DefaultHTTPServerTimeoutProfile()
	server := &http.Server{}

	if err := ApplyHTTPServerTimeoutProfile(server, profile); err != nil {
		t.Fatalf("ApplyHTTPServerTimeoutProfile returned %v, want nil", err)
	}

	if server.ReadTimeout != profile.ReadTimeout {
		t.Fatalf("ReadTimeout = %s, want %s", server.ReadTimeout, profile.ReadTimeout)
	}
	if server.ReadHeaderTimeout != profile.ReadHeaderTimeout {
		t.Fatalf("ReadHeaderTimeout = %s, want %s", server.ReadHeaderTimeout, profile.ReadHeaderTimeout)
	}
	if server.WriteTimeout != profile.WriteTimeout {
		t.Fatalf("WriteTimeout = %s, want %s", server.WriteTimeout, profile.WriteTimeout)
	}
	if server.IdleTimeout != profile.IdleTimeout {
		t.Fatalf("IdleTimeout = %s, want %s", server.IdleTimeout, profile.IdleTimeout)
	}
	if server.HTTP2 == nil {
		t.Fatal("server.HTTP2 = nil, want configured policy")
	}
	if server.HTTP2.MaxConcurrentStreams != defaultHTTP2MaxConcurrentStreams {
		t.Fatalf("MaxConcurrentStreams = %d, want %d", server.HTTP2.MaxConcurrentStreams, defaultHTTP2MaxConcurrentStreams)
	}
	if server.Protocols == nil || !server.Protocols.HTTP1() || !server.Protocols.HTTP2() {
		t.Fatalf("Protocols = %v, want HTTP/1 and HTTP/2 enabled", server.Protocols)
	}
	if server.Protocols.UnencryptedHTTP2() {
		t.Fatal("UnencryptedHTTP2 = true, want false")
	}
}

func TestServerWithTimeoutProfileClonesServer(t *testing.T) {
	handler := http.NewServeMux()
	base := &http.Server{
		Addr:        ":9090",
		Handler:     handler,
		ReadTimeout: time.Hour,
	}

	configured, err := ServerWithTimeoutProfile(base, DefaultHTTPServerTimeoutProfile())
	if err != nil {
		t.Fatalf("ServerWithTimeoutProfile returned %v, want nil", err)
	}

	if configured == base {
		t.Fatal("ServerWithTimeoutProfile returned the input server")
	}
	if configured.Addr != base.Addr {
		t.Fatalf("Addr = %q, want %q", configured.Addr, base.Addr)
	}
	if configured.Handler != handler {
		t.Fatalf("Handler = %v, want configured handler", configured.Handler)
	}
	if configured.ReadTimeout != defaultReadTimeout {
		t.Fatalf("configured ReadTimeout = %s, want %s", configured.ReadTimeout, defaultReadTimeout)
	}
	if base.ReadTimeout != time.Hour {
		t.Fatalf("base ReadTimeout = %s, want unchanged", base.ReadTimeout)
	}
}

func TestValidateHTTPServerTimeouts(t *testing.T) {
	validStreaming := HTTPServerTimeouts{
		KeepAlivesEnabled: true,
		ReadHeaderTimeout: time.Second,
		IdleTimeout:       time.Minute,
	}
	if err := ValidateHTTPServerTimeouts(validStreaming); err != nil {
		t.Fatalf("ValidateHTTPServerTimeouts streaming returned %v, want nil", err)
	}

	tests := []struct {
		name     string
		timeouts HTTPServerTimeouts
	}{
		{
			name: "negative read timeout",
			timeouts: HTTPServerTimeouts{
				ReadTimeout:       -time.Second,
				ReadHeaderTimeout: time.Second,
			},
		},
		{
			name: "missing read header timeout",
			timeouts: HTTPServerTimeouts{
				ReadTimeout: time.Second,
			},
		},
		{
			name: "negative write timeout",
			timeouts: HTTPServerTimeouts{
				ReadHeaderTimeout: time.Second,
				WriteTimeout:      -time.Second,
			},
		},
		{
			name: "negative idle timeout",
			timeouts: HTTPServerTimeouts{
				ReadHeaderTimeout: time.Second,
				IdleTimeout:       -time.Second,
			},
		},
		{
			name: "missing keep-alive idle timeout",
			timeouts: HTTPServerTimeouts{
				KeepAlivesEnabled: true,
				ReadHeaderTimeout: time.Second,
			},
		},
		{
			name: "read timeout shorter than headers",
			timeouts: HTTPServerTimeouts{
				ReadTimeout:       time.Second,
				ReadHeaderTimeout: 2 * time.Second,
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateHTTPServerTimeouts(tt.timeouts)
			if !errors.Is(err, ErrHTTPServerTimeoutsInvalid) {
				t.Fatalf("ValidateHTTPServerTimeouts error = %v, want %v", err, ErrHTTPServerTimeoutsInvalid)
			}
		})
	}
}

func TestValidateHTTP2Policy(t *testing.T) {
	valid := DefaultHTTP2Policy()
	valid.Unencrypted = true
	valid.MaxReadFrameSize = minHTTP2ReadFrameSize
	valid.MaxReceiveBufferPerConnection = minHTTP2ReceiveBufferPerConnection
	if err := ValidateHTTP2Policy(valid); err != nil {
		t.Fatalf("ValidateHTTP2Policy returned %v, want nil", err)
	}

	tests := []struct {
		name   string
		policy HTTP2Policy
	}{
		{name: "unencrypted disabled", policy: HTTP2Policy{Unencrypted: true}},
		{name: "negative streams", policy: HTTP2Policy{Enabled: true, MaxConcurrentStreams: -1}},
		{name: "decoder table too large", policy: HTTP2Policy{Enabled: true, MaxDecoderHeaderTableSize: maxHTTP2HeaderTableSize}},
		{name: "read frame too small", policy: HTTP2Policy{Enabled: true, MaxReadFrameSize: minHTTP2ReadFrameSize - 1}},
		{name: "connection buffer too small", policy: HTTP2Policy{Enabled: true, MaxReceiveBufferPerConnection: minHTTP2ReceiveBufferPerConnection - 1}},
		{name: "stream buffer too large", policy: HTTP2Policy{Enabled: true, MaxReceiveBufferPerStream: maxHTTP2ReceiveBufferPerStream}},
		{name: "negative ping timeout", policy: HTTP2Policy{Enabled: true, PingTimeout: -time.Second}},
		{name: "negative write byte timeout", policy: HTTP2Policy{Enabled: true, WriteByteTimeout: -time.Second}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateHTTP2Policy(tt.policy)
			if !errors.Is(err, ErrHTTP2PolicyInvalid) {
				t.Fatalf("ValidateHTTP2Policy error = %v, want %v", err, ErrHTTP2PolicyInvalid)
			}
		})
	}
}

func TestApplyHTTP2PolicyDisablesHTTP2(t *testing.T) {
	server := &http.Server{}
	if err := ApplyHTTP2Policy(server, DefaultHTTP2Policy()); err != nil {
		t.Fatalf("ApplyHTTP2Policy enable returned %v, want nil", err)
	}

	if err := ApplyHTTP2Policy(server, HTTP2Policy{}); err != nil {
		t.Fatalf("ApplyHTTP2Policy disable returned %v, want nil", err)
	}

	if server.HTTP2 != nil {
		t.Fatalf("server.HTTP2 = %#v, want nil", server.HTTP2)
	}
	if server.Protocols == nil || !server.Protocols.HTTP1() {
		t.Fatalf("Protocols = %v, want HTTP/1 enabled", server.Protocols)
	}
	if server.Protocols.HTTP2() || server.Protocols.UnencryptedHTTP2() {
		t.Fatalf("Protocols = %v, want HTTP/2 disabled", server.Protocols)
	}
}

func TestApplyHTTP2PolicyEnablesUnencryptedHTTP2(t *testing.T) {
	server := &http.Server{}
	policy := DefaultHTTP2Policy()
	policy.Unencrypted = true

	if err := ApplyHTTP2Policy(server, policy); err != nil {
		t.Fatalf("ApplyHTTP2Policy returned %v, want nil", err)
	}

	if server.Protocols == nil || !server.Protocols.HTTP2() || !server.Protocols.UnencryptedHTTP2() {
		t.Fatalf("Protocols = %v, want HTTP/2 and unencrypted HTTP/2 enabled", server.Protocols)
	}
}
