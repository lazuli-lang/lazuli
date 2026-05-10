// Hand-written extensions for the customer feature. NOT generated.
//
// Generated code under `customer.gen.go` declares typed contracts
// (Resource, Command, Query, Validator references). This file implements
// the concrete Go for the typed extension points the DSL declares —
// validators, hooks, computed functions, query modifiers, etc.
//
// In the canonical Lazuli layout this file lives alongside the
// `<feature>.gen.go` and is the only place an author writes Go for the
// feature. Generated code never collides with this file (`.gen.go` suffix
// is reserved for codegen output).
package customer

import (
	"strings"

	"lazuli.dev/runtime/lazuli"
)

func init() {
	// Register every extension declared in the DSL `extensions` block.
	// A future cut of the runtime can validate at boot time that every
	// referenced `@validator.<name>` has a matching RegisterValidator call;
	// the spike trusts the author.
	lazuli.RegisterValidator("@validator.email_check", validateCustomerEmail)
}

// validateCustomerEmail rejects email addresses on a curated blocklist.
// Demonstrates the validator pipeline against any command whose input has
// an `Email` field — the runtime calls this with the typed input boxed in
// `any`, and `lazuli.Field[T]` recovers the typed value.
func validateCustomerEmail(_ *lazuli.Ctx, input any) error {
	email, ok := lazuli.Field[string](input, "Email")
	if !ok {
		return &lazuli.Error{
			Status: 400, Code: lazuli.CodeBadRequest,
			Message: "email_check expects an `Email` field on the command input",
		}
	}
	if strings.HasSuffix(strings.ToLower(email), "@blocked.example") {
		return &lazuli.Error{
			Status: 400, Code: lazuli.CodeValidationFailed,
			Message: "email domain is blocked",
		}
	}
	return nil
}
