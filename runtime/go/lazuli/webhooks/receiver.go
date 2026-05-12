package webhooks

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"
)

const (
	defaultReceiverMaxBodyBytes   int64 = 1 << 20
	defaultReceiverReplayHeader         = "X-Lazuli-Webhook-Timestamp"
	defaultReceiverIdempotencyTTL       = 24 * time.Hour
	compoundReceiverKeySeparator        = "\x1f"
)

var (
	errWebhookBodyTooLarge          = errors.New("webhooks: body too large")
	errWebhookBodyReadFailed        = errors.New("webhooks: read body failed")
	errWebhookHandlerNil            = errors.New("webhooks: handler is nil")
	errWebhookIdempotencyUnresolved = errors.New("webhooks: idempotency key unresolved")
	errWebhookJSONInvalid           = errors.New("webhooks: invalid json payload")
	errWebhookMethodNotAllowed      = errors.New("webhooks: method not allowed")
	errWebhookResponseInvalid       = errors.New("webhooks: response is not json encodable")
	errWebhookSecretUnresolved      = errors.New("webhooks: hmac secret unresolved")
)

// ReceiverOptions configures HandleWithOptions.
type ReceiverOptions struct {
	// MaxBodyBytes caps the inbound request body. Zero uses the receiver
	// default; negative values disable the limit.
	MaxBodyBytes int64
	// SecretLookup resolves the HMAC secret for contract.Verify. Nil reads the
	// environment variable named by contract.Verify.SecretEnv.
	SecretLookup func(context.Context, WebhookContract) ([]byte, error)
	// IdempotencyStore claims contract.IdempotencyBy before the handler runs.
	// Nil disables runtime dedupe.
	IdempotencyStore IdempotencyStore
	// IdempotencyTTL controls how long idempotency claims are retained. Zero
	// uses the receiver default.
	IdempotencyTTL time.Duration
	// ReplayTimestampHeader names the header parsed by ParseWebhookTimestamp
	// for contract.Replay checks. Empty uses X-Lazuli-Webhook-Timestamp.
	ReplayTimestampHeader string
	// ReplayCheck overrides the default replay check. Adapters can use this to
	// read provider-specific timestamps from headers or payload fields.
	ReplayCheck func(context.Context, WebhookContract, *http.Request, Envelope) error
	// Now supplies the current time for replay checks. Nil uses time.Now.
	Now func() time.Time
}

// HandleWithOptions receives one webhook request, verifies it, builds an
// Envelope, and dispatches handler. It writes JSON responses for all errors.
func HandleWithOptions(
	w http.ResponseWriter,
	r *http.Request,
	contract WebhookContract,
	handler HandlerFunc,
	opts ReceiverOptions,
) {
	if r == nil {
		writeReceiverError(w, errWebhookBodyReadFailed)
		return
	}
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", http.MethodPost)
		writeReceiverError(w, errWebhookMethodNotAllowed)
		return
	}
	if handler == nil {
		writeReceiverError(w, errWebhookHandlerNil)
		return
	}

	opts = normalizeReceiverOptions(opts)

	body, err := readReceiverBody(w, r, opts.MaxBodyBytes)
	if err != nil {
		writeReceiverError(w, err)
		return
	}

	secret, err := opts.SecretLookup(r.Context(), contract)
	if err != nil {
		writeReceiverError(w, fmt.Errorf("%w: %s", errWebhookSecretUnresolved, err))
		return
	}
	if err := VerifyHmacSignature(contract.Verify, secret, body, r.Header.Get(contract.Verify.Header)); err != nil {
		writeReceiverError(w, err)
		return
	}

	payload, err := decodeReceiverPayload(body)
	if err != nil {
		writeReceiverError(w, err)
		return
	}

	tenant, err := resolveReceiverTenant(contract, payload)
	if err != nil {
		writeReceiverError(w, err)
		return
	}
	idempotencyKey, err := resolveReceiverIdempotencyKey(contract, payload)
	if err != nil {
		writeReceiverError(w, err)
		return
	}

	envelope := Envelope{
		ID:            idempotencyKey,
		Tenant:        tenant,
		Header:        receiverHeaderMap(r.Header),
		Body:          body,
		ParsedPayload: payload,
	}

	if err := runReceiverReplayCheck(r.Context(), contract, r, envelope, opts); err != nil {
		writeReceiverError(w, err)
		return
	}

	if opts.IdempotencyStore != nil && idempotencyKey != "" {
		if err := opts.IdempotencyStore.Claim(r.Context(), idempotencyKey, opts.IdempotencyTTL); err != nil {
			writeReceiverError(w, err)
			return
		}
	}

	result, err := handler(r.Context(), envelope)
	if err != nil {
		writeReceiverError(w, err)
		return
	}
	writeReceiverResult(w, result)
}

