package rpc_test

import (
	"errors"
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/rpc"
)

func TestConnectRoutePathAndParse(t *testing.T) {
	t.Parallel()

	path, err := rpc.ConnectRoutePath(".lazuli.demo.v1.GreeterService", " SayHello ")
	if err != nil {
		t.Fatalf("ConnectRoutePath() error = %v", err)
	}
	if path != "/lazuli.demo.v1.GreeterService/SayHello" {
		t.Fatalf("path = %q, want canonical ConnectRPC path", path)
	}

	route, err := rpc.ParseConnectRoutePath(path)
	if err != nil {
		t.Fatalf("ParseConnectRoutePath() error = %v", err)
	}
	want := rpc.ConnectRoute{
		Service: "lazuli.demo.v1.GreeterService",
		Method:  "SayHello",
		Path:    "/lazuli.demo.v1.GreeterService/SayHello",
	}
	if route != want {
		t.Fatalf("route = %#v, want %#v", route, want)
	}
}

func TestConnectRoutePathRejectsInvalidInput(t *testing.T) {
	t.Parallel()

	routeCases := []struct {
		name    string
		service string
		method  string
	}{
		{name: "empty service", method: "SayHello"},
		{name: "invalid service segment", service: "lazuli.demo.bad-service", method: "SayHello"},
		{name: "empty method", service: "lazuli.demo.v1.GreeterService"},
		{name: "method with dot", service: "lazuli.demo.v1.GreeterService", method: "Greeter.SayHello"},
		{name: "method with hyphen", service: "lazuli.demo.v1.GreeterService", method: "Say-Hello"},
	}
	for _, tc := range routeCases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			_, err := rpc.ConnectRoutePath(tc.service, tc.method)
			if !errors.Is(err, rpc.ErrInvalidConnectDescriptor) {
				t.Fatalf("ConnectRoutePath() error = %v, want ErrInvalidConnectDescriptor", err)
			}
		})
	}

	pathCases := []string{
		"",
		"https://example.test/lazuli.demo.v1.GreeterService/SayHello",
		"/lazuli.demo.v1.GreeterService/SayHello?debug=1",
		"/lazuli.demo.v1.GreeterService/Say-Hello",
		"/lazuli.demo.v1.GreeterService/SayHello/extra",
		"/.lazuli.demo.v1.GreeterService/SayHello",
	}
	for _, routePath := range pathCases {
		t.Run(routePath, func(t *testing.T) {
			t.Parallel()

			_, err := rpc.ParseConnectRoutePath(routePath)
			if !errors.Is(err, rpc.ErrInvalidConnectDescriptor) {
				t.Fatalf("ParseConnectRoutePath(%q) error = %v, want ErrInvalidConnectDescriptor", routePath, err)
			}
		})
	}
}

func TestNormalizeConnectMethodDescriptorMetadata(t *testing.T) {
	t.Parallel()

	sourceProtocols := []rpc.ConnectProtocolMode{rpc.ConnectProtocolModeGRPCWeb, rpc.ConnectProtocolMode("CONNECT")}
	method := rpc.ConnectMethodDescriptor{
		Service:         " .lazuli.demo.v1.GreeterService ",
		Method:          " SayHello ",
		RequestType:     ".lazuli.demo.v1.SayHelloRequest",
		ResponseType:    " lazuli.demo.v1.SayHelloResponse ",
		ServerStreaming: true,
		Protocols:       sourceProtocols,
		Compression:     rpc.ConnectCompressionRequest | rpc.ConnectCompressionResponse,
	}

	normalized, err := rpc.NormalizeConnectMethodDescriptor(method)
	if err != nil {
		t.Fatalf("NormalizeConnectMethodDescriptor() error = %v", err)
	}

	if normalized.Service != "lazuli.demo.v1.GreeterService" {
		t.Fatalf("Service = %q, want normalized service", normalized.Service)
	}
	if normalized.Method != "SayHello" {
		t.Fatalf("Method = %q, want SayHello", normalized.Method)
	}
	if normalized.RequestType != "lazuli.demo.v1.SayHelloRequest" {
		t.Fatalf("RequestType = %q, want normalized request type", normalized.RequestType)
	}
	if normalized.ResponseType != "lazuli.demo.v1.SayHelloResponse" {
		t.Fatalf("ResponseType = %q, want normalized response type", normalized.ResponseType)
	}
	wantProtocols := []rpc.ConnectProtocolMode{rpc.ConnectProtocolModeConnect, rpc.ConnectProtocolModeGRPCWeb}
	if !reflect.DeepEqual(normalized.Protocols, wantProtocols) {
		t.Fatalf("Protocols = %#v, want %#v", normalized.Protocols, wantProtocols)
	}
	if !normalized.SupportsProtocol(rpc.ConnectProtocolMode("grpc-web")) {
		t.Fatal("SupportsProtocol(grpc-web) = false, want true")
	}
	if normalized.SupportsProtocol(rpc.ConnectProtocolModeGRPC) {
		t.Fatal("SupportsProtocol(grpc) = true, want false")
	}

	path, err := normalized.RoutePath()
	if err != nil {
		t.Fatalf("RoutePath() error = %v", err)
	}
	if path != "/lazuli.demo.v1.GreeterService/SayHello" {
		t.Fatalf("RoutePath() = %q, want ConnectRPC path", path)
	}
	if !normalized.Compression.AllowsRequestCompression() || !normalized.Compression.AllowsResponseCompression() {
		t.Fatalf("Compression = %v, want request and response support", normalized.Compression)
	}
	if !normalized.Compression.Any() {
		t.Fatal("Compression.Any() = false, want true")
	}

	sourceProtocols[0] = rpc.ConnectProtocolModeGRPC
	if !reflect.DeepEqual(normalized.Protocols, wantProtocols) {
		t.Fatalf("normalized protocols changed after caller mutation: %#v", normalized.Protocols)
	}
}

