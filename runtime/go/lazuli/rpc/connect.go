// Package rpc contains adapter-neutral RPC descriptor helpers.
package rpc

import (
	"errors"
	"fmt"
	"sort"
	"strings"
)

// ErrInvalidConnectDescriptor is returned when ConnectRPC route or method
// metadata cannot be normalized safely.
var ErrInvalidConnectDescriptor = errors.New("lazuli/rpc: invalid connect descriptor")

// ConnectProtocolMode identifies the HTTP protocol variant accepted by a
// ConnectRPC adapter route.
type ConnectProtocolMode string

const (
	// ConnectProtocolModeConnect is the native Connect protocol.
	ConnectProtocolModeConnect ConnectProtocolMode = "connect"

	// ConnectProtocolModeGRPC is gRPC over HTTP/2.
	ConnectProtocolModeGRPC ConnectProtocolMode = "grpc"

	// ConnectProtocolModeGRPCWeb is gRPC-Web over HTTP.
	ConnectProtocolModeGRPCWeb ConnectProtocolMode = "grpc-web"
)

// DefaultConnectProtocolModes returns the default protocol set for Lazuli's
// ConnectRPC descriptors. Callers receive a new slice.
func DefaultConnectProtocolModes() []ConnectProtocolMode {
	return []ConnectProtocolMode{ConnectProtocolModeConnect}
}

// NormalizeConnectProtocolMode returns the canonical lowercase spelling for a
// supported ConnectRPC protocol mode.
func NormalizeConnectProtocolMode(mode ConnectProtocolMode) (ConnectProtocolMode, error) {
	normalized, err := normalizeConnectProtocolMode(mode)
	if err != nil {
		return "", invalidConnectDescriptor("%v", err)
	}
	return normalized, nil
}

// Validate reports whether mode is one of the supported ConnectRPC protocol
// modes.
func (m ConnectProtocolMode) Validate() error {
	_, err := NormalizeConnectProtocolMode(m)
	return err
}

// NormalizeConnectProtocolModes trims, validates duplicate-free modes, and
// sorts protocol modes into Lazuli's deterministic order. Empty input defaults to
// DefaultConnectProtocolModes.
func NormalizeConnectProtocolModes(modes []ConnectProtocolMode) ([]ConnectProtocolMode, error) {
	normalized, err := normalizeConnectProtocolModes(modes)
	if err != nil {
		return nil, invalidConnectDescriptor("%v", err)
	}
	return normalized, nil
}

// ValidateConnectProtocolModes checks a protocol mode list without returning
// the normalized copy.
func ValidateConnectProtocolModes(modes []ConnectProtocolMode) error {
	_, err := NormalizeConnectProtocolModes(modes)
	return err
}

// ConnectCompressionFlags describes request and response compression support
// for a ConnectRPC method descriptor.
type ConnectCompressionFlags uint8

const (
	// ConnectCompressionNone means the descriptor does not advertise request or
	// response compression support.
	ConnectCompressionNone ConnectCompressionFlags = 0
)

const (
	// ConnectCompressionRequest means the route may receive compressed request
	// messages.
	ConnectCompressionRequest ConnectCompressionFlags = 1 << iota

	// ConnectCompressionResponse means the route may emit compressed response
	// messages.
	ConnectCompressionResponse
)

const connectCompressionKnown = ConnectCompressionRequest | ConnectCompressionResponse

// Validate checks that flags contains only known compression bits.
func (f ConnectCompressionFlags) Validate() error {
	if err := validateConnectCompressionFlags(f); err != nil {
		return invalidConnectDescriptor("%v", err)
	}
	return nil
}

// AllowsRequestCompression reports whether compressed requests are advertised.
func (f ConnectCompressionFlags) AllowsRequestCompression() bool {
	return f&ConnectCompressionRequest != 0
}

// AllowsResponseCompression reports whether compressed responses are advertised.
func (f ConnectCompressionFlags) AllowsResponseCompression() bool {
	return f&ConnectCompressionResponse != 0
}

// Any reports whether any compression support is advertised.
func (f ConnectCompressionFlags) Any() bool {
	return f&connectCompressionKnown != 0
}

// ConnectRoute is the canonical HTTP route for one ConnectRPC method.
type ConnectRoute struct {
	Service string `json:"service"`
	Method  string `json:"method"`
	Path    string `json:"path"`
}

// NewConnectRoute returns the canonical ConnectRPC route for service and
// method. Service names may be passed with a leading protobuf dot; paths are
// always rendered without it.
func NewConnectRoute(service, method string) (ConnectRoute, error) {
	route, err := newConnectRoute(service, method)
	if err != nil {
		return ConnectRoute{}, invalidConnectDescriptor("%v", err)
	}
	return route, nil
}