func normalizeReceiverOptions(opts ReceiverOptions) ReceiverOptions {
	if opts.MaxBodyBytes == 0 {
		opts.MaxBodyBytes = defaultReceiverMaxBodyBytes
	}
	if opts.SecretLookup == nil {
		opts.SecretLookup = defaultReceiverSecretLookup
	}
	if opts.IdempotencyTTL <= 0 {
		opts.IdempotencyTTL = defaultReceiverIdempotencyTTL
	}
	if opts.Now == nil {
		opts.Now = time.Now
	}
	return opts
}

func defaultReceiverSecretLookup(_ context.Context, contract WebhookContract) ([]byte, error) {
	name := strings.TrimSpace(contract.Verify.SecretEnv)
	if name == "" {
		return nil, errors.New("secret env is empty")
	}
	secret, ok := os.LookupEnv(name)
	if !ok || secret == "" {
		return nil, fmt.Errorf("env %s is empty", name)
	}
	return []byte(secret), nil
}

func readReceiverBody(w http.ResponseWriter, r *http.Request, maxBytes int64) ([]byte, error) {
	if r.Body == nil {
		return nil, errWebhookJSONInvalid
	}
	defer r.Body.Close()

	if maxBytes > 0 && r.ContentLength > maxBytes {
		return nil, errWebhookBodyTooLarge
	}

	var reader io.Reader = r.Body
	if maxBytes > 0 {
		reader = http.MaxBytesReader(w, r.Body, maxBytes)
	}
	body, err := io.ReadAll(reader)
	if err != nil {
		var maxBytesErr *http.MaxBytesError
		if errors.As(err, &maxBytesErr) {
			return nil, errWebhookBodyTooLarge
		}
		return nil, fmt.Errorf("%w: %v", errWebhookBodyReadFailed, err)
	}
	return body, nil
}

func decodeReceiverPayload(body []byte) (map[string]any, error) {
	var payload map[string]any
	if err := json.Unmarshal(body, &payload); err != nil {
		return nil, fmt.Errorf("%w: %v", errWebhookJSONInvalid, err)
	}
	if payload == nil {
		return nil, errWebhookJSONInvalid
	}
	return payload, nil
}

func resolveReceiverTenant(contract WebhookContract, payload map[string]any) (string, error) {
	if contract.TenantFrom == nil || contract.TenantFrom.Path == "" {
		return "", nil
	}
	return resolveReceiverPathString(payload, contract.TenantFrom.Path, ErrWebhookTenantUnscoped, contract)
}

func resolveReceiverIdempotencyKey(contract WebhookContract, payload map[string]any) (string, error) {
	paths := splitReceiverPaths(contract.IdempotencyBy)
	if len(paths) == 0 {
		return "", nil
	}

	values := make([]string, 0, len(paths))
	for _, path := range paths {
		value, err := resolveReceiverPathString(payload, path, errWebhookIdempotencyUnresolved, contract)
		if err != nil {
			return "", err
		}
		values = append(values, value)
	}
	if len(values) == 1 {
		return values[0], nil
	}
	return strings.Join(values, compoundReceiverKeySeparator), nil
}

