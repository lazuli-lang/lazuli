package notifications

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
)

func TestExpoPushProviderDispatchSendsRequestShape(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Fatalf("method = %s, want POST", r.Method)
		}
		if r.URL.Path != expoPushSendPath {
			t.Fatalf("path = %q, want %q", r.URL.Path, expoPushSendPath)
		}
		if got := r.Header.Get("Accept"); got != "application/json" {
			t.Fatalf("Accept = %q, want application/json", got)
		}
		if got := r.Header.Get("Content-Type"); got != "application/json" {
			t.Fatalf("Content-Type = %q, want application/json", got)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer test-token" {
			t.Fatalf("Authorization = %q, want bearer token", got)
		}

		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode body: %v", err)
		}
		if got := body["to"]; got != "ExpoPushToken[device-1]" {
			t.Fatalf("to = %v, want ExpoPushToken[device-1]", got)
		}
		if got := body["title"]; got != "Gate changed" {
			t.Fatalf("title = %v, want Gate changed", got)
		}
		if got := body["body"]; got != "Use entrance B" {
			t.Fatalf("body = %v, want Use entrance B", got)
		}
		data, ok := body["data"].(map[string]any)
		if !ok {
			t.Fatalf("data = %T, want object", body["data"])
		}
		if got := data["booking_id"]; got != "booking-1" {
			t.Fatalf("data.booking_id = %v, want booking-1", got)
		}

		w.Header().Set("Content-Type", "application/json")
		fmt.Fprint(w, `{"data":[{"status":"ok","id":"ticket-1"}]}`)
	}))
	t.Cleanup(server.Close)

	provider := NewExpoPushProvider(server.Client(), server.URL)
	provider.AccessToken = "test-token"

	if provider.Channel() != ChannelPush {
		t.Fatalf("Channel() = %q, want %q", provider.Channel(), ChannelPush)
	}
	err := provider.Dispatch(context.Background(), Envelope{
		Recipient: " ExpoPushToken[device-1] ",
		Payload: map[string]any{
			"booking_id": "booking-1",
		},
		TemplateData: map[string]any{
			"Subject": "Gate changed",
			"Body":    "Use entrance B",
		},
	})
	if err != nil {
		t.Fatalf("Dispatch: %v", err)
	}
}

func TestExpoPushProviderSendReturnsSuccessTickets(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		fmt.Fprint(w, `{"data":{"status":"ok","id":"ticket-1"}}`)
	}))
	t.Cleanup(server.Close)

	provider := NewExpoPushProvider(server.Client(), server.URL)
	tickets, err := provider.Send(context.Background(), ExpoPushMessage{
		To:    "ExponentPushToken[legacy-device]",
		Title: "Welcome",
		Body:  "Ready",
	})
	if err != nil {
		t.Fatalf("Send: %v", err)
	}
	if len(tickets) != 1 {
		t.Fatalf("tickets = %d, want 1", len(tickets))
	}
	if tickets[0].Status != "ok" || tickets[0].ID != "ticket-1" {
		t.Fatalf("ticket = %#v, want ok ticket-1", tickets[0])
	}
}

func TestExpoPushProviderAPIError(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusBadRequest)
		fmt.Fprint(w, `{"errors":[{"code":"PUSH_TOO_MANY_NOTIFICATIONS","message":"too many notifications"}]}`)
	}))
	t.Cleanup(server.Close)

	provider := NewExpoPushProvider(server.Client(), server.URL)
	_, err := provider.Send(context.Background(), ExpoPushMessage{
		To: "ExpoPushToken[device-1]",
	})
	if !errors.Is(err, ErrExpoPushAPI) {
		t.Fatalf("error = %v, want ErrExpoPushAPI", err)
	}
	if errors.Is(err, ErrExpoPushTokenInvalid) {
		t.Fatalf("error = %v, did not want ErrExpoPushTokenInvalid", err)
	}
	var apiErr *ExpoPushAPIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("error = %T, want *ExpoPushAPIError", err)
	}
	if apiErr.StatusCode != http.StatusBadRequest {
		t.Fatalf("StatusCode = %d, want 400", apiErr.StatusCode)
	}
	if len(apiErr.Errors) != 1 || apiErr.Errors[0].Code != "PUSH_TOO_MANY_NOTIFICATIONS" {
		t.Fatalf("Errors = %#v, want PUSH_TOO_MANY_NOTIFICATIONS", apiErr.Errors)
	}
}

func TestExpoPushProviderInvalidTokenDoesNotCallAPI(t *testing.T) {
	t.Parallel()

	var called atomic.Bool
	server := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		called.Store(true)
	}))
	t.Cleanup(server.Close)

	provider := NewExpoPushProvider(server.Client(), server.URL)
	_, err := provider.Send(context.Background(), ExpoPushMessage{
		To: "not-a-token",
	})
	if !errors.Is(err, ErrExpoPushTokenInvalid) {
		t.Fatalf("error = %v, want ErrExpoPushTokenInvalid", err)
	}
	if called.Load() {
		t.Fatal("provider called Expo API for an invalid token")
	}
}

func TestExpoPushProviderDeviceNotRegisteredIsInvalidToken(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		fmt.Fprint(w, `{"data":[{"status":"error","message":"token is not registered","details":{"error":"DeviceNotRegistered"}}]}`)
	}))
	t.Cleanup(server.Close)

	provider := NewExpoPushProvider(server.Client(), server.URL)
	tickets, err := provider.Send(context.Background(), ExpoPushMessage{
		To: "ExpoPushToken[device-1]",
	})
	if !errors.Is(err, ErrExpoPushAPI) {
		t.Fatalf("error = %v, want ErrExpoPushAPI", err)
	}
	if !errors.Is(err, ErrExpoPushTokenInvalid) {
		t.Fatalf("error = %v, want ErrExpoPushTokenInvalid", err)
	}
	if len(tickets) != 1 || tickets[0].Status != "error" {
		t.Fatalf("tickets = %#v, want returned error ticket", tickets)
	}
}
