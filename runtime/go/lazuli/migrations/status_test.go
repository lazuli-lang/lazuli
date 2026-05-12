package migrations

import (
	"errors"
	"testing"
)

func TestMigrationStatusesMarksAppliedPendingAndUnknown(t *testing.T) {
	discovered := []MigrationRecord{
		{ID: "202402_add_indexes", Name: "add_indexes"},
		{ID: "202401_init", Name: "init"},
	}
	applied := []MigrationRecord{
		{ID: "legacy_manual_fix", Name: "manual_fix"},
		{ID: "202401_init", Name: "init"},
	}

	statuses, err := MigrationStatuses(discovered, applied)
	if err != nil {
		t.Fatalf("MigrationStatuses returned %v", err)
	}

	want := []MigrationStatus{
		{MigrationRecord: MigrationRecord{ID: "202401_init", Name: "init"}, State: MigrationStateApplied},
		{MigrationRecord: MigrationRecord{ID: "202402_add_indexes", Name: "add_indexes"}, State: MigrationStatePending},
		{MigrationRecord: MigrationRecord{ID: "legacy_manual_fix", Name: "manual_fix"}, State: MigrationStateUnknown},
	}
	if !equalStatuses(statuses, want) {
		t.Fatalf("statuses = %#v, want %#v", statuses, want)
	}
}

func TestMigrationStatusesDerivesContractIDs(t *testing.T) {
	discovered := MigrationRecordsFromContracts([]TenantMigrationContract{
		{Feature: "billing", Name: "seed_invoice_totals"},
	})
	applied := []MigrationRecord{
		{Feature: "billing", Name: "seed_invoice_totals"},
	}

	statuses, err := MigrationStatuses(discovered, applied)
	if err != nil {
		t.Fatalf("MigrationStatuses returned %v", err)
	}
	want := []MigrationStatus{
		{
			MigrationRecord: MigrationRecord{
				ID:      "billing.seed_invoice_totals",
				Feature: "billing",
				Name:    "seed_invoice_totals",
			},
			State: MigrationStateApplied,
		},
	}
	if !equalStatuses(statuses, want) {
		t.Fatalf("statuses = %#v, want %#v", statuses, want)
	}
}

func TestMigrationStatusesRejectsDuplicateDiscoveredIDs(t *testing.T) {
	_, err := MigrationStatuses(
		[]MigrationRecord{
			{Feature: "customer", Name: "backfill_score"},
			{ID: "customer.backfill_score", Name: "duplicate_backfill_score"},
		},
		nil,
	)
	if !errors.Is(err, ErrDuplicateMigrationID) {
		t.Fatalf("expected ErrDuplicateMigrationID, got %v", err)
	}
}

func TestMigrationStatusesRejectsRecordsWithoutID(t *testing.T) {
	_, err := MigrationStatuses([]MigrationRecord{{}}, nil)
	if !errors.Is(err, ErrMigrationRecordIDRequired) {
		t.Fatalf("expected ErrMigrationRecordIDRequired, got %v", err)
	}
}

func equalStatuses(a, b []MigrationStatus) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
