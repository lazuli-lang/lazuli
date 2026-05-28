package lazuli

import (
	"context"
	"testing"
	"time"
)

// W4 GAP-08 — ScheduleRuleDate resolves the base Date via the registered
// binding fn, parsing string / time.Time results.
func TestScheduleRuleDateResolvesViaBindingFn(t *testing.T) {
	RegisterBindingFn("test_pick_date", func(_ context.Context, args ...any) (any, error) {
		// Echo a fixed RFC 3339 date regardless of the rule argument.
		return "2026-01-15", nil
	})
	got, err := ScheduleRuleDate("test_pick_date", "some_rule")
	if err != nil {
		t.Fatalf("ScheduleRuleDate: %v", err)
	}
	want, _ := time.Parse("2006-01-02", "2026-01-15")
	if !got.Equal(want) {
		t.Errorf("ScheduleRuleDate = %v, want %v", got, want)
	}
}

func TestScheduleRuleDateMissingFnErrors(t *testing.T) {
	if _, err := ScheduleRuleDate("no_such_fn_registered", "rule"); err == nil {
		t.Error("expected error for unregistered binding fn")
	}
}

// W4 GAP-06 — Approvers() returns the chain when populated, else the single
// `By` approver for the back-compat single-approver shape.
func TestApprovalSpecApprovers(t *testing.T) {
	single := ApprovalSpec{By: "@role.admin"}
	if got := single.Approvers(); len(got) != 1 || got[0] != "@role.admin" {
		t.Errorf("single approver fallback = %v", got)
	}
	chained := ApprovalSpec{
		By:    "@role.manager",
		Chain: []string{"@role.manager", "@role.admin"},
	}
	if got := chained.Approvers(); len(got) != 2 || got[1] != "@role.admin" {
		t.Errorf("chain approvers = %v", got)
	}
}