func TestConnectProtocolModesDefaultAndValidation(t *testing.T) {
	t.Parallel()

	modes, err := rpc.NormalizeConnectProtocolModes(nil)
	if err != nil {
		t.Fatalf("NormalizeConnectProtocolModes(nil) error = %v", err)
	}
	if !reflect.DeepEqual(modes, []rpc.ConnectProtocolMode{rpc.ConnectProtocolModeConnect}) {
		t.Fatalf("default modes = %#v, want connect only", modes)
	}

	_, err = rpc.NormalizeConnectProtocolModes([]rpc.ConnectProtocolMode{
		rpc.ConnectProtocolModeConnect,
		rpc.ConnectProtocolMode("CONNECT"),
	})
	if !errors.Is(err, rpc.ErrInvalidConnectDescriptor) {
		t.Fatalf("NormalizeConnectProtocolModes(duplicate) error = %v, want ErrInvalidConnectDescriptor", err)
	}

	if err := rpc.ConnectProtocolMode("grpcweb").Validate(); !errors.Is(err, rpc.ErrInvalidConnectDescriptor) {
		t.Fatalf("ConnectProtocolMode.Validate() error = %v, want ErrInvalidConnectDescriptor", err)
	}
}

func TestConnectServiceDescriptorSortsCopiesAndLooksUpMethods(t *testing.T) {
	t.Parallel()

	methods := []rpc.ConnectMethodDescriptor{
		{
			Method:      "StreamUpdates",
			Protocols:   []rpc.ConnectProtocolMode{rpc.ConnectProtocolModeGRPC},
			Compression: rpc.ConnectCompressionResponse,
		},
		{
			Method:       "SayHello",
			RequestType:  "lazuli.demo.v1.SayHelloRequest",
			ResponseType: "lazuli.demo.v1.SayHelloResponse",
			Protocols:    []rpc.ConnectProtocolMode{rpc.ConnectProtocolModeConnect},
		},
	}

	descriptor, err := rpc.NewConnectServiceDescriptor(" lazuli.demo.v1.GreeterService ", methods)
	if err != nil {
		t.Fatalf("NewConnectServiceDescriptor() error = %v", err)
	}
	if descriptor.Service != "lazuli.demo.v1.GreeterService" {
		t.Fatalf("Service = %q, want normalized service", descriptor.Service)
	}
	if got := []string{descriptor.Methods[0].Method, descriptor.Methods[1].Method}; !reflect.DeepEqual(got, []string{"SayHello", "StreamUpdates"}) {
		t.Fatalf("method order = %#v, want sorted method names", got)
	}
	if descriptor.Methods[0].Service != descriptor.Service || descriptor.Methods[1].Service != descriptor.Service {
		t.Fatalf("methods did not inherit service: %#v", descriptor.Methods)
	}

	method, ok := descriptor.Method(" SayHello ")
	if !ok {
		t.Fatal("Method(SayHello) did not find descriptor")
	}
	method.Protocols[0] = rpc.ConnectProtocolModeGRPCWeb
	method, ok = descriptor.Method("SayHello")
	if !ok {
		t.Fatal("Method(SayHello) did not find descriptor after returned copy mutation")
	}
	if !reflect.DeepEqual(method.Protocols, []rpc.ConnectProtocolMode{rpc.ConnectProtocolModeConnect}) {
		t.Fatalf("Method returned shared protocols = %#v", method.Protocols)
	}

	methods[0].Protocols[0] = rpc.ConnectProtocolModeGRPCWeb
	if !reflect.DeepEqual(descriptor.Methods[1].Protocols, []rpc.ConnectProtocolMode{rpc.ConnectProtocolModeGRPC}) {
		t.Fatalf("descriptor protocols changed after caller mutation: %#v", descriptor.Methods[1].Protocols)
	}

	routes, err := descriptor.Routes()
	if err != nil {
		t.Fatalf("Routes() error = %v", err)
	}
	wantRoutes := []rpc.ConnectRoute{
		{
			Service: "lazuli.demo.v1.GreeterService",
			Method:  "SayHello",
			Path:    "/lazuli.demo.v1.GreeterService/SayHello",
		},
		{
			Service: "lazuli.demo.v1.GreeterService",
			Method:  "StreamUpdates",
			Path:    "/lazuli.demo.v1.GreeterService/StreamUpdates",
		},
	}
	if !reflect.DeepEqual(routes, wantRoutes) {
		t.Fatalf("Routes() = %#v, want %#v", routes, wantRoutes)
	}
}

