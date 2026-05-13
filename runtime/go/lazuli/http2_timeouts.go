package lazuli

import (
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"
)

const (
	// HTTPServerTimeoutProfileDefault is the canonical Lazuli HTTP server
	// timeout profile for API servers.
	HTTPServerTimeoutProfileDefault = "default"
	// HTTPServerTimeoutProfileStrict tightens request and idle budgets for
	// edge-facing endpoints.
	HTTPServerTimeoutProfileStrict = "strict"
	// HTTPServerTimeoutProfileStreaming keeps header and idle protection while
	// leaving long-lived response bodies to handler-level controls.
	HTTPServerTimeoutProfileStreaming = "streaming"
)

const (
	defaultHTTP2MaxConcurrentStreams = 128
	defaultHTTP2SendPingTimeout      = 30 * time.Second
	defaultHTTP2PingTimeout          = 15 * time.Second
	defaultHTTP2WriteByteTimeout     = 15 * time.Second

	minHTTP2ReadFrameSize              = 16 << 10
	maxHTTP2ReadFrameSize              = 16 << 20
	maxHTTP2HeaderTableSize            = 4 << 20
	minHTTP2ReceiveBufferPerConnection = 64 << 10
	maxHTTP2ReceiveBufferPerConnection = 4 << 20
	maxHTTP2ReceiveBufferPerStream     = 4 << 20
)

var (
	// ErrHTTPServerTimeoutsInvalid is wrapped when server timeout validation
	// fails.
	ErrHTTPServerTimeoutsInvalid = errors.New("lazuli/http: invalid server timeouts")
	// ErrHTTP2PolicyInvalid is wrapped when HTTP/2 policy validation fails.
	ErrHTTP2PolicyInvalid = errors.New("lazuli/http: invalid http2 policy")
)

// HTTPServerTimeouts contains the http.Server timeout fields Lazuli treats as
// a coherent server profile.
type HTTPServerTimeouts struct {
	// KeepAlivesEnabled controls http.Server keep-alive state when the profile
	// is applied.
	KeepAlivesEnabled bool
	// ReadTimeout is the maximum duration for reading the entire request.
	ReadTimeout time.Duration
	// ReadHeaderTimeout is the maximum duration for reading request headers.
	ReadHeaderTimeout time.Duration
	// WriteTimeout is the maximum duration before timing out response writes.
	WriteTimeout time.Duration
	// IdleTimeout is the maximum time to wait for the next request on an idle
	// keep-alive connection.
	IdleTimeout time.Duration
}

// HTTP2Policy describes the standard-library HTTP/2 settings Lazuli can apply
// to an http.Server.
type HTTP2Policy struct {
	// Enabled controls TLS HTTP/2 support through http.Server.Protocols.
	Enabled bool
	// Unencrypted permits h2c HTTP/2 on cleartext connections. It is disabled
	// by Lazuli profiles unless callers opt in explicitly.
	Unencrypted bool
	// MaxConcurrentStreams limits concurrent HTTP/2 streams per client.
	MaxConcurrentStreams int
	// MaxDecoderHeaderTableSize caps the peer's header decoder table.
	MaxDecoderHeaderTableSize int
	// MaxEncoderHeaderTableSize caps the local header encoder table.
	MaxEncoderHeaderTableSize int
	// MaxReadFrameSize limits the largest HTTP/2 frame this endpoint reads.
	MaxReadFrameSize int
	// MaxReceiveBufferPerConnection caps per-connection flow-control receive
	// buffering.
	MaxReceiveBufferPerConnection int
	// MaxReceiveBufferPerStream caps per-stream flow-control receive buffering.
	MaxReceiveBufferPerStream int
	// SendPingTimeout is the idle period before sending an HTTP/2 health ping.
	SendPingTimeout time.Duration
	// PingTimeout closes a connection when a health ping is not acknowledged.
	PingTimeout time.Duration
	// WriteByteTimeout closes a connection when queued data cannot be written.
	WriteByteTimeout time.Duration
}

