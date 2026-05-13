// Package rpc provides provider-neutral RPC descriptor helpers used by
// generated Lazuli runtime code and concrete transport adapters.
package rpc

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"
)

const (
	// GRPCTimeoutMetadataKey is the wire metadata key used by gRPC to carry a
	// relative request deadline.
	GRPCTimeoutMetadataKey = "grpc-timeout"

	grpcTimeoutMaxDigits = 8
	grpcTimeoutMaxValue  = int64(99999999)
)

var (
	// ErrInvalidGRPCServiceDescriptor reports structurally invalid gRPC
	// service metadata.
	ErrInvalidGRPCServiceDescriptor = errors.New("lazuli/rpc: invalid grpc service descriptor")

	// ErrDuplicateGRPCServiceDescriptor reports duplicate service descriptors
	// in one catalog.
	ErrDuplicateGRPCServiceDescriptor = errors.New("lazuli/rpc: duplicate grpc service descriptor")

	// ErrInvalidGRPCMethodDescriptor reports structurally invalid gRPC method
	// metadata.
	ErrInvalidGRPCMethodDescriptor = errors.New("lazuli/rpc: invalid grpc method descriptor")

	// ErrDuplicateGRPCMethodDescriptor reports duplicate methods within one
	// service descriptor.
	ErrDuplicateGRPCMethodDescriptor = errors.New("lazuli/rpc: duplicate grpc method descriptor")

	// ErrInvalidGRPCStreamingMode reports an unknown method streaming mode.
	ErrInvalidGRPCStreamingMode = errors.New("lazuli/rpc: invalid grpc streaming mode")

	// ErrInvalidGRPCDeadlineMetadata reports malformed grpc-timeout metadata.
	ErrInvalidGRPCDeadlineMetadata = errors.New("lazuli/rpc: invalid grpc deadline metadata")
)

// GRPCStreamingMode describes the request and response streaming shape of a
// gRPC method without importing a concrete grpc implementation.
type GRPCStreamingMode string

const (
	// GRPCStreamingUnary means the client sends one request and receives one
	// response.
	GRPCStreamingUnary GRPCStreamingMode = "unary"

	// GRPCStreamingClient means the client sends a stream and receives one
	// response.
	GRPCStreamingClient GRPCStreamingMode = "client_streaming"

	// GRPCStreamingServer means the client sends one request and receives a
	// response stream.
	GRPCStreamingServer GRPCStreamingMode = "server_streaming"

	// GRPCStreamingBidirectional means both client and server stream messages.
	GRPCStreamingBidirectional GRPCStreamingMode = "bidirectional_streaming"
)

// GRPCStreamingModeFor returns the streaming mode represented by the client
// and server streaming flags found in protobuf method descriptors.
func GRPCStreamingModeFor(clientStreaming, serverStreaming bool) GRPCStreamingMode {
	switch {
	case clientStreaming && serverStreaming:
		return GRPCStreamingBidirectional
	case clientStreaming:
		return GRPCStreamingClient
	case serverStreaming:
		return GRPCStreamingServer
	default:
		return GRPCStreamingUnary
	}
}

// String returns the canonical streaming mode token. The zero value renders as
// unary because descriptors normalize empty mode to unary.
func (mode GRPCStreamingMode) String() string {
	if mode == "" {
		return string(GRPCStreamingUnary)
	}
	return string(mode)
}

// Normalize returns unary for the zero value and leaves explicit modes intact.
func (mode GRPCStreamingMode) Normalize() GRPCStreamingMode {
	if mode == "" {
		return GRPCStreamingUnary
	}
	return mode
}

// ClientStreaming reports whether the client side streams request messages.
func (mode GRPCStreamingMode) ClientStreaming() bool {
	switch mode.Normalize() {
	case GRPCStreamingClient, GRPCStreamingBidirectional:
		return true
	default:
		return false
	}
}

// ServerStreaming reports whether the server side streams response messages.
func (mode GRPCStreamingMode) ServerStreaming() bool {
	switch mode.Normalize() {
	case GRPCStreamingServer, GRPCStreamingBidirectional:
		return true
	default:
		return false
	}
}

