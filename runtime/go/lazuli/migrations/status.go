package migrations

import (
	"errors"
	"fmt"
	"sort"
)

// MigrationState is the observed state of a migration when comparing the
// generated migration set against the applied ledger.
type MigrationState string

const (
	// MigrationStatePending means a discovered migration has not been recorded
	// in the applied ledger.
	MigrationStatePending MigrationState = "pending"
	// MigrationStateApplied means a discovered migration is present in the
	// applied ledger.
	MigrationStateApplied MigrationState = "applied"
	// MigrationStateUnknown means the ledger contains a migration no longer
	// present in the discovered generated set.
	MigrationStateUnknown MigrationState = "unknown"
)

var (
	// ErrDuplicateMigrationID is returned when the discovered migration set
	// contains the same stable migration ID more than once.
	ErrDuplicateMigrationID = errors.New("migrations: duplicate migration id")
	// ErrMigrationRecordIDRequired is returned when a record has no explicit ID
	// and no migration name from which a stable ID can be derived.
	ErrMigrationRecordIDRequired = errors.New("migrations: migration record id required")
)

// MigrationRecord identifies a migration discovered from generated contracts
// or read from an applied-migrations ledger.
type MigrationRecord struct {
	// ID is the stable migration identifier. When empty, helpers derive it from
	// Feature and Name as "<feature>.<name>", or Name when Feature is empty.
	ID string
	// Feature is the Lazuli feature that owns the migration.
	Feature string
	// Name is the migration name from the tenant_migration declaration.
	Name string
}

// MigrationStatus is one row in the status comparison between discovered
// migrations and applied ledger records.
type MigrationStatus struct {
	MigrationRecord
	// State is pending, applied, or unknown for this migration record.
	State MigrationState
}

// MigrationRecordFromContract converts a generated tenant migration contract
// into the record shape used by status helpers.
func MigrationRecordFromContract(contract TenantMigrationContract) MigrationRecord {
	var id string
	if contract.Name != "" {
		id = migrationID(contract.Feature, contract.Name)
	}
	return MigrationRecord{
		ID:      id,
		Feature: contract.Feature,
		Name:    contract.Name,
	}
}

// MigrationRecordsFromContracts converts generated tenant migration contracts
// into records suitable for MigrationStatuses.
func MigrationRecordsFromContracts(contracts []TenantMigrationContract) []MigrationRecord {
	records := make([]MigrationRecord, len(contracts))
	for i, contract := range contracts {
		records[i] = MigrationRecordFromContract(contract)
	}
	return records
}

// MigrationStatuses compares discovered migrations against applied ledger
// records. The returned slice is deterministic: discovered migrations are
// marked applied or pending, ledger-only records are marked unknown, and all
// rows are sorted by migration ID then name.
func MigrationStatuses(discovered, applied []MigrationRecord) ([]MigrationStatus, error) {
	discoveredByID := make(map[string]MigrationRecord, len(discovered))
	statusesByID := make(map[string]MigrationStatus, len(discovered)+len(applied))

	for i, record := range discovered {
		id, err := record.StableID()
		if err != nil {
			return nil, fmt.Errorf("migrations: discovered migration %d: %w", i, err)
		}
		if _, exists := discoveredByID[id]; exists {
			return nil, fmt.Errorf("%w %q", ErrDuplicateMigrationID, id)
		}
		record.ID = id
		discoveredByID[id] = record
		statusesByID[id] = MigrationStatus{
			MigrationRecord: record,
			State:           MigrationStatePending,
		}
	}

	for i, record := range applied {
		id, err := record.StableID()
		if err != nil {
			return nil, fmt.Errorf("migrations: applied migration %d: %w", i, err)
		}
		if discoveredRecord, ok := discoveredByID[id]; ok {
			discoveredRecord.ID = id
			statusesByID[id] = MigrationStatus{
				MigrationRecord: discoveredRecord,
				State:           MigrationStateApplied,
			}
			continue
		}

		if _, exists := statusesByID[id]; exists {
			continue
		}
		record.ID = id
		statusesByID[id] = MigrationStatus{
			MigrationRecord: record,
			State:           MigrationStateUnknown,
		}
	}

	statuses := make([]MigrationStatus, 0, len(statusesByID))
	for _, status := range statusesByID {
		statuses = append(statuses, status)
	}
	sort.Slice(statuses, func(i, j int) bool {
		if statuses[i].ID != statuses[j].ID {
			return statuses[i].ID < statuses[j].ID
		}
		return statuses[i].Name < statuses[j].Name
	})
	return statuses, nil
}

// StableID returns the explicit stable ID, or derives one from Name with an
// optional Feature prefix.
func (r MigrationRecord) StableID() (string, error) {
	if r.ID != "" {
		return r.ID, nil
	}
	if r.Name == "" {
		return "", ErrMigrationRecordIDRequired
	}
	return migrationID(r.Feature, r.Name), nil
}

func migrationID(feature, name string) string {
	if feature == "" {
		return name
	}
	return feature + "." + name
}
