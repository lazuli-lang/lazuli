package rpc

import (
	"errors"
	"testing"
	"time"
)

func TestGRPCStreamingModes(t *testing.T) {
	tests := []struct {
		name             string
		clientStreaming  bool
		serverStreaming  bool
		want             GRPCStreamingMode
		wantClientStream bool
		wantServerStream bool
	}{
		{
			name: "unary",
			want: GRPCStreamingUnary,
		},
		{
			name:             "client",
			clientStreaming:  true,
			want:             GRPCStreamingClient,
			wantClientStream: true,
		},
		{
			name:             "server",
			serverStreaming:  true,
			want:             GRPCStreamingServer,
			wantServerStream: true,
		},
		{
			name:             "bidirectional",
			clientStreaming:  true,
			serverStreaming:  true,
			want:             GRPCStreamingBidirectional,
			wantClientStream: true,
			wantServerStream: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := GRPCStreamingModeFor(tt.clientStreaming, tt.serverStreaming)
			if got != tt.want {
				t.Fatalf("GRPCStreamingModeFor() = %q, want %q", got, tt.want)
			}
			if got.ClientStreaming() != tt.wantClientStream {
				t.Fatalf("ClientStreaming() = %v, want %v", got.ClientStreaming(), tt.wantClientStream)
			}
			if got.ServerStreaming() != tt.wantServerStream {
				t.Fatalf("ServerStreaming() = %v, want %v", got.ServerStreaming(), tt.wantServerStream)
			}
		})
	}

	var zero GRPCStreamingMode
	if zero.Normalize() != GRPCStreamingUnary {
		t.Fatalf("zero Normalize() = %q, want %q", zero.Normalize(), GRPCStreamingUnary)
	}
	if !zero.Valid() {
		t.Fatal("zero Valid() = false, want true")
	}
	if err := GRPCStreamingMode("other").Validate(); !errors.Is(err, ErrInvalidGRPCStreamingMode) {
		t.Fatalf("invalid streaming mode error = %v, want ErrInvalidGRPCStreamingMode", err)
	}
}

func TestNormalizeGRPCServiceDescriptor(t *testing.T) {
	input := GRPCService(" CustomerService ",
		GRPCMethod(" ListCustomers ", " .lazuli.customer.v1.ListCustomersRequest ", "lazuli.customer.v1.ListCustomersResponse", "").
			WithDeadline(1500*time.Millisecond),
		GRPCServerStreamingMethod("WatchCustomers", "lazuli.customer.v1.WatchCustomersRequest", "lazuli.customer.v1.CustomerEvent"),
	).WithPackage(" lazuli.customer.v1 ")

	normalized, err := NormalizeGRPCServiceDescriptor(input)
	if err != nil {
		t.Fatalf("NormalizeGRPCServiceDescriptor() error = %v", err)
	}
	if normalized.Package != "lazuli.customer.v1" {
		t.Fatalf("Package = %q, want lazuli.customer.v1", normalized.Package)
	}
	if normalized.Name != "CustomerService" {
		t.Fatalf("Name = %q, want CustomerService", normalized.Name)
	}
	if got := normalized.FullName(); got != "lazuli.customer.v1.CustomerService" {
		t.Fatalf("FullName() = %q, want lazuli.customer.v1.CustomerService", got)
	}
	if got := normalized.MethodPath("ListCustomers"); got != "/lazuli.customer.v1.CustomerService/ListCustomers" {
		t.Fatalf("MethodPath() = %q, want canonical path", got)
	}

	method, ok := normalized.LookupMethod("ListCustomers")
	if !ok {
		t.Fatal("LookupMethod() ok = false, want true")
	}
	if method.Streaming != GRPCStreamingUnary {
		t.Fatalf("method Streaming = %q, want unary", method.Streaming)
	}
	if method.RequestType != ".lazuli.customer.v1.ListCustomersRequest" {
		t.Fatalf("RequestType = %q, want trimmed type", method.RequestType)
	}
	if got := method.Path(normalized); got != "/lazuli.customer.v1.CustomerService/ListCustomers" {
		t.Fatalf("method Path() = %q, want canonical path", got)
	}

	normalized.Methods[0].Name = "Mutated"
	if input.Methods[0].Name != " ListCustomers " {
		t.Fatalf("NormalizeGRPCServiceDescriptor mutated input method name to %q", input.Methods[0].Name)
	}
}

