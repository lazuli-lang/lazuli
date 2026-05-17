// SMTPDispatcher shape tests. Stdlib `net/smtp` itself is tested
// upstream; this file exercises the construction + envelope + config
// surface the framework owns. Live SMTP delivery is out of scope here
// (covered by integration tests on a real MX target).
package notifications_test

import (
	"context"
	"testing"

	"lazuli.dev/runtime/lazuli/notifications"
)

func TestSMTPConfigFromEnvEmptyHostReturnsZero(t *testing.T) {
	t.Setenv("SMTP_HOST", "")
	cfg := notifications.SMTPConfigFromEnv()
	if cfg.Host != "" || cfg.Port != "" {
		t.Fatalf("expected zero config when SMTP_HOST unset, got %+v", cfg)
	}
}

func TestSMTPConfigFromEnvDefaultsPort(t *testing.T) {
	t.Setenv("SMTP_HOST", "smtp.example.com")
	t.Setenv("SMTP_PORT", "")
	t.Setenv("SMTP_USER", "noreply@example.com")
	t.Setenv("SMTP_FROM_ADDRESS", "")
	cfg := notifications.SMTPConfigFromEnv()
	if cfg.Port != "587" {
		t.Fatalf("expected default port 587, got %q", cfg.Port)
	}
	if cfg.FromAddress != "noreply@example.com" {
		t.Fatalf("expected FromAddress to fall back to User, got %q", cfg.FromAddress)
	}
}

func TestNewSMTPDispatcherReturnsNilWhenHostEmpty(t *testing.T) {
	d := notifications.NewSMTPDispatcher(notifications.SMTPConfig{})
	if d != nil {
		t.Fatalf("expected nil dispatcher when host empty, got %v", d)
	}
}

func TestNewSMTPDispatcherReturnsNilWhenNoFromAddress(t *testing.T) {
	d := notifications.NewSMTPDispatcher(notifications.SMTPConfig{Host: "smtp.example.com"})
	if d != nil {
		t.Fatalf("expected nil dispatcher when no From available, got %v", d)
	}
}

func TestNewSMTPDispatcherReturnsDispatcherWhenConfigured(t *testing.T) {
	d := notifications.NewSMTPDispatcher(notifications.SMTPConfig{
		Host:        "smtp.example.com",
		Port:        "587",
		FromAddress: "noreply@example.com",
	})
	if d == nil {
		t.Fatal("expected dispatcher when host + from set")
	}
	if d.Channel() != notifications.ChannelEmail {
		t.Fatalf("expected ChannelEmail, got %v", d.Channel())
	}
}

func TestSMTPDispatcherRejectsEmptyRecipient(t *testing.T) {
	d := notifications.NewSMTPDispatcher(notifications.SMTPConfig{
		Host:        "smtp.example.com",
		FromAddress: "noreply@example.com",
	})
	err := d.Dispatch(context.Background(), notifications.Envelope{})
	if err == nil {
		t.Fatal("expected error on empty recipient")
	}
}

func TestSMTPDispatcherRejectsEmptySubjectAndBody(t *testing.T) {
	d := notifications.NewSMTPDispatcher(notifications.SMTPConfig{
		Host:        "smtp.example.com",
		FromAddress: "noreply@example.com",
	})
	err := d.Dispatch(context.Background(), notifications.Envelope{
		Recipient:    "user@example.com",
		TemplateData: map[string]any{},
	})
	if err == nil {
		t.Fatal("expected error when subject and body both empty")
	}
}

func TestSMTPDispatcherCustomSubjectAndBodyFns(t *testing.T) {
	d := notifications.NewSMTPDispatcher(notifications.SMTPConfig{
		Host:        "smtp.example.com",
		FromAddress: "noreply@example.com",
	})
	d.SubjectFn = func(notifications.Envelope) string { return "" }
	d.BodyFn = func(notifications.Envelope) string { return "" }
	err := d.Dispatch(context.Background(), notifications.Envelope{
		Recipient: "user@example.com",
	})
	if err == nil {
		t.Fatal("expected error when custom fns return empty")
	}
}
