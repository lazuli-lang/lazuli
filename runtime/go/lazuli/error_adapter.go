package lazuli

import "fmt"

// AdapterError signals failure in an external adapter call (HTTP, gRPC, DB driver, etc.).
// EXPERIMENTAL: subject to change before 1.0.
type AdapterError struct {
	Base                ErrorBase
	Adapter             string // adapter name, e.g. "@plugin/example/payment-gateway"
	Op                  string // adapter-level operation, e.g. "create_preference"
	RetryBudgetConsumed int    // attempts made before this error
	RetryBudgetMax      int    // policy-defined retry ceiling
}

// Error implements the error interface.
func (e *AdapterError) Error() string {
	return fmt.Sprintf("adapter_error: %s.%s (retries=%d/%d)", e.Adapter, e.Op, e.RetryBudgetConsumed, e.RetryBudgetMax)
}

// Unwrap returns the underlying cause.
func (e *AdapterError) Unwrap() error { return e.Base.Cause }

// ErrorBase returns the shared typed error context.
func (e *AdapterError) ErrorBase() ErrorBase { return e.Base }
