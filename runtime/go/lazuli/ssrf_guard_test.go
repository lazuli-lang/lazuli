package lazuli

import (
	"context"
	"errors"
	"net"
	"testing"
)

func TestValidateOutboundURLAllowsResolvedPublicHTTP(t *testing.T) {
	called := false
	resolver := SSRFResolverFunc(func(ctx context.Context, host string) ([]net.IPAddr, error) {
		called = true
		if ctx == nil {
			t.Fatal("resolver ctx is nil")
		}
		if host != "api.example.com" {
			t.Fatalf("resolver host = %q, want %q", host, "api.example.com")
		}
		return []net.IPAddr{{IP: net.ParseIP("93.184.216.34")}}, nil
	})

	err := ValidateOutboundURL(context.Background(), "https://api.example.com/webhook", SSRFGuard{}, resolver)

	if err != nil {
		t.Fatalf("ValidateOutboundURL error = %v", err)
	}
	if !called {
		t.Fatal("resolver was not called")
	}
}

func TestValidateOutboundURLRejectsNonHTTPSchemes(t *testing.T) {
	called := false
	resolver := SSRFResolverFunc(func(context.Context, string) ([]net.IPAddr, error) {
		called = true
		return nil, nil
	})

	err := ValidateOutboundURL(context.Background(), "file:///etc/passwd", SSRFGuard{}, resolver)

	if !errors.Is(err, ErrSSRFBlocked) {
		t.Fatalf("ValidateOutboundURL error = %v, want ErrSSRFBlocked", err)
	}
	if called {
		t.Fatal("resolver was called for denied scheme")
	}
}

func TestValidateOutboundURLRejectsReservedLiteralIPs(t *testing.T) {
	tests := []string{
		"http://0.0.0.0/",
		"http://10.1.2.3/",
		"http://127.0.0.1/",
		"http://169.254.1.2/",
		"http://224.0.0.1/",
		"http://[::]/",
		"http://[::1]/",
		"http://[fc00::1]/",
		"http://[fe80::1]/",
		"http://[ff02::1]/",
	}

	for _, rawURL := range tests {
		t.Run(rawURL, func(t *testing.T) {
			err := ValidateOutboundURL(context.Background(), rawURL, SSRFGuard{}, nil)
			if !errors.Is(err, ErrSSRFBlocked) {
				t.Fatalf("ValidateOutboundURL(%q) error = %v, want ErrSSRFBlocked", rawURL, err)
			}
		})
	}
}

func TestValidateOutboundURLRejectsReservedResolvedIPs(t *testing.T) {
	resolver := SSRFResolverFunc(func(context.Context, string) ([]net.IPAddr, error) {
		return []net.IPAddr{
			{IP: net.ParseIP("93.184.216.34")},
			{IP: net.ParseIP("127.0.0.1")},
		}, nil
	})

	err := ValidateOutboundURL(context.Background(), "https://api.example.com", SSRFGuard{}, resolver)

	if !errors.Is(err, ErrSSRFBlocked) {
		t.Fatalf("ValidateOutboundURL error = %v, want ErrSSRFBlocked", err)
	}
}

func TestValidateOutboundURLAllowedCIDRPermitsNormallyDeniedAddress(t *testing.T) {
	resolver := SSRFResolverFunc(func(context.Context, string) ([]net.IPAddr, error) {
		return []net.IPAddr{{IP: net.ParseIP("10.1.2.3")}}, nil
	})
	guard := SSRFGuard{AllowedCIDRs: []string{"10.0.0.0/8"}}

	if err := ValidateOutboundURL(context.Background(), "https://internal.example.com", guard, resolver); err != nil {
		t.Fatalf("ValidateOutboundURL resolved address error = %v", err)
	}
	if err := ValidateOutboundURL(context.Background(), "https://10.1.2.3", guard, nil); err != nil {
		t.Fatalf("ValidateOutboundURL literal address error = %v", err)
	}
}

func TestValidateOutboundURLAllowedHostsRestrictsHostnames(t *testing.T) {
	var resolved []string
	resolver := SSRFResolverFunc(func(_ context.Context, host string) ([]net.IPAddr, error) {
		resolved = append(resolved, host)
		return []net.IPAddr{{IP: net.ParseIP("93.184.216.34")}}, nil
	})
	guard := SSRFGuard{
		AllowedHosts: []string{"api.example.com", "*.trusted.example"},
	}

	if err := ValidateOutboundURL(context.Background(), "https://api.example.com", guard, resolver); err != nil {
		t.Fatalf("ValidateOutboundURL exact host error = %v", err)
	}
	if err := ValidateOutboundURL(context.Background(), "https://jobs.trusted.example", guard, resolver); err != nil {
		t.Fatalf("ValidateOutboundURL wildcard host error = %v", err)
	}

	err := ValidateOutboundURL(context.Background(), "https://evil.example", guard, resolver)
	if !errors.Is(err, ErrSSRFBlocked) {
		t.Fatalf("ValidateOutboundURL rejected host error = %v, want ErrSSRFBlocked", err)
	}
	if len(resolved) != 2 {
		t.Fatalf("resolver calls = %d, want 2", len(resolved))
	}
}

func TestValidateOutboundURLRejectsFailedResolution(t *testing.T) {
	resolver := SSRFResolverFunc(func(context.Context, string) ([]net.IPAddr, error) {
		return nil, errors.New("dns unavailable")
	})

	err := ValidateOutboundURL(context.Background(), "https://api.example.com", SSRFGuard{}, resolver)

	if !errors.Is(err, ErrSSRFBlocked) {
		t.Fatalf("ValidateOutboundURL error = %v, want ErrSSRFBlocked", err)
	}
}

func TestValidateOutboundURLRejectsInvalidAllowlistCIDR(t *testing.T) {
	err := ValidateOutboundURL(context.Background(), "https://api.example.com", SSRFGuard{
		AllowedCIDRs: []string{"not-a-cidr"},
	}, SSRFResolverFunc(func(context.Context, string) ([]net.IPAddr, error) {
		return []net.IPAddr{{IP: net.ParseIP("93.184.216.34")}}, nil
	}))

	if err == nil {
		t.Fatal("ValidateOutboundURL error = nil, want invalid CIDR error")
	}
}
