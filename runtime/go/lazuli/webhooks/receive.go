// Package webhooks — chi-mounted receiver. The generated app calls
// `webhooks.Mount(router, contracts, handlers)` once at boot; the
// receiver wires HMAC verify, idempotency dedupe, tenant resolution,
// and handler dispatch.
//
// Phase L Tier 3 / row 33 stubs.
package webhooks

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"os"
	"time"

	"github.com/tidwall/gjson"

	"lazuli.dev/runtime/lazuli/plangate"
)

// EventPublisher is the post-success publish hook the lazuli root
// package installs at init to avoid a `lazuli ↔ webhooks` import
// cycle. Mirrors the `RegisterPreludeRunner` pattern.
type EventPublisher func(ctx context.Context, name string, payload map[string]any, occurredAt time.Time)

var eventPublisher EventPublisher

// RegisterEventPublisher wires the bridge to `lazuli.Publish`. The
// root lazuli package installs this from its `init()` so receivers
// can fire `Emits` without importing the root (avoiding a cycle).
func RegisterEventPublisher(p EventPublisher) {
	eventPublisher = p
}

// preludeRunner is the package-level hook the `billing` package
// installs at init. Same shape as the dispatcher-side runner in
// `lazuli.RunPrelude`, scoped to `WebhookContract.Prelude`.
var preludeRunner func(ctx context.Context, prelude []plangate.GateRef) error

// incrementRunner is the post-success companion to preludeRunner.
var incrementRunner func(ctx context.Context, prelude []plangate.GateRef) error

// idempotencyChecker, when set, is consulted to short-circuit
// duplicate envelopes. Returning `(true, nil)` means the envelope
// already landed and the receiver responds 200 without re-dispatch
// (replay-safe semantics matching Stripe/GitHub). `nil` (default)
// disables dedupe and the receiver delegates to the underlying
// handler / DB UNIQUE constraint.
var idempotencyChecker func(ctx context.Context, feature, name, key string) (seen bool, err error)

// RegisterPreludeRunner is called by `billing.init` to install the
// gate-prelude evaluator. Tests substitute via the same hook.
func RegisterPreludeRunner(run func(ctx context.Context, prelude []plangate.GateRef) error) {
	preludeRunner = run
}

// RegisterIncrementRunner is called by `billing.init` to install
// the quota-counter advancer.
func RegisterIncrementRunner(run func(ctx context.Context, prelude []plangate.GateRef) error) {
	incrementRunner = run
}

// RegisterIdempotencyChecker installs a dedupe hook for inbound
// envelopes. The hook receives the feature + webhook name + the
// resolved idempotency key (per `IdempotencyBy` path or the
// content-addressed sha256 fallback) and returns true if the key has
// already been seen. Adapter packs (`@runtime/postgres-idempotency`,
// `@runtime/redis-idempotency`, etc.) install this at boot.
func RegisterIdempotencyChecker(fn func(ctx context.Context, feature, name, key string) (bool, error)) {
	idempotencyChecker = fn
}

func runWebhookPrelude(ctx context.Context, prelude []plangate.GateRef) error {
	if len(prelude) == 0 || preludeRunner == nil {
		return nil
	}
	return preludeRunner(ctx, prelude)
}

func runWebhookIncrement(ctx context.Context, prelude []plangate.GateRef) error {
	if len(prelude) == 0 || incrementRunner == nil {
		return nil
	}
	return incrementRunner(ctx, prelude)
}

// Router is the minimal chi-like router interface the Lazuli runtime
// relies on. Concrete implementations live in `@runtime/chi`; the
// language stays out of routing mechanics.
type Router interface {
	Method(method, pattern string, handler http.HandlerFunc)
}

// Mount wires every webhook contract onto a Router. Each handler
// reads the request body, runs `VerifyHmacSignature`, decodes the
// envelope, optionally resolves `tenant_from`, dispatches the user
// handler, and emits the declared events on success.
func Mount(
	r Router,
	contracts []WebhookContract,
	handlers []HandlerFunc,
) error {
	if len(contracts) != len(handlers) {
		return errors.New("webhooks: contract/handler slices misaligned")
	}
	for i, contract := range contracts {
		handler := handlers[i]
		r.Method(http.MethodPost, contract.Route, func(w http.ResponseWriter, req *http.Request) {
			handleOne(req.Context(), contract, handler, w, req)
		})
	}
	return nil
}

