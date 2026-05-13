package email

import (
	"net"
	"net/smtp"
	"net/textproto"
	"strings"
	"testing"
	"time"
)

func TestSMTPDevServerCapturesMessage(t *testing.T) {
	t.Parallel()

	server := NewSMTPDevServer()
	captured := make(chan SMTPDevMessage, 1)
	server.OnMessage = func(message SMTPDevMessage) {
		captured <- message
	}

	listener, done := serveSMTPDevServer(t, server)
	defer closeSMTPDevServer(t, listener, done)

	body := "From: sender@example.com\r\nTo: first@example.com\r\nSubject: Test\r\n\r\nHello.\r\n"
	err := smtp.SendMail(
		listener.Addr().String(),
		nil,
		"sender@example.com",
		[]string{"first@example.com", "second@example.com"},
		[]byte(body),
	)
	if err != nil {
		t.Fatalf("SendMail() error = %v", err)
	}

	var callbackMessage SMTPDevMessage
	select {
	case callbackMessage = <-captured:
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for capture callback")
	}

	messages := server.Messages()
	if len(messages) != 1 {
		t.Fatalf("Messages() length = %d, want 1", len(messages))
	}

	message := messages[0]
	if message.From != "sender@example.com" {
		t.Fatalf("message.From = %q, want sender@example.com", message.From)
	}
	if got := strings.Join(message.To, ","); got != "first@example.com,second@example.com" {
		t.Fatalf("message.To = %q, want both recipients", got)
	}
	if string(message.Data) != body {
		t.Fatalf("message.Data = %q, want %q", message.Data, body)
	}
	if callbackMessage.From != message.From || string(callbackMessage.Data) != body {
		t.Fatalf("callback message = %+v, want stored message", callbackMessage)
	}

	messages[0].To[0] = "mutated@example.com"
	messages[0].Data[0] = 'X'
	again := server.Messages()[0]
	if again.To[0] != "first@example.com" || string(again.Data) != body {
		t.Fatalf("Messages() returned mutable internal storage: %+v", again)
	}

	server.Reset()
	if got := len(server.Messages()); got != 0 {
		t.Fatalf("Messages() length after Reset() = %d, want 0", got)
	}
}

func TestSMTPDevServerProtocolCommands(t *testing.T) {
	t.Parallel()

	server := NewSMTPDevServer()
	listener, done := serveSMTPDevServer(t, server)
	defer closeSMTPDevServer(t, listener, done)

	conn, err := net.Dial("tcp", listener.Addr().String())
	if err != nil {
		t.Fatalf("Dial() error = %v", err)
	}
	defer conn.Close()

	client := textproto.NewConn(conn)
	defer client.Close()

	if got := readSMTPDevCode(t, client); got != 220 {
		t.Fatalf("greeting code = %d, want 220", got)
	}
	writeSMTPDevLine(t, client, "NOOP")
	if got := readSMTPDevCode(t, client); got != 250 {
		t.Fatalf("NOOP code = %d, want 250", got)
	}
	writeSMTPDevLine(t, client, "RCPT TO:<first@example.com>")
	if got := readSMTPDevCode(t, client); got != 503 {
		t.Fatalf("RCPT before MAIL code = %d, want 503", got)
	}
	writeSMTPDevLine(t, client, "EHLO example.test")
	if got := readSMTPDevCode(t, client); got != 250 {
		t.Fatalf("EHLO code = %d, want 250", got)
	}
	writeSMTPDevLine(t, client, "MAIL FROM:<sender@example.com>")
	if got := readSMTPDevCode(t, client); got != 250 {
		t.Fatalf("MAIL code = %d, want 250", got)
	}
	writeSMTPDevLine(t, client, "RSET")
	if got := readSMTPDevCode(t, client); got != 250 {
		t.Fatalf("RSET code = %d, want 250", got)
	}
	writeSMTPDevLine(t, client, "DATA")
	if got := readSMTPDevCode(t, client); got != 503 {
		t.Fatalf("DATA after RSET code = %d, want 503", got)
	}
	writeSMTPDevLine(t, client, "MAIL FROM:<sender@example.com>")
	if got := readSMTPDevCode(t, client); got != 250 {
		t.Fatalf("second MAIL code = %d, want 250", got)
	}
	writeSMTPDevLine(t, client, "RCPT TO:<first@example.com>")
	if got := readSMTPDevCode(t, client); got != 250 {
		t.Fatalf("RCPT code = %d, want 250", got)
	}
	writeSMTPDevLine(t, client, "DATA")
	if got := readSMTPDevCode(t, client); got != 354 {
		t.Fatalf("DATA code = %d, want 354", got)
	}
	writeSMTPDevLine(t, client, "Subject: Dot")
	writeSMTPDevLine(t, client, "")
	writeSMTPDevLine(t, client, "..visible")
	writeSMTPDevLine(t, client, ".")
	if got := readSMTPDevCode(t, client); got != 250 {
		t.Fatalf("DATA completion code = %d, want 250", got)
	}
	writeSMTPDevLine(t, client, "QUIT")
	if got := readSMTPDevCode(t, client); got != 221 {
		t.Fatalf("QUIT code = %d, want 221", got)
	}

	messages := server.Messages()
	if len(messages) != 1 {
		t.Fatalf("Messages() length = %d, want 1", len(messages))
	}
	if messages[0].HELO != "example.test" {
		t.Fatalf("message.HELO = %q, want example.test", messages[0].HELO)
	}
	if string(messages[0].Data) != "Subject: Dot\r\n\r\n.visible\r\n" {
		t.Fatalf("message.Data = %q, want dot-unescaped data", messages[0].Data)
	}
}