// ConnectRoutePath returns the canonical HTTP path for a ConnectRPC method.
func ConnectRoutePath(service, method string) (string, error) {
	route, err := NewConnectRoute(service, method)
	if err != nil {
		return "", err
	}
	return route.Path, nil
}

// ParseConnectRoutePath parses a canonical ConnectRPC route path such as
// "/acme.profile.v1.ProfileService/GetProfile".
func ParseConnectRoutePath(routePath string) (ConnectRoute, error) {
	route, err := parseConnectRoutePath(routePath)
	if err != nil {
		return ConnectRoute{}, invalidConnectDescriptor("%v", err)
	}
	return route, nil
}

// Validate checks that route contains valid service and method names. If Path
// is present, it must match the canonical ConnectRPC path.
func (r ConnectRoute) Validate() error {
	route, err := NewConnectRoute(r.Service, r.Method)
	if err != nil {
		return err
	}
	if strings.TrimSpace(r.Path) != "" && r.Path != route.Path {
		return invalidConnectDescriptor("path %q does not match %q", r.Path, route.Path)
	}
	return nil
}

// ConnectMethodDescriptor is adapter-neutral metadata for one ConnectRPC
// method.
type ConnectMethodDescriptor struct {
	Service         string                  `json:"service"`
	Method          string                  `json:"method"`
	RequestType     string                  `json:"request_type,omitempty"`
	ResponseType    string                  `json:"response_type,omitempty"`
	ClientStreaming bool                    `json:"client_streaming,omitempty"`
	ServerStreaming bool                    `json:"server_streaming,omitempty"`
	Protocols       []ConnectProtocolMode   `json:"protocols,omitempty"`
	Compression     ConnectCompressionFlags `json:"compression,omitempty"`
}

// NormalizeConnectMethodDescriptor returns a canonical copy of method.
func NormalizeConnectMethodDescriptor(method ConnectMethodDescriptor) (ConnectMethodDescriptor, error) {
	normalized, err := normalizeConnectMethodDescriptor(method)
	if err != nil {
		return ConnectMethodDescriptor{}, invalidConnectDescriptor("%v", err)
	}
	return normalized, nil
}

// ValidateConnectMethodDescriptor checks method metadata without returning the
// normalized copy.
func ValidateConnectMethodDescriptor(method ConnectMethodDescriptor) error {
	_, err := NormalizeConnectMethodDescriptor(method)
	return err
}

// Validate checks method metadata without mutating the descriptor.
func (m ConnectMethodDescriptor) Validate() error {
	return ValidateConnectMethodDescriptor(m)
}

// Route returns the canonical ConnectRPC route for this method.
func (m ConnectMethodDescriptor) Route() (ConnectRoute, error) {
	return NewConnectRoute(m.Service, m.Method)
}

// RoutePath returns the canonical HTTP path for this method.
func (m ConnectMethodDescriptor) RoutePath() (string, error) {
	return ConnectRoutePath(m.Service, m.Method)
}

// SupportsProtocol reports whether this method descriptor advertises mode.
func (m ConnectMethodDescriptor) SupportsProtocol(mode ConnectProtocolMode) bool {
	normalizedMode, err := normalizeConnectProtocolMode(mode)
	if err != nil {
		return false
	}
	modes, err := normalizeConnectProtocolModes(m.Protocols)
	if err != nil {
		return false
	}
	for _, candidate := range modes {
		if candidate == normalizedMode {
			return true
		}
	}
	return false
}

// ConnectServiceDescriptor groups ConnectRPC method metadata for one protobuf
// service.
type ConnectServiceDescriptor struct {
	Service string                    `json:"service"`
	Methods []ConnectMethodDescriptor `json:"methods,omitempty"`
}

// NewConnectServiceDescriptor returns a normalized service descriptor with
// methods sorted by method name. Method descriptors with an empty Service field
// inherit service.
func NewConnectServiceDescriptor(service string, methods []ConnectMethodDescriptor) (ConnectServiceDescriptor, error) {
	descriptor, err := newConnectServiceDescriptor(service, methods)
	if err != nil {
		return ConnectServiceDescriptor{}, invalidConnectDescriptor("%v", err)
	}
	return descriptor, nil
}

// ValidateConnectServiceDescriptor checks service metadata without returning
// the normalized copy.
func ValidateConnectServiceDescriptor(descriptor ConnectServiceDescriptor) error {
	_, err := NewConnectServiceDescriptor(descriptor.Service, descriptor.Methods)
	return err
}

// Validate checks service metadata without mutating the descriptor.
func (d ConnectServiceDescriptor) Validate() error {
	return ValidateConnectServiceDescriptor(d)
}

