// Package i18n — exposure helper.
//
// Codegen lowers each feature's `errors default hide / expose client 4xx ...`
// block into a `FeatureErrorContract`. The HTTP boundary calls
// `ShouldExpose` to decide which envelope fields (`message`, `code`,
// `data`, `message_key`) reach the wire per response.
//
// Proposal §2.G — exposure and message-override compose orthogonally:
// the resolver runs anyway (so logs always carry the human-readable
// string), but the wire payload is filtered by these rules.
package i18n

// ShouldExpose reports whether `field` should appear in the wire
// response. `status` selects the 4xx vs 5xx exposure list; the
// contract's `Default` applies when no contract is registered or no
// matching list covers the status family.
//
// Closed catalog of `field`:
//   - "code", "message", "data", "message_key" — recognised by callers.
//
// Semantics:
//   - 5xx + "message" is force-hidden regardless of contract (proposal §2.G —
//     risks leaking internal state). Validation is at the doctor layer
//     (`ERR-VOCAB-EXPOSE-5XX-MESSAGE`), but the runtime fails closed too.
//   - If the contract is the zero value, default to ExposureHide and
//     allow only `code` (the v1 always-on field).
func ShouldExpose(contract FeatureErrorContract, status int, field string) bool {
	if status >= 500 && field == "message" {
		return false
	}
	// Always-on field — `code` is the stable client-branchable identifier.
	if field == "code" {
		return true
	}
	list := contract.ExposeClient4xx
	if status >= 500 {
		list = contract.ExposeClient5xx
	}
	for _, f := range list {
		if f == field {
			return true
		}
	}
	if contract.Default == ExposureExpose {
		return true
	}
	return false
}
