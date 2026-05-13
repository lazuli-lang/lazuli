package email

import (
	"errors"
	"strings"
	"testing"
)

func TestSMTPServerDescriptorValidateAcceptsDescriptor(t *testing.T) {
	t.Parallel()

	descriptor := validSMTPServerDescriptor()
	if err := descriptor.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if got := descriptor.BindAddr(); got != "127.0.0.1:2525" {
		t.Fatalf("BindAddr() = %q, want 127.0.0.1:2525", got)
	}
	if !SMTPServerTLSStartTLS.Valid() {
		t.Fatalf("SMTPServerTLSStartTLS.Valid() = false, want true")
	}
	if !SMTPServerAuthRequired.Valid() {
		t.Fatalf("SMTPServerAuthRequired.Valid() = false, want true")
	}
}

func TestSMTPServerDescriptorNormalize(t *testing.T) {
	t.Parallel()

	descriptor := SMTPServerDescriptor{
		BindAddress: SMTPServerBindAddress{Host: " localhost ", Port: 2465},
		TLSMode:     "TLS",
		AuthMode:    "Require",
		Routes: []SMTPMailboxRoute{
			{Mailbox: " support ", Recipient: "Support@Example.COM"},
			{Mailbox: " domain ", Domain: "Example.NET"},
		},
	}

	normalized := descriptor.Normalize()
	if normalized.BindAddress.Host != "localhost" {
		t.Fatalf("normalized host = %q, want localhost", normalized.BindAddress.Host)
	}
	if normalized.TLSMode != SMTPServerTLSImplicit {
		t.Fatalf("normalized TLSMode = %q, want %q", normalized.TLSMode, SMTPServerTLSImplicit)
	}
	if normalized.AuthMode != SMTPServerAuthRequired {
		t.Fatalf("normalized AuthMode = %q, want %q", normalized.AuthMode, SMTPServerAuthRequired)
	}
	if normalized.Routes[0].Mailbox != "support" || normalized.Routes[0].Recipient != "support@example.com" {
		t.Fatalf("normalized exact route = %+v", normalized.Routes[0])
	}
	if normalized.Routes[1].Mailbox != "domain" || normalized.Routes[1].Domain != "example.net" {
		t.Fatalf("normalized domain route = %+v", normalized.Routes[1])
	}
}

func TestResolveSMTPMailboxRoutePrecedence(t *testing.T) {
	t.Parallel()

	routes := []SMTPMailboxRoute{
		{Mailbox: "catch-all"},
		{Mailbox: "example-domain", Domain: "example.com"},
		{Mailbox: "exact-user", Recipient: "user@example.com"},
	}

	route, ok, err := ResolveSMTPMailboxRoute(routes, "USER@example.com")
	if err != nil {
		t.Fatalf("ResolveSMTPMailboxRoute(exact) error = %v", err)
	}
	if !ok || route.Mailbox != "exact-user" {
		t.Fatalf("ResolveSMTPMailboxRoute(exact) = %+v, %v; want exact-user", route, ok)
	}

	route, ok, err = ResolveSMTPMailboxRoute(routes, "team@example.com")
	if err != nil {
		t.Fatalf("ResolveSMTPMailboxRoute(domain) error = %v", err)
	}
	if !ok || route.Mailbox != "example-domain" {
		t.Fatalf("ResolveSMTPMailboxRoute(domain) = %+v, %v; want example-domain", route, ok)
	}

	route, ok, err = ResolveSMTPMailboxRoute(routes, "other@example.net")
	if err != nil {
		t.Fatalf("ResolveSMTPMailboxRoute(catch-all) error = %v", err)
	}
	if !ok || route.Mailbox != "catch-all" {
		t.Fatalf("ResolveSMTPMailboxRoute(catch-all) = %+v, %v; want catch-all", route, ok)
	}
}