// Method returns the descriptor for method name after applying the same
// normalization used by NewConnectServiceDescriptor.
func (d ConnectServiceDescriptor) Method(method string) (ConnectMethodDescriptor, bool) {
	method, err := normalizeConnectIdentifier(method, "method")
	if err != nil {
		return ConnectMethodDescriptor{}, false
	}
	normalized, err := newConnectServiceDescriptor(d.Service, d.Methods)
	if err != nil {
		return ConnectMethodDescriptor{}, false
	}
	for _, candidate := range normalized.Methods {
		if candidate.Method == method {
			return cloneConnectMethodDescriptor(candidate), true
		}
	}
	return ConnectMethodDescriptor{}, false
}

// Routes returns the canonical ConnectRPC routes for every method in descriptor.
func (d ConnectServiceDescriptor) Routes() ([]ConnectRoute, error) {
	normalized, err := NewConnectServiceDescriptor(d.Service, d.Methods)
	if err != nil {
		return nil, err
	}
	routes := make([]ConnectRoute, 0, len(normalized.Methods))
	for _, method := range normalized.Methods {
		route, err := method.Route()
		if err != nil {
			return nil, err
		}
		routes = append(routes, route)
	}
	return routes, nil
}

func normalizeConnectProtocolMode(mode ConnectProtocolMode) (ConnectProtocolMode, error) {
	switch normalized := ConnectProtocolMode(strings.ToLower(strings.TrimSpace(string(mode)))); normalized {
	case ConnectProtocolModeConnect, ConnectProtocolModeGRPC, ConnectProtocolModeGRPCWeb:
		return normalized, nil
	default:
		return "", fmt.Errorf("unsupported protocol mode %q", mode)
	}
}

func normalizeConnectProtocolModes(modes []ConnectProtocolMode) ([]ConnectProtocolMode, error) {
	if len(modes) == 0 {
		return DefaultConnectProtocolModes(), nil
	}

	seen := make(map[ConnectProtocolMode]int, len(modes))
	normalized := make([]ConnectProtocolMode, 0, len(modes))
	for i, mode := range modes {
		mode, err := normalizeConnectProtocolMode(mode)
		if err != nil {
			return nil, fmt.Errorf("protocols[%d]: %v", i, err)
		}
		if previous, exists := seen[mode]; exists {
			return nil, fmt.Errorf("protocols[%d] duplicates protocols[%d]", i, previous)
		}
		seen[mode] = i
		normalized = append(normalized, mode)
	}
	sort.Slice(normalized, func(i, j int) bool {
		return connectProtocolRank(normalized[i]) < connectProtocolRank(normalized[j])
	})
	return normalized, nil
}

func connectProtocolRank(mode ConnectProtocolMode) int {
	switch mode {
	case ConnectProtocolModeConnect:
		return 0
	case ConnectProtocolModeGRPC:
		return 1
	case ConnectProtocolModeGRPCWeb:
		return 2
	default:
		return 99
	}
}

func validateConnectCompressionFlags(flags ConnectCompressionFlags) error {
	if flags&^connectCompressionKnown != 0 {
		return fmt.Errorf("unknown compression flags 0x%x", uint8(flags&^connectCompressionKnown))
	}
	return nil
}

func newConnectRoute(service, method string) (ConnectRoute, error) {
	service, err := normalizeConnectFullName(service, "service")
	if err != nil {
		return ConnectRoute{}, err
	}
	method, err = normalizeConnectIdentifier(method, "method")
	if err != nil {
		return ConnectRoute{}, err
	}
	return ConnectRoute{
		Service: service,
		Method:  method,
		Path:    "/" + service + "/" + method,
	}, nil
}

func parseConnectRoutePath(routePath string) (ConnectRoute, error) {
	routePath = strings.TrimSpace(routePath)
	if routePath == "" {
		return ConnectRoute{}, fmt.Errorf("path is required")
	}
	if strings.HasPrefix(routePath, "//") || strings.Contains(routePath, "://") {
		return ConnectRoute{}, fmt.Errorf("path must not be an absolute URL")
	}
	if strings.ContainsAny(routePath, "?#\\") {
		return ConnectRoute{}, fmt.Errorf("path must not contain query strings, fragments, or backslashes")
	}
	if !strings.HasPrefix(routePath, "/") {
		return ConnectRoute{}, fmt.Errorf("path must start with /")
	}

	parts := strings.Split(strings.TrimPrefix(routePath, "/"), "/")
	if len(parts) != 2 {
		return ConnectRoute{}, fmt.Errorf("path must have service and method segments")
	}
	route, err := newConnectRoute(parts[0], parts[1])
	if err != nil {
		return ConnectRoute{}, err
	}
	if route.Path != routePath {
		return ConnectRoute{}, fmt.Errorf("path %q is not canonical; use %q", routePath, route.Path)
	}
	return route, nil
}

