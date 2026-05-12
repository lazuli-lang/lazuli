package lazuli

import "testing"

func TestApprovalByRecordsThenAndApprover(t *testing.T) {
	builder := Approval(ApprovalThenDeny).By("role.admin")

	if builder.Then != ApprovalThenDeny {
		t.Fatalf("Then = %q, want %q", builder.Then, ApprovalThenDeny)
	}
	if builder.By != "role.admin" {
		t.Fatalf("By = %q, want %q", builder.By, "role.admin")
	}
	if builder.Reason != "" {
		t.Fatalf("Reason = %q, want empty", builder.Reason)
	}
}

func TestApprovalWithReasonReturnsUpdatedBuilder(t *testing.T) {
	builder := Approval(ApprovalThenEscalate).By("scope.finance")
	withReason := builder.WithReason("settle invoice")

	if builder.Reason != "" {
		t.Fatalf("original Reason = %q, want empty", builder.Reason)
	}
	if withReason.Reason != "settle invoice" {
		t.Fatalf("Reason = %q, want %q", withReason.Reason, "settle invoice")
	}
}

func TestApprovalSpecBuildsContract(t *testing.T) {
	spec := Approval(ApprovalThenAllow).By("actor.system").WithReason("maintenance").Spec()

	if spec == nil {
		t.Fatal("Spec returned nil")
	}
	if spec.Then != ApprovalThenAllow {
		t.Fatalf("Then = %q, want %q", spec.Then, ApprovalThenAllow)
	}
	if spec.By != "actor.system" {
		t.Fatalf("By = %q, want %q", spec.By, "actor.system")
	}
	if spec.Reason != "maintenance" {
		t.Fatalf("Reason = %q, want %q", spec.Reason, "maintenance")
	}
}