// Valid reports whether mode is one of the known streaming modes. The zero
// value is valid and normalizes to unary.
func (mode GRPCStreamingMode) Valid() bool {
	return mode.Validate() == nil
}

// Validate reports an error when mode is not a known streaming mode. The zero
// value is valid and normalizes to unary.
func (mode GRPCStreamingMode) Validate() error {
	switch mode.Normalize() {
	case GRPCStreamingUnary, GRPCStreamingClient, GRPCStreamingServer, GRPCStreamingBidirectional:
		return nil
	default:
		return fmt.Errorf("%w: %q", ErrInvalidGRPCStreamingMode, mode)
	}
}

// GRPCServiceDescriptor describes one generated gRPC service in a provider-
// neutral form. Package is optional; Name and Methods are required.
type GRPCServiceDescriptor struct {
	Package string
	Name    string
	Methods []GRPCMethodDescriptor
}

// GRPCMethodDescriptor describes one generated gRPC method in a provider-
// neutral form. RequestType and ResponseType are protobuf type names.
type GRPCMethodDescriptor struct {
	Name         string
	RequestType  string
	ResponseType string
	Streaming    GRPCStreamingMode
	Deadline     GRPCDeadlineMetadata
}

// GRPCDeadlineMetadata carries the relative timeout used to encode the
// grpc-timeout metadata value. A zero Timeout means no deadline metadata.
type GRPCDeadlineMetadata struct {
	Timeout time.Duration
}

// GRPCService returns a service descriptor with copied method descriptors.
func GRPCService(name string, methods ...GRPCMethodDescriptor) GRPCServiceDescriptor {
	return GRPCServiceDescriptor{
		Name:    name,
		Methods: append([]GRPCMethodDescriptor(nil), methods...),
	}
}

// WithPackage returns a copy assigned to a protobuf package.
func (service GRPCServiceDescriptor) WithPackage(pkg string) GRPCServiceDescriptor {
	service.Package = pkg
	return service
}

// FullName returns the fully qualified service name, omitting the package when
// Package is empty.
func (service GRPCServiceDescriptor) FullName() string {
	pkg := strings.TrimSpace(service.Package)
	name := strings.TrimSpace(service.Name)
	if pkg == "" {
		return name
	}
	if name == "" {
		return pkg
	}
	return pkg + "." + name
}

// MethodPath returns the canonical gRPC full method path for method.
func (service GRPCServiceDescriptor) MethodPath(method string) string {
	method = strings.TrimSpace(method)
	fullName := service.FullName()
	if fullName == "" || method == "" {
		return ""
	}
	return "/" + fullName + "/" + method
}

// LookupMethod returns the first method with the supplied name.
func (service GRPCServiceDescriptor) LookupMethod(name string) (GRPCMethodDescriptor, bool) {
	name = strings.TrimSpace(name)
	for _, method := range service.Methods {
		if strings.TrimSpace(method.Name) == name {
			return method, true
		}
	}
	return GRPCMethodDescriptor{}, false
}

// Validate reports whether the service descriptor is structurally valid.
func (service GRPCServiceDescriptor) Validate() error {
	_, err := NormalizeGRPCServiceDescriptor(service)
	return err
}

// GRPCMethod returns a method descriptor for the supplied streaming mode.
func GRPCMethod(name, requestType, responseType string, streaming GRPCStreamingMode) GRPCMethodDescriptor {
	return GRPCMethodDescriptor{
		Name:         name,
		RequestType:  requestType,
		ResponseType: responseType,
		Streaming:    streaming,
	}
}

// GRPCUnaryMethod returns a unary method descriptor.
func GRPCUnaryMethod(name, requestType, responseType string) GRPCMethodDescriptor {
	return GRPCMethod(name, requestType, responseType, GRPCStreamingUnary)
}

// GRPCClientStreamingMethod returns a client-streaming method descriptor.
func GRPCClientStreamingMethod(name, requestType, responseType string) GRPCMethodDescriptor {
	return GRPCMethod(name, requestType, responseType, GRPCStreamingClient)
}