func TestConnectServiceDescriptorRejectsInvalidAndDuplicates(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name       string
		service    string
		methods    []rpc.ConnectMethodDescriptor
		methodOnly *rpc.ConnectMethodDescriptor
		flags      *rpc.ConnectCompressionFlags
	}{
		{
			name:    "invalid service",
			service: "lazuli.demo.bad-service",
		},
		{
			name:    "method service mismatch",
			service: "lazuli.demo.v1.GreeterService",
			methods: []rpc.ConnectMethodDescriptor{
				{Service: "lazuli.demo.v1.OtherService", Method: "SayHello"},
			},
		},
		{
			name:    "duplicate method",
			service: "lazuli.demo.v1.GreeterService",
			methods: []rpc.ConnectMethodDescriptor{
				{Method: "SayHello"},
				{Method: " SayHello "},
			},
		},
		{
			name:    "duplicate protocol",
			service: "lazuli.demo.v1.GreeterService",
			methods: []rpc.ConnectMethodDescriptor{
				{Method: "SayHello", Protocols: []rpc.ConnectProtocolMode{rpc.ConnectProtocolModeConnect, rpc.ConnectProtocolMode("CONNECT")}},
			},
		},
		{
			name:       "missing response type",
			methodOnly: &rpc.ConnectMethodDescriptor{Service: "lazuli.demo.v1.GreeterService", Method: "SayHello", RequestType: "lazuli.demo.v1.SayHelloRequest"},
		},
		{
			name:  "unknown compression flag",
			flags: ptr(rpc.ConnectCompressionFlags(8)),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			var err error
			switch {
			case tc.methodOnly != nil:
				err = tc.methodOnly.Validate()
			case tc.flags != nil:
				err = tc.flags.Validate()
			default:
				_, err = rpc.NewConnectServiceDescriptor(tc.service, tc.methods)
			}
			if !errors.Is(err, rpc.ErrInvalidConnectDescriptor) {
				t.Fatalf("validation error = %v, want ErrInvalidConnectDescriptor", err)
			}
		})
	}
}

func TestConnectServiceDescriptorValidateDoesNotMutate(t *testing.T) {
	t.Parallel()

	descriptor := rpc.ConnectServiceDescriptor{
		Service: " .lazuli.demo.v1.GreeterService ",
		Methods: []rpc.ConnectMethodDescriptor{
			{
				Method:    " SayHello ",
				Protocols: []rpc.ConnectProtocolMode{rpc.ConnectProtocolMode("CONNECT")},
			},
		},
	}

	if err := descriptor.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if descriptor.Service != " .lazuli.demo.v1.GreeterService " {
		t.Fatalf("Validate mutated service = %q", descriptor.Service)
	}
	if descriptor.Methods[0].Method != " SayHello " {
		t.Fatalf("Validate mutated method = %q", descriptor.Methods[0].Method)
	}
	if descriptor.Methods[0].Protocols[0] != rpc.ConnectProtocolMode("CONNECT") {
		t.Fatalf("Validate mutated protocol = %q", descriptor.Methods[0].Protocols[0])
	}
}

func ptr[T any](value T) *T {
	return &value
}
