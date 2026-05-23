// Package notifications — stdlib SMTP dispatcher for `ChannelEmail`.
//
// Wire-thin: this is `net/smtp.SendMail` plus env-driven config. No
// queue (the framework's `retry` policy handles transient failures),
// no template engine (`Envelope.TemplateData` is rendered upstream by
// the caller — see codegen), no per-vendor quirks (SendGrid / Postmark
// / SES live in `@lazuli/plugin-notification-<vendor>` per scope-discipline).
//
// Activates when SMTP_HOST is set; otherwise `NewSMTPDispatcher`
// returns nil, signaling "no transport bound" so the registry skips
// the email channel (existing `ErrNotificationChannelUnsupported`
// path).
//
// Per bucket-notifications-expanded-cycle.md 2026-05-17 ship trigger,
// the pilot evidence is the pilot-A `smtp.go` wire-thin adapter that
// established the env-var shape.
package notifications

import (
	"context"
	"errors"
	"fmt"
	"net/smtp"
	"os"
)

// SMTPConfig captures the wire-thin env-derived SMTP knobs. Empty
// fields fall back to either defaults or `nil`-auth (anonymous SMTP).
type SMTPConfig struct {
	Host        string // SMTP_HOST
	Port        string // SMTP_PORT (default "587")
	User        string // SMTP_USER
	Password    string // SMTP_PASSWORD
	FromAddress string // SMTP_FROM_ADDRESS (falls back to User)
}

// SMTPConfigFromEnv reads the closed env-var set. Returns the zero
// value when SMTP_HOST is unset so callers can opt into the channel.
func SMTPConfigFromEnv() SMTPConfig {
	host := os.Getenv("SMTP_HOST")
	if host == "" {
		return SMTPConfig{}
	}
	port := os.Getenv("SMTP_PORT")
	if port == "" {
		port = "587"
	}
	user := os.Getenv("SMTP_USER")
	from := os.Getenv("SMTP_FROM_ADDRESS")
	if from == "" {
		from = user
	}
	return SMTPConfig{
		Host:        host,
		Port:        port,
		User:        user,
		Password:    os.Getenv("SMTP_PASSWORD"),
		FromAddress: from,
	}
}

// SMTPDispatcher is the stdlib `net/smtp` ChannelDispatcher binding
// for `ChannelEmail`. Construct via `NewSMTPDispatcher` and register
// with `Registry.Register`.
type SMTPDispatcher struct {
	cfg SMTPConfig
	// SubjectFn extracts the message subject from the envelope's
	// template data. Defaults to `Envelope.TemplateData["subject"]`.
	SubjectFn func(Envelope) string
	// BodyFn extracts the message body. Defaults to
	// `Envelope.TemplateData["body"]`.
	BodyFn func(Envelope) string
}

// NewSMTPDispatcher returns a configured dispatcher, or nil when SMTP
// is not configured (SMTP_HOST unset). Caller decides whether nil
// means "skip email" or "fail loud".
func NewSMTPDispatcher(cfg SMTPConfig) *SMTPDispatcher {
	if cfg.Host == "" {
		return nil
	}
	if cfg.FromAddress == "" && cfg.User == "" {
		return nil
	}
	return &SMTPDispatcher{cfg: cfg}
}

// Channel implements ChannelDispatcher.
func (d *SMTPDispatcher) Channel() Channel { return ChannelEmail }

// Dispatch implements ChannelDispatcher. The envelope's recipient is
// the To address; subject + body come from TemplateData via SubjectFn
// + BodyFn (defaults read the canonical keys).
func (d *SMTPDispatcher) Dispatch(_ context.Context, env Envelope) error {
	if env.Recipient == "" {
		return errors.New("notifications/smtp: empty recipient")
	}
	subject := d.subject(env)
	body := d.body(env)
	if subject == "" && body == "" {
		return errors.New("notifications/smtp: empty subject and body")
	}

	from := d.cfg.FromAddress
	if from == "" {
		from = d.cfg.User
	}

	msg := fmt.Sprintf(
		"From: %s\r\nTo: %s\r\nSubject: %s\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n%s",
		from, env.Recipient, subject, body,
	)

	var auth smtp.Auth
	if d.cfg.User != "" {
		auth = smtp.PlainAuth("", d.cfg.User, d.cfg.Password, d.cfg.Host)
	}

	addr := fmt.Sprintf("%s:%s", d.cfg.Host, d.cfg.Port)
	if err := smtp.SendMail(addr, auth, from, []string{env.Recipient}, []byte(msg)); err != nil {
		return fmt.Errorf("notifications/smtp send to %s: %w", env.Recipient, err)
	}
	return nil
}

func (d *SMTPDispatcher) subject(env Envelope) string {
	if d.SubjectFn != nil {
		return d.SubjectFn(env)
	}
	return stringFrom(env.TemplateData, "subject")
}

func (d *SMTPDispatcher) body(env Envelope) string {
	if d.BodyFn != nil {
		return d.BodyFn(env)
	}
	return stringFrom(env.TemplateData, "body")
}

func stringFrom(m map[string]any, key string) string {
	if m == nil {
		return ""
	}
	v, ok := m[key]
	if !ok {
		return ""
	}
	s, ok := v.(string)
	if !ok {
		return ""
	}
	return s
}
