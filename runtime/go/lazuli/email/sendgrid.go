// Package email wires email channel adapters for Lazuli notifications.
package email

import (
	"context"
	"fmt"
	netmail "net/mail"

	"github.com/sendgrid/sendgrid-go"
	"github.com/sendgrid/sendgrid-go/helpers/mail"
)

// SendgridAdapter sends via Sendgrid's REST API.
type SendgridAdapter struct {
	APIKey string
	// From accepts either "sender@example.com" or "Name <sender@example.com>".
	From string

	newClient func(apiKey string) *sendgrid.Client
}

// Send dispatches a single email. It honors ctx cancellation via
// Sendgrid's SendWithContext HTTP path.
func (a *SendgridAdapter) Send(ctx context.Context, to, subject, htmlBody, textBody string) error {
	message := mail.NewSingleEmail(emailAddress(a.From), subject, emailAddress(to), textBody, htmlBody)
	resp, err := a.sendClient().SendWithContext(ctx, message)
	if err != nil {
		return err
	}
	if resp.StatusCode >= 400 {
		return fmt.Errorf("sendgrid: status %d body %s", resp.StatusCode, resp.Body)
	}
	return nil
}

func (a *SendgridAdapter) sendClient() *sendgrid.Client {
	if a.newClient != nil {
		return a.newClient(a.APIKey)
	}
	return sendgrid.NewSendClient(a.APIKey)
}

func emailAddress(raw string) *mail.Email {
	addr, err := netmail.ParseAddress(raw)
	if err != nil {
		return mail.NewEmail("", raw)
	}
	return mail.NewEmail(addr.Name, addr.Address)
}