// GRPCServerStreamingMethod returns a server-streaming method descriptor.
func GRPCServerStreamingMethod(name, requestType, responseType string) GRPCMethodDescriptor {
	return GRPCMethod(name, requestType, responseType, GRPCStreamingServer)
}

// GRPCBidirectionalStreamingMethod returns a bidirectional-streaming method
// descriptor.
func GRPCBidirectionalStreamingMethod(name, requestType, responseType string) GRPCMethodDescriptor {
	return GRPCMethod(name, requestType, responseType, GRPCStreamingBidirectional)
}

// WithDeadline returns a copy with grpc-timeout metadata attached.
func (method GRPCMethodDescriptor) WithDeadline(timeout time.Duration) GRPCMethodDescriptor {
	method.Deadline = GRPCDeadlineMetadata{Timeout: timeout}
	return method
}

// Path returns the canonical gRPC full method path for method under service.
func (method GRPCMethodDescriptor) Path(service GRPCServiceDescriptor) string {
	return service.MethodPath(method.Name)
}

// Validate reports whether the method descriptor is structurally valid.
func (method GRPCMethodDescriptor) Validate() error {
	_, err := normalizeGRPCMethodDescriptor(method, -1, -1)
	return err
}

// NormalizeGRPCServiceDescriptor returns a validated, normalized copy of
// service without mutating the input descriptor.
func NormalizeGRPCServiceDescriptor(service GRPCServiceDescriptor) (GRPCServiceDescriptor, error) {
	return normalizeGRPCServiceDescriptor(service, -1)
}

