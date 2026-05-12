package notifications

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
)

// ChannelPush is the mobile push notification channel.
const ChannelPush Channel = "push"

const (
	// DefaultExpoPushBaseURL is the origin for Expo Push Service requests.
	DefaultExpoPushBaseURL = "https://exp.host"

	expoPushSendPath = "/--/api/v2/push/send"
)

// Expo Push Service error sentinels.
var (
	ErrExpoPushAPI          = errors.New("notifications: expo push api error")
	ErrExpoPushTokenInvalid = errors.New("notifications: invalid expo push token")
)

// ExpoPushProvider sends mobile push notifications through Expo Push Service.
// Client and BaseURL are injectable so tests and local runtimes never need live
// network calls.
type ExpoPushProvider struct {
	Client *http.Client
	// BaseURL defaults to https://exp.host. Tests can point it at an httptest
	// server; the provider appends Expo's push send path.
	BaseURL string
	// AccessToken is optional and is sent as a Bearer token when configured.
	AccessToken string
}

var _ ChannelDispatcher = (*ExpoPushProvider)(nil)

// NewExpoPushProvider returns an Expo provider using client and baseURL.
func NewExpoPushProvider(client *http.Client, baseURL string) *ExpoPushProvider {
	return &ExpoPushProvider{
		Client:  client,
		BaseURL: baseURL,
	}
}

// Channel implements ChannelDispatcher.
func (p *ExpoPushProvider) Channel() Channel {
	return ChannelPush
}

// Dispatch implements ChannelDispatcher by translating an Envelope into an
// Expo message. TemplateData keys named subject/title and body/message populate
// the visible notification; Payload is delivered as Expo data.
func (p *ExpoPushProvider) Dispatch(ctx context.Context, env Envelope) error {
	message := ExpoPushMessage{
		To:    env.Recipient,
		Title: firstNotificationString(env.TemplateData, "title", "Title", "subject", "Subject"),
		Body:  firstNotificationString(env.TemplateData, "body", "Body", "message", "Message"),
		Data:  cloneNotificationPayload(env.Payload),
	}
	if message.Title == "" {
		message.Title = firstNotificationString(env.Payload, "title", "Title", "subject", "Subject")
	}
	if message.Body == "" {
		message.Body = firstNotificationString(env.Payload, "body", "Body", "message", "Message")
	}

	_, err := p.Send(ctx, message)
	return err
}

// ExpoPushMessage is the Expo Push Service message shape used by Hostpoint
// mobile notifications. Only `to` is required by Expo.
type ExpoPushMessage struct {
	To                string         `json:"to"`
	Title             string         `json:"title,omitempty"`
	Body              string         `json:"body,omitempty"`
	Data              map[string]any `json:"data,omitempty"`
	Sound             string         `json:"sound,omitempty"`
	Badge             *int           `json:"badge,omitempty"`
	TTL               *int           `json:"ttl,omitempty"`
	Expiration        *int64         `json:"expiration,omitempty"`
	Priority          string         `json:"priority,omitempty"`
	Subtitle          string         `json:"subtitle,omitempty"`
	ChannelID         string         `json:"channelId,omitempty"`
	CategoryID        string         `json:"categoryId,omitempty"`
	InterruptionLevel string         `json:"interruptionLevel,omitempty"`
}

// ExpoPushTicket is one ticket returned by Expo for a submitted message.
type ExpoPushTicket struct {
	Status  string         `json:"status"`
	ID      string         `json:"id,omitempty"`
	Message string         `json:"message,omitempty"`
	Details map[string]any `json:"details,omitempty"`
}

// ExpoPushError is one top-level request error returned by Expo.
type ExpoPushError struct {
	Code    string         `json:"code,omitempty"`
	Message string         `json:"message,omitempty"`
	Details map[string]any `json:"details,omitempty"`
}

// ExpoPushAPIError wraps Expo HTTP, request, and ticket-level errors while
// preserving typed matching with ErrExpoPushAPI and ErrExpoPushTokenInvalid.
type ExpoPushAPIError struct {
	StatusCode int
	Errors     []ExpoPushError
	Tickets    []ExpoPushTicket
	Body       string
}

// Error implements error.
func (e *ExpoPushAPIError) Error() string {
	if e == nil {
		return "<nil>"
	}

	parts := make([]string, 0, 3)
	if e.StatusCode != 0 {
		parts = append(parts, fmt.Sprintf("status %d", e.StatusCode))
	}
	if len(e.Errors) > 0 {
		parts = append(parts, formatExpoPushError(e.Errors[0]))
	}
	if len(e.Tickets) > 0 {
		parts = append(parts, formatExpoPushTicket(e.Tickets[0]))
	}
	if len(parts) == 0 && e.Body != "" {
		parts = append(parts, e.Body)
	}
	if len(parts) == 0 {
		return ErrExpoPushAPI.Error()
	}
	return ErrExpoPushAPI.Error() + ": " + strings.Join(parts, ": ")
}

// Is supports errors.Is for Expo push sentinels.
func (e *ExpoPushAPIError) Is(target error) bool {
	if target == ErrExpoPushAPI {
		return true
	}
	if target == ErrExpoPushTokenInvalid {
		return e.hasInvalidToken()
	}
	return false
}