func TestSMTPDevServerRejectsOversizedMessage(t *testing.T) {
	t.Parallel()

	server := NewSMTPDevServer()
	server.MaxMessageBytes = 4
	listener, done := serveSMTPDevServer(t, server)
	defer closeSMTPDevServer(t, listener, done)

	conn, err := net.Dial("tcp", listener.Addr().String())
	if err != nil {
		t.Fatalf("Dial() error = %v", err)
	}
	defer conn.Close()

	client := textproto.NewConn(conn)
	defer client.Close()

	readSMTPDevCode(t, client)
	writeSMTPDevLine(t, client, "HELO example.test")
	readSMTPDevCode(t, client)
	writeSMTPDevLine(t, client, "MAIL FROM:<sender@example.com>")
	readSMTPDevCode(t, client)
	writeSMTPDevLine(t, client, "RCPT TO:<first@example.com>")
	readSMTPDevCode(t, client)
	writeSMTPDevLine(t, client, "DATA")
	readSMTPDevCode(t, client)
	writeSMTPDevLine(t, client, "too long")
	writeSMTPDevLine(t, client, ".")
	if got := readSMTPDevCode(t, client); got != 552 {
		t.Fatalf("oversized message code = %d, want 552", got)
	}
	if len(server.Messages()) != 0 {
		t.Fatalf("oversized message was captured")
	}
}

func serveSMTPDevServer(t *testing.T, server *SMTPDevServer) (net.Listener, chan error) {
	t.Helper()

	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("Listen() error = %v", err)
	}
	done := make(chan error, 1)
	go func() {
		done <- server.Serve(listener)
	}()
	return listener, done
}

func closeSMTPDevServer(t *testing.T, listener net.Listener, done chan error) {
	t.Helper()

	if err := listener.Close(); err != nil {
		t.Fatalf("Close() error = %v", err)
	}
	select {
	case err := <-done:
		if err != nil {
			t.Fatalf("Serve() error = %v", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for Serve() to stop")
	}
}

func writeSMTPDevLine(t *testing.T, client *textproto.Conn, line string) {
	t.Helper()
	if err := client.PrintfLine("%s", line); err != nil {
		t.Fatalf("PrintfLine(%q) error = %v", line, err)
	}
}

func readSMTPDevCode(t *testing.T, client *textproto.Conn) int {
	t.Helper()

	code, _, err := client.ReadResponse(-1)
	if err != nil {
		t.Fatalf("ReadResponse() error = %v", err)
	}
	return code
}