// NormalizeGRPCServiceDescriptors returns validated, normalized copies of a
// service descriptor catalog without mutating the input slice.
func NormalizeGRPCServiceDescriptors(services []GRPCServiceDescriptor) ([]GRPCServiceDescriptor, error) {
	normalized := make([]GRPCServiceDescriptor, 0, len(services))
	seen := make(map[string]int, len(services))

	var errs []error
	for i, service := range services {
		clean, err := normalizeGRPCServiceDescriptor(service, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := clean.FullName()
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: service[%d] %q also appears at service[%d]", ErrDuplicateGRPCServiceDescriptor, i, key, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

// ValidateGRPCServiceDescriptors checks a service descriptor catalog without
// mutating the input slice.
func ValidateGRPCServiceDescriptors(services []GRPCServiceDescriptor) error {
	_, err := NormalizeGRPCServiceDescriptors(services)
	return err
}

// NewGRPCDeadlineMetadata returns deadline metadata for a positive timeout.
func NewGRPCDeadlineMetadata(timeout time.Duration) (GRPCDeadlineMetadata, error) {
	metadata := GRPCDeadlineMetadata{Timeout: timeout}
	if timeout <= 0 {
		return GRPCDeadlineMetadata{}, fmt.Errorf("%w: timeout must be positive", ErrInvalidGRPCDeadlineMetadata)
	}
	if err := metadata.Validate(); err != nil {
		return GRPCDeadlineMetadata{}, err
	}
	return metadata, nil
}

// GRPCDeadlineMetadataFromDeadline returns deadline metadata for an absolute
// deadline relative to now.
func GRPCDeadlineMetadataFromDeadline(now, deadline time.Time) (GRPCDeadlineMetadata, error) {
	if now.IsZero() {
		return GRPCDeadlineMetadata{}, fmt.Errorf("%w: now is required", ErrInvalidGRPCDeadlineMetadata)
	}
	if deadline.IsZero() {
		return GRPCDeadlineMetadata{}, fmt.Errorf("%w: deadline is required", ErrInvalidGRPCDeadlineMetadata)
	}
	return NewGRPCDeadlineMetadata(deadline.Sub(now))
}

// GRPCDeadlineMetadataFromMap parses grpc-timeout from metadata. The returned
// bool is false when the map does not contain deadline metadata.
func GRPCDeadlineMetadataFromMap(metadata map[string]string) (GRPCDeadlineMetadata, bool, error) {
	value, ok := grpcMetadataValue(metadata, GRPCTimeoutMetadataKey)
	if !ok {
		return GRPCDeadlineMetadata{}, false, nil
	}

	timeout, err := ParseGRPCTimeout(value)
	if err != nil {
		return GRPCDeadlineMetadata{}, true, err
	}
	return GRPCDeadlineMetadata{Timeout: timeout}, true, nil
}

// Present reports whether metadata carries a timeout.
func (metadata GRPCDeadlineMetadata) Present() bool {
	return metadata.Timeout > 0
}

// Validate reports whether metadata can be encoded as grpc-timeout. A zero
// timeout is valid and represents absent deadline metadata.
func (metadata GRPCDeadlineMetadata) Validate() error {
	if metadata.Timeout < 0 {
		return fmt.Errorf("%w: timeout must not be negative", ErrInvalidGRPCDeadlineMetadata)
	}
	if metadata.Timeout == 0 {
		return nil
	}
	_, err := FormatGRPCTimeout(metadata.Timeout)
	return err
}

// Metadata returns a grpc metadata map containing grpc-timeout when a timeout
// is present. The returned map is always caller-owned.
func (metadata GRPCDeadlineMetadata) Metadata() (map[string]string, error) {
	if metadata.Timeout == 0 {
		return map[string]string{}, nil
	}

	value, err := FormatGRPCTimeout(metadata.Timeout)
	if err != nil {
		return nil, err
	}
	return map[string]string{GRPCTimeoutMetadataKey: value}, nil
}

// Deadline returns the absolute deadline relative to now. The returned bool is
// false when no timeout is present.
func (metadata GRPCDeadlineMetadata) Deadline(now time.Time) (time.Time, bool) {
	if metadata.Timeout <= 0 {
		return time.Time{}, false
	}
	return now.Add(metadata.Timeout), true
}

// FormatGRPCTimeout encodes timeout using the grpc-timeout value grammar:
// at most eight digits followed by H, M, S, m, u, or n.
func FormatGRPCTimeout(timeout time.Duration) (string, error) {
	if timeout <= 0 {
		return "", fmt.Errorf("%w: timeout must be positive", ErrInvalidGRPCDeadlineMetadata)
	}

	for _, unit := range grpcTimeoutExactUnits() {
		if timeout%unit.duration != 0 {
			continue
		}
		amount := int64(timeout / unit.duration)
		if amount > 0 && amount <= grpcTimeoutMaxValue {
			return strconv.FormatInt(amount, 10) + string(unit.suffix), nil
		}
	}

	for _, unit := range grpcTimeoutFallbackUnits() {
		amount := ceilGRPCTimeout(timeout, unit.duration)
		if amount > 0 && amount <= grpcTimeoutMaxValue {
			return strconv.FormatInt(amount, 10) + string(unit.suffix), nil
		}
	}

	return "", fmt.Errorf("%w: timeout exceeds grpc-timeout range", ErrInvalidGRPCDeadlineMetadata)
}

// ParseGRPCTimeout parses one grpc-timeout metadata value.
func ParseGRPCTimeout(value string) (time.Duration, error) {
	value = strings.TrimSpace(value)
	if len(value) < 2 {
		return 0, fmt.Errorf("%w: grpc-timeout is incomplete", ErrInvalidGRPCDeadlineMetadata)
	}

	suffix := value[len(value)-1]
	unit, ok := grpcTimeoutUnitDuration(suffix)
	if !ok {
		return 0, fmt.Errorf("%w: grpc-timeout unit %q is unknown", ErrInvalidGRPCDeadlineMetadata, suffix)
	}

	digits := value[:len(value)-1]
	if len(digits) > grpcTimeoutMaxDigits {
		return 0, fmt.Errorf("%w: grpc-timeout has more than %d digits", ErrInvalidGRPCDeadlineMetadata, grpcTimeoutMaxDigits)
	}
	for _, r := range digits {
		if r < '0' || r > '9' {
			return 0, fmt.Errorf("%w: grpc-timeout amount must be decimal digits", ErrInvalidGRPCDeadlineMetadata)
		}
	}

	amount, err := strconv.ParseInt(digits, 10, 64)
	if err != nil || amount <= 0 {
		return 0, fmt.Errorf("%w: grpc-timeout amount must be positive", ErrInvalidGRPCDeadlineMetadata)
	}
	if amount > int64(maxDuration()/unit) {
		return 0, fmt.Errorf("%w: grpc-timeout exceeds duration range", ErrInvalidGRPCDeadlineMetadata)
	}

	return time.Duration(amount) * unit, nil
}

type grpcTimeoutUnit struct {
	suffix   byte
	duration time.Duration
}

func normalizeGRPCServiceDescriptor(service GRPCServiceDescriptor, serviceIndex int) (GRPCServiceDescriptor, error) {
	clean := GRPCServiceDescriptor{
		Package: strings.TrimSpace(service.Package),
		Name:    strings.TrimSpace(service.Name),
	}

	var errs []error
	if clean.Package != "" && !validGRPCQualifiedName(clean.Package, false) {
		errs = append(errs, invalidGRPCServiceField(serviceIndex, "package", "must be a protobuf package name"))
	}
	if clean.Name == "" {
		errs = append(errs, invalidGRPCServiceField(serviceIndex, "name", "is required"))
	} else if !validGRPCIdentifier(clean.Name) {
		errs = append(errs, invalidGRPCServiceField(serviceIndex, "name", "must be a protobuf service identifier"))
	}
	if len(service.Methods) == 0 {
		errs = append(errs, invalidGRPCServiceField(serviceIndex, "methods", "must contain at least one method"))
	}

	methods, err := normalizeGRPCMethodDescriptors(service.Methods, serviceIndex)
	if err != nil {
		errs = append(errs, err)
	}
	clean.Methods = methods

	if err := errors.Join(errs...); err != nil {
		return GRPCServiceDescriptor{}, err
	}
	return clean, nil
}

func normalizeGRPCMethodDescriptors(methods []GRPCMethodDescriptor, serviceIndex int) ([]GRPCMethodDescriptor, error) {
	normalized := make([]GRPCMethodDescriptor, 0, len(methods))
	seen := make(map[string]int, len(methods))

	var errs []error
	for i, method := range methods {
		clean, err := normalizeGRPCMethodDescriptor(method, serviceIndex, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		if first, ok := seen[clean.Name]; ok {
			errs = append(errs, fmt.Errorf("%w: %s %q also appears at %s", ErrDuplicateGRPCMethodDescriptor, grpcMethodPath(serviceIndex, i), clean.Name, grpcMethodPath(serviceIndex, first)))
			continue
		}
		seen[clean.Name] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizeGRPCMethodDescriptor(method GRPCMethodDescriptor, serviceIndex, methodIndex int) (GRPCMethodDescriptor, error) {
	clean := GRPCMethodDescriptor{
		Name:         strings.TrimSpace(method.Name),
		RequestType:  strings.TrimSpace(method.RequestType),
		ResponseType: strings.TrimSpace(method.ResponseType),
		Streaming:    method.Streaming.Normalize(),
		Deadline:     method.Deadline,
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidGRPCMethodField(serviceIndex, methodIndex, "name", "is required"))
	} else if !validGRPCIdentifier(clean.Name) {
		errs = append(errs, invalidGRPCMethodField(serviceIndex, methodIndex, "name", "must be a protobuf method identifier"))
	}
	if clean.RequestType == "" {
		errs = append(errs, invalidGRPCMethodField(serviceIndex, methodIndex, "request_type", "is required"))
	} else if !validGRPCQualifiedName(clean.RequestType, true) {
		errs = append(errs, invalidGRPCMethodField(serviceIndex, methodIndex, "request_type", "must be a protobuf type name"))
	}
	if clean.ResponseType == "" {
		errs = append(errs, invalidGRPCMethodField(serviceIndex, methodIndex, "response_type", "is required"))
	} else if !validGRPCQualifiedName(clean.ResponseType, true) {
		errs = append(errs, invalidGRPCMethodField(serviceIndex, methodIndex, "response_type", "must be a protobuf type name"))
	}
	if err := clean.Streaming.Validate(); err != nil {
		errs = append(errs, invalidGRPCMethodField(serviceIndex, methodIndex, "streaming", err.Error()))
	}
	if err := clean.Deadline.Validate(); err != nil {
		errs = append(errs, invalidGRPCMethodField(serviceIndex, methodIndex, "deadline", err.Error()))
	}

	if err := errors.Join(errs...); err != nil {
		return GRPCMethodDescriptor{}, err
	}
	return clean, nil
}

func invalidGRPCServiceField(serviceIndex int, field, reason string) error {
	if serviceIndex >= 0 {
		return fmt.Errorf("%w: service[%d].%s %s", ErrInvalidGRPCServiceDescriptor, serviceIndex, field, reason)
	}
	return fmt.Errorf("%w: %s %s", ErrInvalidGRPCServiceDescriptor, field, reason)
}

func invalidGRPCMethodField(serviceIndex, methodIndex int, field, reason string) error {
	return fmt.Errorf("%w: %s.%s %s", ErrInvalidGRPCMethodDescriptor, grpcMethodPath(serviceIndex, methodIndex), field, reason)
}

func grpcMethodPath(serviceIndex, methodIndex int) string {
	switch {
	case serviceIndex >= 0 && methodIndex >= 0:
		return fmt.Sprintf("service[%d].methods[%d]", serviceIndex, methodIndex)
	case methodIndex >= 0:
		return fmt.Sprintf("methods[%d]", methodIndex)
	default:
		return "method"
	}
}

func validGRPCQualifiedName(name string, allowLeadingDot bool) bool {
	if allowLeadingDot && strings.HasPrefix(name, ".") {
		name = strings.TrimPrefix(name, ".")
	}
	if name == "" {
		return false
	}
	for _, part := range strings.Split(name, ".") {
		if !validGRPCIdentifier(part) {
			return false
		}
	}
	return true
}

func validGRPCIdentifier(name string) bool {
	if name == "" {
		return false
	}
	for i, r := range name {
		switch {
		case r == '_':
			continue
		case r >= 'A' && r <= 'Z':
			continue
		case r >= 'a' && r <= 'z':
			continue
		case i > 0 && r >= '0' && r <= '9':
			continue
		default:
			return false
		}
	}
	return true
}

func grpcMetadataValue(metadata map[string]string, key string) (string, bool) {
	if value, ok := metadata[key]; ok {
		return value, true
	}
	for metadataKey, value := range metadata {
		if strings.EqualFold(metadataKey, key) {
			return value, true
		}
	}
	return "", false
}

func grpcTimeoutExactUnits() []grpcTimeoutUnit {
	return []grpcTimeoutUnit{
		{suffix: 'H', duration: time.Hour},
		{suffix: 'M', duration: time.Minute},
		{suffix: 'S', duration: time.Second},
		{suffix: 'm', duration: time.Millisecond},
		{suffix: 'u', duration: time.Microsecond},
		{suffix: 'n', duration: time.Nanosecond},
	}
}

func grpcTimeoutFallbackUnits() []grpcTimeoutUnit {
	return []grpcTimeoutUnit{
		{suffix: 'n', duration: time.Nanosecond},
		{suffix: 'u', duration: time.Microsecond},
		{suffix: 'm', duration: time.Millisecond},
		{suffix: 'S', duration: time.Second},
		{suffix: 'M', duration: time.Minute},
		{suffix: 'H', duration: time.Hour},
	}
}

func grpcTimeoutUnitDuration(suffix byte) (time.Duration, bool) {
	for _, unit := range grpcTimeoutExactUnits() {
		if unit.suffix == suffix {
			return unit.duration, true
		}
	}
	return 0, false
}

func ceilGRPCTimeout(timeout, unit time.Duration) int64 {
	amount := timeout / unit
	if timeout%unit != 0 {
		amount++
	}
	return int64(amount)
}

func maxDuration() time.Duration {
	return time.Duration(1<<63 - 1)
}