func TestValidateGRPCServiceDescriptorsRejectsInvalidAndDuplicateMetadata(t *testing.T) {
	validMethod := GRPCUnaryMethod("GetCustomer", "lazuli.customer.v1.GetCustomerRequest", "lazuli.customer.v1.Customer")
	services := []GRPCServiceDescriptor{
		GRPCService("CustomerService", validMethod).WithPackage("lazuli.customer.v1"),
		GRPCService("CustomerService", validMethod).WithPackage("lazuli.customer.v1"),
		GRPCService("OrderService",
			GRPCUnaryMethod("GetOrder", "lazuli.order.v1.GetOrderRequest", "lazuli.order.v1.Order"),
			GRPCUnaryMethod("GetOrder", "lazuli.order.v1.GetOrderRequest", "lazuli.order.v1.Order"),
		).WithPackage("lazuli.order.v1"),
		GRPCService("Bad-Service",
			GRPCMethod("Stream", "bad type", "lazuli.bad.v1.Response", GRPCStreamingMode("sideways")),
		).WithPackage("lazuli.bad.v1"),
	}

	err := ValidateGRPCServiceDescriptors(services)
	if !errors.Is(err, ErrDuplicateGRPCServiceDescriptor) {
		t.Fatalf("ValidateGRPCServiceDescriptors() error = %v, want ErrDuplicateGRPCServiceDescriptor", err)
	}
	if !errors.Is(err, ErrDuplicateGRPCMethodDescriptor) {
		t.Fatalf("ValidateGRPCServiceDescriptors() error = %v, want ErrDuplicateGRPCMethodDescriptor", err)
	}
	if !errors.Is(err, ErrInvalidGRPCServiceDescriptor) {
		t.Fatalf("ValidateGRPCServiceDescriptors() error = %v, want ErrInvalidGRPCServiceDescriptor", err)
	}
	if !errors.Is(err, ErrInvalidGRPCMethodDescriptor) {
		t.Fatalf("ValidateGRPCServiceDescriptors() error = %v, want ErrInvalidGRPCMethodDescriptor", err)
	}
}

func TestGRPCDeadlineMetadataRoundTrip(t *testing.T) {
	metadata, err := NewGRPCDeadlineMetadata(1500 * time.Millisecond)
	if err != nil {
		t.Fatalf("NewGRPCDeadlineMetadata() error = %v", err)
	}
	if !metadata.Present() {
		t.Fatal("Present() = false, want true")
	}

	headers, err := metadata.Metadata()
	if err != nil {
		t.Fatalf("Metadata() error = %v", err)
	}
	if got := headers[GRPCTimeoutMetadataKey]; got != "1500m" {
		t.Fatalf("grpc-timeout = %q, want 1500m", got)
	}

	parsed, ok, err := GRPCDeadlineMetadataFromMap(map[string]string{"Grpc-Timeout": headers[GRPCTimeoutMetadataKey]})
	if err != nil {
		t.Fatalf("GRPCDeadlineMetadataFromMap() error = %v", err)
	}
	if !ok {
		t.Fatal("GRPCDeadlineMetadataFromMap() ok = false, want true")
	}
	if parsed.Timeout != metadata.Timeout {
		t.Fatalf("parsed Timeout = %s, want %s", parsed.Timeout, metadata.Timeout)
	}

	now := time.Date(2026, 5, 12, 10, 0, 0, 0, time.UTC)
	deadline, ok := parsed.Deadline(now)
	if !ok {
		t.Fatal("Deadline() ok = false, want true")
	}
	if !deadline.Equal(now.Add(1500 * time.Millisecond)) {
		t.Fatalf("Deadline() = %s, want %s", deadline, now.Add(1500*time.Millisecond))
	}

	fromDeadline, err := GRPCDeadlineMetadataFromDeadline(now, deadline)
	if err != nil {
		t.Fatalf("GRPCDeadlineMetadataFromDeadline() error = %v", err)
	}
	if fromDeadline.Timeout != 1500*time.Millisecond {
		t.Fatalf("fromDeadline Timeout = %s, want 1.5s", fromDeadline.Timeout)
	}
}

func TestGRPCTimeoutRejectsInvalidMetadata(t *testing.T) {
	for _, value := range []string{"", "0S", "1x", "100000000S", "1 S", "99999999H"} {
		t.Run(value, func(t *testing.T) {
			if _, err := ParseGRPCTimeout(value); !errors.Is(err, ErrInvalidGRPCDeadlineMetadata) {
				t.Fatalf("ParseGRPCTimeout(%q) error = %v, want ErrInvalidGRPCDeadlineMetadata", value, err)
			}
		})
	}

	if _, err := FormatGRPCTimeout(0); !errors.Is(err, ErrInvalidGRPCDeadlineMetadata) {
		t.Fatalf("FormatGRPCTimeout(0) error = %v, want ErrInvalidGRPCDeadlineMetadata", err)
	}
	if _, err := NewGRPCDeadlineMetadata(-time.Second); !errors.Is(err, ErrInvalidGRPCDeadlineMetadata) {
		t.Fatalf("NewGRPCDeadlineMetadata(-1s) error = %v, want ErrInvalidGRPCDeadlineMetadata", err)
	}
	if err := (GRPCDeadlineMetadata{Timeout: -time.Second}).Validate(); !errors.Is(err, ErrInvalidGRPCDeadlineMetadata) {
		t.Fatalf("Validate() error = %v, want ErrInvalidGRPCDeadlineMetadata", err)
	}

	metadata, ok, err := GRPCDeadlineMetadataFromMap(map[string]string{})
	if err != nil {
		t.Fatalf("GRPCDeadlineMetadataFromMap(empty) error = %v", err)
	}
	if ok {
		t.Fatalf("GRPCDeadlineMetadataFromMap(empty) ok = true, want false with metadata %#v", metadata)
	}
}