func TestResolveSMTPMailboxRouteNoMatch(t *testing.T) {
	t.Parallel()

	route, ok, err := ResolveSMTPMailboxRoute(
		[]SMTPMailboxRoute{{Mailbox: "example-domain", Domain: "example.com"}},
		"user@example.net",
	)
	if err != nil {
		t.Fatalf("ResolveSMTPMailboxRoute() error = %v", err)
	}
	if ok {
		t.Fatalf("ResolveSMTPMailboxRoute() = %+v, true; want no match", route)
	}
}

func TestValidateSMTPServerDescriptorRejectsInvalidShape(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		mutate   func(*SMTPServerDescriptor)
		wantText string
	}{
		{
			name: "missing port",
			mutate: func(d *SMTPServerDescriptor) {
				d.BindAddress.Port = 0
			},
			wantText: "port",
		},
		{
			name: "host includes port",
			mutate: func(d *SMTPServerDescriptor) {
				d.BindAddress.Host = "127.0.0.1:2525"
			},
			wantText: "host",
		},
		{
			name: "invalid tls",
			mutate: func(d *SMTPServerDescriptor) {
				d.TLSMode = "ssl"
			},
			wantText: "tls_mode",
		},
		{
			name: "invalid auth",
			mutate: func(d *SMTPServerDescriptor) {
				d.AuthMode = "plain"
			},
			wantText: "auth_mode",
		},
		{
			name: "missing routes",
			mutate: func(d *SMTPServerDescriptor) {
				d.Routes = nil
			},
			wantText: "route",
		},
		{
			name: "route missing mailbox",
			mutate: func(d *SMTPServerDescriptor) {
				d.Routes[0].Mailbox = " "
			},
			wantText: "mailbox",
		},
		{
			name: "route recipient and domain",
			mutate: func(d *SMTPServerDescriptor) {
				d.Routes[0].Domain = "example.com"
			},
			wantText: "mutually exclusive",
		},
		{
			name: "route invalid recipient",
			mutate: func(d *SMTPServerDescriptor) {
				d.Routes[0].Recipient = "not an address"
			},
			wantText: "recipient",
		},
		{
			name: "duplicate domain",
			mutate: func(d *SMTPServerDescriptor) {
				d.Routes = []SMTPMailboxRoute{
					{Mailbox: "a", Domain: "Example.com"},
					{Mailbox: "b", Domain: "example.com"},
				}
			},
			wantText: "duplicates",
		},
		{
			name: "duplicate catch all",
			mutate: func(d *SMTPServerDescriptor) {
				d.Routes = []SMTPMailboxRoute{
					{Mailbox: "a"},
					{Mailbox: "b"},
				}
			},
			wantText: "duplicates",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			descriptor := validSMTPServerDescriptor()
			tt.mutate(&descriptor)

			err := ValidateSMTPServerDescriptor(descriptor)
			if !errors.Is(err, ErrInvalidSMTPServerDescriptor) {
				t.Fatalf("ValidateSMTPServerDescriptor() error = %v, want ErrInvalidSMTPServerDescriptor", err)
			}
			if !strings.Contains(err.Error(), tt.wantText) {
				t.Fatalf("ValidateSMTPServerDescriptor() error = %q, want %q", err, tt.wantText)
			}
		})
	}
}

func TestResolveSMTPMailboxRouteRejectsInvalidRecipient(t *testing.T) {
	t.Parallel()

	_, _, err := ResolveSMTPMailboxRoute(validSMTPServerDescriptor().Routes, "bad recipient")
	if !errors.Is(err, ErrInvalidSMTPServerDescriptor) {
		t.Fatalf("ResolveSMTPMailboxRoute() error = %v, want ErrInvalidSMTPServerDescriptor", err)
	}
}

func validSMTPServerDescriptor() SMTPServerDescriptor {
	return SMTPServerDescriptor{
		BindAddress: SMTPServerBindAddress{Host: "127.0.0.1", Port: 2525},
		TLSMode:     SMTPServerTLSStartTLS,
		AuthMode:    SMTPServerAuthRequired,
		Routes: []SMTPMailboxRoute{
			{Mailbox: "support", Recipient: "support@example.com"},
			{Mailbox: "example-domain", Domain: "example.com"},
			{Mailbox: "catch-all"},
		},
	}
}
