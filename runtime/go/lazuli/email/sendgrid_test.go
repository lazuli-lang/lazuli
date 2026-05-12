package email

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/sendgrid/sendgrid-go"
)

type sendgridPayload struct {
	From             payloadAddress `json:"from"`
	Subject          string         `json:"subject"`
	Personalizations []struct {
		To []payloadAddress `json:"to"`
	} `json:"personalizations"`
	Content []struct {
		Type  string `json:"type"`
		Value string `json:"value"`
	} `json:"content"`
}

type payloadAddress struct {
	Name  string `json:"name"`
	Email string `json:"email"`
}

func TestSendgridAdapterSendPostsMailSend(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Fatalf("method = %s, want POST", r.Method)
		}
		if r.URL.Path != "/v3/mail/send" {
			t.Fatalf("path = %s, want /v3/mail/send", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer test-key" {
			t.Fatalf("Authorization = %q, want bearer API key", got)
		}
		var payload sendgridPayload
		if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
			t.Fatalf("decode payload: %v", err)
		}
		if payload.From != (payloadAddress{Name: "Acme", Email: "noreply@acme.com"}) {
			t.Fatalf("from = %+v", payload.From)
		}
		if len(payload.Personalizations) != 1 || len(payload.Personalizations[0].To) != 1 {
			t.Fatalf("unexpected personalizations: %+v", payload.Personalizations)
		}
		if payload.Personalizations[0].To[0].Email != "user@example.com" {
			t.Fatalf("to = %+v", payload.Personalizations[0].To[0])
		}
		if payload.Subject != "Welcome" {
			t.Fatalf("subject = %q", payload.Subject)
		}
		if len(payload.Content) != 2 ||
			payload.Content[0].Type != "text/plain" ||
			payload.Content[0].Value != "plain body" ||
			payload.Content[1].Type != "text/html" ||
			payload.Content[1].Value != "<p>html body</p>" {
			t.Fatalf("content = %+v", payload.Content)
		}
		w.WriteHeader(http.StatusAccepted)
	}))
	defer server.Close()

	adapter := &SendgridAdapter{
		APIKey:    "test-key",
		From:      "Acme <noreply@acme.com>",
		newClient: testSendgridClient(server.URL),
	}
	if err := adapter.Send(context.Background(), "user@example.com", "Welcome", "<p>html body</p>", "plain body"); err != nil {
		t.Fatalf("Send: %v", err)
	}
}

func TestSendgridAdapterSendReturnsStatusError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "invalid sender", http.StatusBadRequest)
	}))
	defer server.Close()

	adapter := &SendgridAdapter{
		APIKey:    "test-key",
		From:      "noreply@example.com",
		newClient: testSendgridClient(server.URL),
	}
	err := adapter.Send(context.Background(), "user@example.com", "Welcome", "<p>html</p>", "text")
	if err == nil {
		t.Fatalf("expected status error")
	}
	if !strings.Contains(err.Error(), "sendgrid: status 400 body invalid sender") {
		t.Fatalf("error = %q", err)
	}
}

func TestSendgridAdapterSendHonorsCanceledContext(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		t.Fatalf("server should not receive canceled request")
	}))
	defer server.Close()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	adapter := &SendgridAdapter{
		APIKey:    "test-key",
		From:      "noreply@example.com",
		newClient: testSendgridClient(server.URL),
	}
	err := adapter.Send(ctx, "user@example.com", "Welcome", "<p>html</p>", "text")
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("expected context.Canceled, got %v", err)
	}
}

func testSendgridClient(host string) func(string) *sendgrid.Client {
	return func(apiKey string) *sendgrid.Client {
		client := sendgrid.NewSendClient(apiKey)
		client.Request.BaseURL = host + "/v3/mail/send"
		return client
	}
}