// HTTPServerTimeoutProfile combines named server timeouts with HTTP/2 policy
// metadata. Name and Description are informational for custom profiles.
type HTTPServerTimeoutProfile struct {
	Name        string
	Description string
	HTTPServerTimeouts
	HTTP2 HTTP2Policy
}

// DefaultHTTP2Policy returns Lazuli's standard HTTP/2 server policy.
func DefaultHTTP2Policy() HTTP2Policy {
	return HTTP2Policy{
		Enabled:              true,
		MaxConcurrentStreams: defaultHTTP2MaxConcurrentStreams,
		SendPingTimeout:      defaultHTTP2SendPingTimeout,
		PingTimeout:          defaultHTTP2PingTimeout,
		WriteByteTimeout:     defaultHTTP2WriteByteTimeout,
	}
}

// DefaultHTTPServerTimeoutProfile returns Lazuli's default HTTP server timeout
// profile.
func DefaultHTTPServerTimeoutProfile() HTTPServerTimeoutProfile {
	profile, _ := HTTPServerTimeoutProfileByName(HTTPServerTimeoutProfileDefault)
	return profile
}

// HTTPServerTimeoutProfiles returns Lazuli's canonical named server timeout
// profiles. The returned slice is safe for callers to modify.
func HTTPServerTimeoutProfiles() []HTTPServerTimeoutProfile {
	profiles := []HTTPServerTimeoutProfile{
		{
			Name:        HTTPServerTimeoutProfileDefault,
			Description: "Balanced API server defaults.",
			HTTPServerTimeouts: HTTPServerTimeouts{
				KeepAlivesEnabled: true,
				ReadTimeout:       defaultReadTimeout,
				ReadHeaderTimeout: defaultReadHeaderTimeout,
				WriteTimeout:      defaultWriteTimeout,
				IdleTimeout:       defaultIdleTimeout,
			},
			HTTP2: DefaultHTTP2Policy(),
		},
		{
			Name:        HTTPServerTimeoutProfileStrict,
			Description: "Tighter budgets for edge-facing endpoints.",
			HTTPServerTimeouts: HTTPServerTimeouts{
				KeepAlivesEnabled: true,
				ReadTimeout:       10 * time.Second,
				ReadHeaderTimeout: 2 * time.Second,
				WriteTimeout:      15 * time.Second,
				IdleTimeout:       60 * time.Second,
			},
			HTTP2: DefaultHTTP2Policy(),
		},
		{
			Name:        HTTPServerTimeoutProfileStreaming,
			Description: "Header and idle protection for streaming responses.",
			HTTPServerTimeouts: HTTPServerTimeouts{
				KeepAlivesEnabled: true,
				ReadHeaderTimeout: defaultReadHeaderTimeout,
				IdleTimeout:       defaultIdleTimeout,
			},
			HTTP2: DefaultHTTP2Policy(),
		},
	}
	return profiles
}

// HTTPServerTimeoutProfileByName returns a named Lazuli server timeout profile.
// Names are matched case-insensitively after trimming surrounding space.
func HTTPServerTimeoutProfileByName(name string) (HTTPServerTimeoutProfile, bool) {
	switch normalizeHTTPServerTimeoutProfileName(name) {
	case HTTPServerTimeoutProfileDefault, "api", "production":
		return HTTPServerTimeoutProfiles()[0], true
	case HTTPServerTimeoutProfileStrict, "edge":
		return HTTPServerTimeoutProfiles()[1], true
	case HTTPServerTimeoutProfileStreaming, "sse", "long-polling", "long_polling":
		return HTTPServerTimeoutProfiles()[2], true
	default:
		return HTTPServerTimeoutProfile{}, false
	}
}

// Validate reports whether profile can be applied to an http.Server.
func (p HTTPServerTimeoutProfile) Validate() error {
	return ValidateHTTPServerTimeoutProfile(p)
}

// ApplyTo validates and applies profile to server.
func (p HTTPServerTimeoutProfile) ApplyTo(server *http.Server) error {
	return ApplyHTTPServerTimeoutProfile(server, p)
}