func normalizeConnectMethodDescriptor(method ConnectMethodDescriptor) (ConnectMethodDescriptor, error) {
	route, err := newConnectRoute(method.Service, method.Method)
	if err != nil {
		return ConnectMethodDescriptor{}, err
	}
	requestType, err := normalizeConnectOptionalFullName(method.RequestType, "request type")
	if err != nil {
		return ConnectMethodDescriptor{}, err
	}
	responseType, err := normalizeConnectOptionalFullName(method.ResponseType, "response type")
	if err != nil {
		return ConnectMethodDescriptor{}, err
	}
	if (requestType == "") != (responseType == "") {
		return ConnectMethodDescriptor{}, fmt.Errorf("request type and response type must be provided together")
	}
	protocols, err := normalizeConnectProtocolModes(method.Protocols)
	if err != nil {
		return ConnectMethodDescriptor{}, err
	}
	if err := validateConnectCompressionFlags(method.Compression); err != nil {
		return ConnectMethodDescriptor{}, err
	}

	method.Service = route.Service
	method.Method = route.Method
	method.RequestType = requestType
	method.ResponseType = responseType
	method.Protocols = protocols
	return method, nil
}

func newConnectServiceDescriptor(service string, methods []ConnectMethodDescriptor) (ConnectServiceDescriptor, error) {
	service, err := normalizeConnectFullName(service, "service")
	if err != nil {
		return ConnectServiceDescriptor{}, err
	}

	normalized := make([]ConnectMethodDescriptor, len(methods))
	seen := make(map[string]int, len(methods))
	for i, method := range methods {
		if strings.TrimSpace(method.Service) == "" {
			method.Service = service
		}

		method, err := normalizeConnectMethodDescriptor(method)
		if err != nil {
			return ConnectServiceDescriptor{}, fmt.Errorf("methods[%d]: %v", i, err)
		}
		if method.Service != service {
			return ConnectServiceDescriptor{}, fmt.Errorf("methods[%d] service %q does not match %q", i, method.Service, service)
		}
		if previous, exists := seen[method.Method]; exists {
			return ConnectServiceDescriptor{}, fmt.Errorf("methods[%d] duplicates methods[%d] method %q", i, previous, method.Method)
		}
		seen[method.Method] = i
		normalized[i] = method
	}

	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Method < normalized[j].Method
	})
	return ConnectServiceDescriptor{
		Service: service,
		Methods: normalized,
	}, nil
}

func normalizeConnectOptionalFullName(value, field string) (string, error) {
	if strings.TrimSpace(value) == "" {
		return "", nil
	}
	return normalizeConnectFullName(value, field)
}

func normalizeConnectFullName(value, field string) (string, error) {
	value = strings.TrimSpace(value)
	value = strings.TrimPrefix(value, ".")
	if value == "" {
		return "", fmt.Errorf("%s is required", field)
	}
	parts := strings.Split(value, ".")
	for _, part := range parts {
		if !isConnectIdentifier(part) {
			return "", fmt.Errorf("%s %q must be a protobuf full name", field, value)
		}
	}
	return value, nil
}

func normalizeConnectIdentifier(value, field string) (string, error) {
	value = strings.TrimSpace(value)
	if value == "" {
		return "", fmt.Errorf("%s is required", field)
	}
	if !isConnectIdentifier(value) {
		return "", fmt.Errorf("%s %q must be a protobuf identifier", field, value)
	}
	return value, nil
}

func isConnectIdentifier(value string) bool {
	if value == "" {
		return false
	}
	for i := 0; i < len(value); i++ {
		ch := value[i]
		if i == 0 {
			if !isConnectIdentifierStart(ch) {
				return false
			}
			continue
		}
		if !isConnectIdentifierPart(ch) {
			return false
		}
	}
	return true
}

func isConnectIdentifierStart(ch byte) bool {
	return ch == '_' || ch >= 'A' && ch <= 'Z' || ch >= 'a' && ch <= 'z'
}

func isConnectIdentifierPart(ch byte) bool {
	return isConnectIdentifierStart(ch) || ch >= '0' && ch <= '9'
}

func cloneConnectMethodDescriptor(method ConnectMethodDescriptor) ConnectMethodDescriptor {
	method.Protocols = append([]ConnectProtocolMode(nil), method.Protocols...)
	return method
}

func invalidConnectDescriptor(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidConnectDescriptor, fmt.Sprintf(format, args...))
}
