package lazuli

// Approval is a fluent builder for ApprovalSpec.
//
//	approval := lazuli.Approval(lazuli.ApprovalThenDeny).By("role.admin").WithReason("settle").Spec()
type Approval ApprovalThen

// By records the approver role/scope/actor.
func (a Approval) By(approver string) ApprovalBuilder {
	return ApprovalBuilder{Then: ApprovalThen(a), By: approver}
}

// ApprovalBuilder accumulates the fields for an ApprovalSpec.
type ApprovalBuilder struct {
	Then   ApprovalThen
	By     string
	Reason string
}

// WithReason records the human-readable approval reason.
func (b ApprovalBuilder) WithReason(reason string) ApprovalBuilder {
	b.Reason = reason
	return b
}

// Spec returns the lowered approval contract.
func (b ApprovalBuilder) Spec() *ApprovalSpec {
	return &ApprovalSpec{Then: b.Then, By: b.By, Reason: b.Reason}
}