func handleOne(
	ctx context.Context,
	contract WebhookContract,
	handler HandlerFunc,
	w http.ResponseWriter,
	req *http.Request,
) {
	if contract.WithSource != nil {
		ctx = contract.WithSource(ctx)
	}
	if err := runWebhookPrelude(ctx, contract.Prelude); err != nil {
		w.WriteHeader(http.StatusPaymentRequired)
		return
	}

	body, err := io.ReadAll(req.Body)
	defer req.Body.Close()
	if err != nil {
		slog.Error("webhooks: read body", "feature", contract.Feature,
			"name", contract.Name, "error", err)
		w.WriteHeader(http.StatusBadRequest)
		return
	}

	headerValue := req.Header.Get(contract.Verify.Header)
	secret := []byte(os.Getenv(contract.Verify.SecretEnv))
	if len(secret) == 0 {
		slog.Error("webhooks: secret env empty",
			"feature", contract.Feature, "name", contract.Name,
			"env", contract.Verify.SecretEnv)
		w.WriteHeader(http.StatusInternalServerError)
		return
	}
	if err := VerifyHmacSignature(contract.Verify, secret, body, headerValue); err != nil {
		w.WriteHeader(http.StatusUnauthorized)
		return
	}

	// Replay-window check: when the contract denies replays beyond
	// `Window`, parse the duration and reject envelopes whose body
	// timestamp (extracted via `payload.timestamp`) falls outside.
	// Most pilots use ReplayAllow → this is a no-op.
	if contract.Replay != nil && contract.Replay.Mode == ReplayDeny && contract.Replay.Window != "" {
		if window, perr := time.ParseDuration(contract.Replay.Window); perr == nil {
			tsRaw := gjson.GetBytes(body, "timestamp").String()
			if ts, terr := time.Parse(time.RFC3339, tsRaw); terr == nil {
				if time.Since(ts) > window {
					w.WriteHeader(http.StatusGone)
					return
				}
			}
		}
	}

	var parsed map[string]any
	if len(body) > 0 {
		if err := json.Unmarshal(body, &parsed); err != nil {
			w.WriteHeader(http.StatusBadRequest)
			return
		}
	}

	tenant := ""
	if contract.TenantFrom != nil {
		tenant = gjson.GetBytes(body, contract.TenantFrom.Path).String()
		if tenant == "" {
			slog.Warn("webhooks: tenant_from unresolved",
				"feature", contract.Feature, "name", contract.Name,
				"path", contract.TenantFrom.Path)
			w.WriteHeader(http.StatusUnprocessableEntity)
			return
		}
	}

	envelopeID := resolveIdempotencyKey(body, contract)

	if idempotencyChecker != nil {
		seen, ierr := idempotencyChecker(ctx, contract.Feature, contract.Name, envelopeID)
		if ierr != nil {
			slog.Error("webhooks: idempotency check",
				"feature", contract.Feature, "name", contract.Name, "error", ierr)
			w.WriteHeader(http.StatusInternalServerError)
			return
		}
		if seen {
			// Replay-safe: respond 200 without re-dispatching.
			w.WriteHeader(http.StatusOK)
			return
		}
	}

	envelope := Envelope{
		ID:            envelopeID,
		Tenant:        tenant,
		Header:        flattenHeader(req.Header),
		Body:          body,
		ParsedPayload: parsed,
	}

	if _, herr := handler(ctx, envelope); herr != nil {
		slog.Error("webhooks: handler", "feature", contract.Feature,
			"name", contract.Name, "error", herr)
		w.WriteHeader(http.StatusInternalServerError)
		return
	}

	if err := runWebhookIncrement(ctx, contract.Prelude); err != nil {
		slog.Error("webhooks: increment", "feature", contract.Feature,
			"name", contract.Name, "error", err)
		// Increment failure does not fail the response — the envelope
		// was processed; the counter advance can be reconciled out-of-
		// band. Same posture as the dispatcher emit path.
	}

	if eventPublisher != nil {
		now := time.Now()
		for _, name := range contract.Emits {
			eventPublisher(ctx, name, parsed, now)
		}
	}

	w.WriteHeader(http.StatusOK)
}

// resolveIdempotencyKey returns the contract-declared key resolved
// against the body, falling back to a content-addressed SHA-256 of
// the body when `IdempotencyBy` is empty or unresolved.
func resolveIdempotencyKey(body []byte, contract WebhookContract) string {
	if contract.IdempotencyBy != "" {
		key := gjson.GetBytes(body, contract.IdempotencyBy).String()
		if key != "" {
			return key
		}
	}
	sum := sha256.Sum256(body)
	return hex.EncodeToString(sum[:])
}

func flattenHeader(h http.Header) map[string]string {
	out := make(map[string]string, len(h))
	for k, v := range h {
		if len(v) > 0 {
			out[k] = v[0]
		}
	}
	return out
}