func splitReceiverPaths(raw string) []string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil
	}
	fields := strings.Split(raw, ",")
	paths := make([]string, 0, len(fields))
	for _, field := range fields {
		path := strings.TrimSpace(field)
		if path != "" {
			paths = append(paths, path)
		}
	}
	return paths
}

func resolveReceiverPathString(
	payload map[string]any,
	path string,
	sentinel error,
	contract WebhookContract,
) (string, error) {
	value, ok := resolveReceiverPayloadPath(payload, path)
	if !ok {
		return "", receiverPathError(sentinel, contract, path)
	}
	resolved, ok := stringifyReceiverPathValue(value)
	if !ok || resolved == "" {
		return "", receiverPathError(sentinel, contract, path)
	}
	return resolved, nil
}

func resolveReceiverPayloadPath(payload map[string]any, path string) (any, bool) {
	path = strings.TrimSpace(path)
	if path == "" {
		return nil, false
	}

	parts := strings.Split(path, ".")
	if len(parts) > 0 && parts[0] == "payload" {
		parts = parts[1:]
	}
	if len(parts) == 0 {
		return nil, false
	}

	var current any = payload
	for _, part := range parts {
		if part == "" {
			return nil, false
		}
		switch node := current.(type) {
		case map[string]any:
			next, ok := node[part]
			if !ok {
				return nil, false
			}
			current = next
		case map[string]string:
			next, ok := node[part]
			if !ok {
				return nil, false
			}
			current = next
		default:
			return nil, false
		}
	}

	return current, true
}

func stringifyReceiverPathValue(value any) (string, bool) {
	switch v := value.(type) {
	case string:
		return v, true
	case []byte:
		return string(v), true
	case fmt.Stringer:
		return v.String(), true
	case bool:
		return strconv.FormatBool(v), true
	case int:
		return strconv.FormatInt(int64(v), 10), true
	case int8:
		return strconv.FormatInt(int64(v), 10), true
	case int16:
		return strconv.FormatInt(int64(v), 10), true
	case int32:
		return strconv.FormatInt(int64(v), 10), true
	case int64:
		return strconv.FormatInt(v, 10), true
	case uint:
		return strconv.FormatUint(uint64(v), 10), true
	case uint8:
		return strconv.FormatUint(uint64(v), 10), true
	case uint16:
		return strconv.FormatUint(uint64(v), 10), true
	case uint32:
		return strconv.FormatUint(uint64(v), 10), true
	case uint64:
		return strconv.FormatUint(v, 10), true
	case float32:
		return strconv.FormatFloat(float64(v), 'f', -1, 32), true
	case float64:
		return strconv.FormatFloat(v, 'f', -1, 64), true
	default:
		return "", false
	}
}

func receiverPathError(sentinel error, contract WebhookContract, path string) error {
	return fmt.Errorf("%w: %s path %q", sentinel, receiverWebhookName(contract), path)
}

func runReceiverReplayCheck(
	ctx context.Context,
	contract WebhookContract,
	r *http.Request,
	envelope Envelope,
	opts ReceiverOptions,
) error {
	if opts.ReplayCheck != nil {
		return opts.ReplayCheck(ctx, contract, r, envelope)
	}
	if contract.Replay == nil {
		return nil
	}
	if contract.Replay.Mode != ReplayAllow {
		return CheckReplay(opts.Now(), contract.Replay, time.Time{})
	}

	header := opts.ReplayTimestampHeader
	if header == "" {
		header = defaultReceiverReplayHeader
	}
	deliveredAt, err := ParseWebhookTimestamp(r.Header.Get(header))
	if err != nil {
		return err
	}
	return CheckReplay(opts.Now(), contract.Replay, deliveredAt)
}