// ValidateHTTPServerTimeoutProfile validates a complete timeout and HTTP/2
// profile.
func ValidateHTTPServerTimeoutProfile(profile HTTPServerTimeoutProfile) error {
	if err := ValidateHTTPServerTimeouts(profile.HTTPServerTimeouts); err != nil {
		return err
	}
	if err := ValidateHTTP2Policy(profile.HTTP2); err != nil {
		return err
	}
	return nil
}

// ValidateHTTPServerTimeouts validates keep-alive, read, write, idle, and
// header timeout settings before they are applied to an http.Server.
func ValidateHTTPServerTimeouts(timeouts HTTPServerTimeouts) error {
	if timeouts.ReadTimeout < 0 {
		return fmt.Errorf("%w: read timeout must be non-negative", ErrHTTPServerTimeoutsInvalid)
	}
	if timeouts.ReadHeaderTimeout <= 0 {
		return fmt.Errorf("%w: read header timeout must be positive", ErrHTTPServerTimeoutsInvalid)
	}
	if timeouts.WriteTimeout < 0 {
		return fmt.Errorf("%w: write timeout must be non-negative", ErrHTTPServerTimeoutsInvalid)
	}
	if timeouts.IdleTimeout < 0 {
		return fmt.Errorf("%w: idle timeout must be non-negative", ErrHTTPServerTimeoutsInvalid)
	}
	if timeouts.KeepAlivesEnabled && timeouts.IdleTimeout == 0 {
		return fmt.Errorf("%w: idle timeout must be positive when keep-alives are enabled", ErrHTTPServerTimeoutsInvalid)
	}
	if timeouts.ReadTimeout > 0 && timeouts.ReadTimeout < timeouts.ReadHeaderTimeout {
		return fmt.Errorf("%w: read timeout must be greater than or equal to read header timeout", ErrHTTPServerTimeoutsInvalid)
	}
	return nil
}

// ValidateHTTP2Policy validates an HTTP/2 policy against net/http's supported
// HTTP2Config ranges.
func ValidateHTTP2Policy(policy HTTP2Policy) error {
	if !policy.Enabled {
		if policy.Unencrypted {
			return fmt.Errorf("%w: unencrypted http2 requires http2 to be enabled", ErrHTTP2PolicyInvalid)
		}
		return nil
	}

	if policy.MaxConcurrentStreams < 0 {
		return fmt.Errorf("%w: max concurrent streams must be non-negative", ErrHTTP2PolicyInvalid)
	}
	if policy.MaxDecoderHeaderTableSize < 0 || policy.MaxDecoderHeaderTableSize >= maxHTTP2HeaderTableSize {
		return fmt.Errorf("%w: max decoder header table size must be less than 4 MiB", ErrHTTP2PolicyInvalid)
	}
	if policy.MaxEncoderHeaderTableSize < 0 || policy.MaxEncoderHeaderTableSize >= maxHTTP2HeaderTableSize {
		return fmt.Errorf("%w: max encoder header table size must be less than 4 MiB", ErrHTTP2PolicyInvalid)
	}
	if policy.MaxReadFrameSize != 0 && (policy.MaxReadFrameSize < minHTTP2ReadFrameSize || policy.MaxReadFrameSize > maxHTTP2ReadFrameSize) {
		return fmt.Errorf("%w: max read frame size must be between 16 KiB and 16 MiB", ErrHTTP2PolicyInvalid)
	}
	if policy.MaxReceiveBufferPerConnection != 0 &&
		(policy.MaxReceiveBufferPerConnection < minHTTP2ReceiveBufferPerConnection ||
			policy.MaxReceiveBufferPerConnection >= maxHTTP2ReceiveBufferPerConnection) {
		return fmt.Errorf("%w: max receive buffer per connection must be at least 64 KiB and less than 4 MiB", ErrHTTP2PolicyInvalid)
	}
	if policy.MaxReceiveBufferPerStream < 0 || policy.MaxReceiveBufferPerStream >= maxHTTP2ReceiveBufferPerStream {
		return fmt.Errorf("%w: max receive buffer per stream must be less than 4 MiB", ErrHTTP2PolicyInvalid)
	}
	if policy.SendPingTimeout < 0 {
		return fmt.Errorf("%w: send ping timeout must be non-negative", ErrHTTP2PolicyInvalid)
	}
	if policy.PingTimeout < 0 {
		return fmt.Errorf("%w: ping timeout must be non-negative", ErrHTTP2PolicyInvalid)
	}
	if policy.WriteByteTimeout < 0 {
		return fmt.Errorf("%w: write byte timeout must be non-negative", ErrHTTP2PolicyInvalid)
	}
	return nil
}

