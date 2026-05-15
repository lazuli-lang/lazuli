// Package webhooks — chi-mounted receiver. The generated app calls
// `webhooks.Mount(router, contracts, handlers)` once at boot; the
// receiver wires HMAC verify, idempotency dedupe, tenant resolution,
// and handler dispatch.
//
// Phase L Tier 3 / row 33 stubs.
package webhooks

import (
	"context"
	"errors"
	"net/http"

	"lazuli.dev/runtime/lazuli/plangate"
)

// preludeRunner is the package-level hook the `billing` package
// installs at init. Same shape as the dispatcher-side runner in
// `lazuli.RunPrelude`, scoped to `WebhookContract.Prelude`.
var preludeRunner func(ctx context.Context, prelude []plangate.GateRef) error

// incrementRunner is the post-success companion to preludeRunner.
var incrementRunner func(ctx context.Context, prelude []plangate.GateRef) error

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

// Receiver wires every webhook contract onto a Router. Each handler
// reads the request body, runs `VerifyHmacSignature`, decodes the
// envelope, optionally resolves `tenant_from`, dispatches the user
// handler, and emits the declared events on success.
//
// Stub: full implementation lands with the runtime team. Codegen
// depends only on the function signature.
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
	_ = ctx
	_ = contract
	_ = handler
	_ = req
	// TODO(runtime): read body, run VerifyHmacSignature against the
	// declared SecretEnv/Header, parse JSON, dedupe via
	// IdempotencyBy path, resolve TenantFrom, dispatch handler, emit
	// declared events on success, propagate audit + observability
	// span. On success, call `runWebhookIncrement(ctx, contract.Prelude)`
	// so quota-gate counters advance.
	w.WriteHeader(http.StatusNotImplemented)
}