func receiverHeaderMap(header http.Header) map[string]string {
	if len(header) == 0 {
		return nil
	}
	out := make(map[string]string, len(header))
	for name, values := range header {
		if len(values) == 0 {
			out[name] = ""
			continue
		}
		out[name] = values[0]
	}
	return out
}

func writeReceiverResult(w http.ResponseWriter, result any) {
	if result == nil {
		w.WriteHeader(http.StatusNoContent)
		return
	}

	data, err := json.Marshal(result)
	if err != nil {
		writeReceiverError(w, fmt.Errorf("%w: %v", errWebhookResponseInvalid, err))
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(append(data, '\n'))
}

type receiverErrorResponse struct {
	Error string `json:"error"`
	Code  string `json:"code"`
}

func writeReceiverError(w http.ResponseWriter, err error) {
	status := receiverStatusCode(err)
	message := err.Error()
	if status >= http.StatusInternalServerError {
		message = http.StatusText(status)
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(receiverErrorResponse{
		Error: message,
		Code:  receiverErrorCode(err),
	})
}

func receiverStatusCode(err error) int {
	switch {
	case errors.Is(err, errWebhookMethodNotAllowed):
		return http.StatusMethodNotAllowed
	case errors.Is(err, errWebhookBodyTooLarge):
		return http.StatusRequestEntityTooLarge
	case errors.Is(err, ErrWebhookHmacInvalid):
		return http.StatusUnauthorized
	case errors.Is(err, ErrWebhookIdempotent),
		errors.Is(err, ErrWebhookReplayDenied),
		errors.Is(err, ErrWebhookReplayWindowExpired):
		return http.StatusConflict
	case errors.Is(err, errWebhookBodyReadFailed),
		errors.Is(err, errWebhookIdempotencyUnresolved),
		errors.Is(err, errWebhookJSONInvalid),
		errors.Is(err, ErrWebhookReplayModeInvalid),
		errors.Is(err, ErrWebhookReplayTimestampInvalid),
		errors.Is(err, ErrWebhookReplayWindowInvalid),
		errors.Is(err, ErrWebhookTenantUnscoped):
		return http.StatusBadRequest
	case errors.Is(err, context.DeadlineExceeded):
		return http.StatusGatewayTimeout
	default:
		return http.StatusInternalServerError
	}
}

func receiverErrorCode(err error) string {
	switch {
	case errors.Is(err, errWebhookMethodNotAllowed):
		return "method_not_allowed"
	case errors.Is(err, errWebhookBodyTooLarge):
		return "body_too_large"
	case errors.Is(err, ErrWebhookHmacInvalid):
		return "hmac_invalid"
	case errors.Is(err, ErrWebhookIdempotent):
		return "duplicate_envelope"
	case errors.Is(err, ErrWebhookReplayDenied):
		return "replay_denied"
	case errors.Is(err, ErrWebhookReplayWindowExpired):
		return "replay_window_expired"
	case errors.Is(err, ErrWebhookReplayTimestampInvalid):
		return "replay_timestamp_invalid"
	case errors.Is(err, ErrWebhookReplayWindowInvalid):
		return "replay_window_invalid"
	case errors.Is(err, ErrWebhookReplayModeInvalid):
		return "replay_mode_invalid"
	case errors.Is(err, ErrWebhookTenantUnscoped):
		return "tenant_unscoped"
	case errors.Is(err, errWebhookIdempotencyUnresolved):
		return "idempotency_unresolved"
	case errors.Is(err, errWebhookJSONInvalid):
		return "invalid_json"
	case errors.Is(err, errWebhookBodyReadFailed):
		return "bad_request"
	case errors.Is(err, context.DeadlineExceeded):
		return "timeout"
	default:
		return "internal"
	}
}

func receiverWebhookName(contract WebhookContract) string {
	switch {
	case contract.Feature != "" && contract.Name != "":
		return contract.Feature + "." + contract.Name
	case contract.Feature != "":
		return contract.Feature
	case contract.Name != "":
		return contract.Name
	default:
		return "<unnamed>"
	}
}
