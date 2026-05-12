package email

import (
	"context"
	"fmt"
	"net/smtp"
	"strings"
)

// SMTPAdapter sends via plain SMTP. Use SendgridAdapter for production.
type SMTPAdapter struct {
	Host     string // "smtp.gmail.com"
	Port     int    // 587 (STARTTLS) or 465 (TLS)
	Username string
	Password string
	From     string
}

func (a *SMTPAdapter) Send(ctx context.Context, to, subject, htmlBody, textBody string) error {
	auth := smtp.PlainAuth("", a.Username, a.Password, a.Host)
	addr := fmt.Sprintf("%s:%d", a.Host, a.Port)
	msg := []byte(buildMimeEmail(a.From, to, subject, textBody, htmlBody))
	return smtp.SendMail(addr, auth, a.From, []string{to}, msg)
}

func buildMimeEmail(from, to, subject, textBody, htmlBody string) string {
	// Multipart MIME with text + html alternative.
	var b strings.Builder
	fmt.Fprintf(&b, "From: %s\r\n", from)
	fmt.Fprintf(&b, "To: %s\r\n", to)
	fmt.Fprintf(&b, "Subject: %s\r\n", subject)
	boundary := "lazuli_mime_boundary"
	fmt.Fprintf(&b, "MIME-Version: 1.0\r\n")
	fmt.Fprintf(&b, "Content-Type: multipart/alternative; boundary=%s\r\n\r\n", boundary)
	fmt.Fprintf(&b, "--%s\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n%s\r\n", boundary, textBody)
	fmt.Fprintf(&b, "--%s\r\nContent-Type: text/html; charset=utf-8\r\n\r\n%s\r\n", boundary, htmlBody)
	fmt.Fprintf(&b, "--%s--\r\n", boundary)
	return b.String()
}