// ApplyHTTPServerTimeoutProfile validates and applies profile to server.
func ApplyHTTPServerTimeoutProfile(server *http.Server, profile HTTPServerTimeoutProfile) error {
	if server == nil {
		return errNilHTTPServer
	}
	if err := ValidateHTTPServerTimeoutProfile(profile); err != nil {
		return err
	}

	server.SetKeepAlivesEnabled(profile.KeepAlivesEnabled)
	server.ReadTimeout = profile.ReadTimeout
	server.ReadHeaderTimeout = profile.ReadHeaderTimeout
	server.WriteTimeout = profile.WriteTimeout
	server.IdleTimeout = profile.IdleTimeout
	return ApplyHTTP2Policy(server, profile.HTTP2)
}

// ServerWithTimeoutProfile returns a shallow clone of server with profile
// applied. A nil server returns a new http.Server with the profile applied.
func ServerWithTimeoutProfile(server *http.Server, profile HTTPServerTimeoutProfile) (*http.Server, error) {
	clone := &http.Server{}
	if server != nil {
		*clone = *server
	}
	if err := ApplyHTTPServerTimeoutProfile(clone, profile); err != nil {
		return nil, err
	}
	return clone, nil
}

// ApplyHTTP2Policy validates and applies policy to server.
func ApplyHTTP2Policy(server *http.Server, policy HTTP2Policy) error {
	if server == nil {
		return errNilHTTPServer
	}
	if err := ValidateHTTP2Policy(policy); err != nil {
		return err
	}

	server.HTTP2 = policy.http2Config()
	protocols := cloneHTTPProtocols(server.Protocols)
	protocols.SetHTTP1(true)
	protocols.SetHTTP2(policy.Enabled)
	protocols.SetUnencryptedHTTP2(policy.Enabled && policy.Unencrypted)
	server.Protocols = protocols
	return nil
}

// ServerWithHTTP2Policy returns a shallow clone of server with policy applied.
// A nil server returns a new http.Server with the policy applied.
func ServerWithHTTP2Policy(server *http.Server, policy HTTP2Policy) (*http.Server, error) {
	clone := &http.Server{}
	if server != nil {
		*clone = *server
	}
	if err := ApplyHTTP2Policy(clone, policy); err != nil {
		return nil, err
	}
	return clone, nil
}

func (p HTTP2Policy) http2Config() *http.HTTP2Config {
	if !p.Enabled {
		return nil
	}
	return &http.HTTP2Config{
		MaxConcurrentStreams:          p.MaxConcurrentStreams,
		MaxDecoderHeaderTableSize:     p.MaxDecoderHeaderTableSize,
		MaxEncoderHeaderTableSize:     p.MaxEncoderHeaderTableSize,
		MaxReadFrameSize:              p.MaxReadFrameSize,
		MaxReceiveBufferPerConnection: p.MaxReceiveBufferPerConnection,
		MaxReceiveBufferPerStream:     p.MaxReceiveBufferPerStream,
		SendPingTimeout:               p.SendPingTimeout,
		PingTimeout:                   p.PingTimeout,
		WriteByteTimeout:              p.WriteByteTimeout,
	}
}

func cloneHTTPProtocols(protocols *http.Protocols) *http.Protocols {
	clone := &http.Protocols{}
	if protocols != nil {
		*clone = *protocols
	}
	return clone
}

func normalizeHTTPServerTimeoutProfileName(name string) string {
	return strings.ToLower(strings.TrimSpace(name))
}