// Send submits one message to Expo Push Service and returns Expo's tickets.
func (p *ExpoPushProvider) Send(ctx context.Context, message ExpoPushMessage) ([]ExpoPushTicket, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	message.To = strings.TrimSpace(message.To)
	if !IsExpoPushToken(message.To) {
		return nil, fmt.Errorf("%w: %q", ErrExpoPushTokenInvalid, message.To)
	}

	body, err := json.Marshal(message)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, p.sendURL(), bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/json")
	req.Header.Set("Content-Type", "application/json")
	if p != nil && strings.TrimSpace(p.AccessToken) != "" {
		req.Header.Set("Authorization", "Bearer "+strings.TrimSpace(p.AccessToken))
	}

	resp, err := p.httpClient().Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	responseBody, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return nil, err
	}

	parsed, parseErr := parseExpoPushResponse(responseBody)
	if resp.StatusCode >= http.StatusBadRequest {
		apiErr := &ExpoPushAPIError{
			StatusCode: resp.StatusCode,
			Body:       strings.TrimSpace(string(responseBody)),
		}
		if parseErr == nil {
			apiErr.Errors = parsed.Errors
			apiErr.Tickets = parsed.Tickets
		}
		return nil, apiErr
	}
	if parseErr != nil {
		return nil, fmt.Errorf("%w: decode response: %v", ErrExpoPushAPI, parseErr)
	}
	if len(parsed.Errors) > 0 {
		return parsed.Tickets, &ExpoPushAPIError{
			StatusCode: resp.StatusCode,
			Errors:     parsed.Errors,
		}
	}
	if failed := failedExpoPushTickets(parsed.Tickets); len(failed) > 0 {
		return parsed.Tickets, &ExpoPushAPIError{
			StatusCode: resp.StatusCode,
			Tickets:    failed,
		}
	}
	return parsed.Tickets, nil
}

// IsExpoPushToken reports whether token has the Expo push token wrapper format.
func IsExpoPushToken(token string) bool {
	token = strings.TrimSpace(token)
	if token == "" || !strings.HasSuffix(token, "]") {
		return false
	}
	return hasExpoPushTokenPrefix(token, "ExpoPushToken[") ||
		hasExpoPushTokenPrefix(token, "ExponentPushToken[")
}

func hasExpoPushTokenPrefix(token string, prefix string) bool {
	if !strings.HasPrefix(token, prefix) {
		return false
	}
	return strings.TrimSpace(token[len(prefix):len(token)-1]) != ""
}

func (p *ExpoPushProvider) httpClient() *http.Client {
	if p != nil && p.Client != nil {
		return p.Client
	}
	return http.DefaultClient
}

func (p *ExpoPushProvider) sendURL() string {
	baseURL := DefaultExpoPushBaseURL
	if p != nil && strings.TrimSpace(p.BaseURL) != "" {
		baseURL = strings.TrimSpace(p.BaseURL)
	}
	return strings.TrimRight(baseURL, "/") + expoPushSendPath
}

type parsedExpoPushResponse struct {
	Tickets []ExpoPushTicket
	Errors  []ExpoPushError
}

type expoPushResponseEnvelope struct {
	Data   json.RawMessage `json:"data"`
	Errors []ExpoPushError `json:"errors"`
}

func parseExpoPushResponse(body []byte) (parsedExpoPushResponse, error) {
	var envelope expoPushResponseEnvelope
	if err := json.Unmarshal(body, &envelope); err != nil {
		return parsedExpoPushResponse{}, err
	}

	tickets, err := parseExpoPushTickets(envelope.Data)
	if err != nil {
		return parsedExpoPushResponse{}, err
	}
	return parsedExpoPushResponse{Tickets: tickets, Errors: envelope.Errors}, nil
}

func parseExpoPushTickets(raw json.RawMessage) ([]ExpoPushTicket, error) {
	raw = bytes.TrimSpace(raw)
	if len(raw) == 0 || bytes.Equal(raw, []byte("null")) {
		return nil, nil
	}

	if raw[0] == '[' {
		var tickets []ExpoPushTicket
		if err := json.Unmarshal(raw, &tickets); err != nil {
			return nil, err
		}
		return tickets, nil
	}

	var ticket ExpoPushTicket
	if err := json.Unmarshal(raw, &ticket); err != nil {
		return nil, err
	}
	return []ExpoPushTicket{ticket}, nil
}

func failedExpoPushTickets(tickets []ExpoPushTicket) []ExpoPushTicket {
	var failed []ExpoPushTicket
	for _, ticket := range tickets {
		if ticket.Status != "" && ticket.Status != "ok" {
			failed = append(failed, ticket)
		}
	}
	return failed
}

func (e *ExpoPushAPIError) hasInvalidToken() bool {
	if e == nil {
		return false
	}
	for _, ticket := range e.Tickets {
		if expoPushDetailError(ticket.Details) == "DeviceNotRegistered" {
			return true
		}
	}
	for _, expoErr := range e.Errors {
		if expoPushDetailError(expoErr.Details) == "DeviceNotRegistered" {
			return true
		}
	}
	return false
}

func expoPushDetailError(details map[string]any) string {
	value, ok := details["error"].(string)
	if !ok {
		return ""
	}
	return value
}

func formatExpoPushError(err ExpoPushError) string {
	switch {
	case err.Code != "" && err.Message != "":
		return err.Code + ": " + err.Message
	case err.Code != "":
		return err.Code
	case err.Message != "":
		return err.Message
	default:
		return "request failed"
	}
}

func formatExpoPushTicket(ticket ExpoPushTicket) string {
	detail := expoPushDetailError(ticket.Details)
	switch {
	case detail != "" && ticket.Message != "":
		return detail + ": " + ticket.Message
	case detail != "":
		return detail
	case ticket.Message != "":
		return ticket.Message
	case ticket.Status != "":
		return ticket.Status
	default:
		return "ticket failed"
	}
}

func firstNotificationString(values map[string]any, keys ...string) string {
	for _, key := range keys {
		value, ok := values[key]
		if !ok {
			continue
		}
		resolved, ok := stringifyPathValue(value)
		if ok {
			return resolved
		}
	}
	return ""
}
