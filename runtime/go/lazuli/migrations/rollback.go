package migrations

import (
	"context"
	"errors"
	"fmt"
	"io/fs"
	"path"
	"sort"
	"strings"
)

var (
	errInvalidRollbackCount = errors.New("migrations: rollback count must not be negative")

	// ErrRollbackMigrationNotFound is returned when a selected forward
	// migration ID has no matching .down.sql rollback file.
	ErrRollbackMigrationNotFound = errors.New("migrations: rollback migration not found")
)

// RolledBackMigration describes a rollback SQL file successfully handed to an
// Executor.
type RolledBackMigration struct {
	// ID is the forward migration identifier: the slash-separated migration
	// path relative to Dir, without the .sql suffix.
	ID string
	// File is the slash-separated path of the .down.sql rollback file within
	// the fs.FS.
	File string
	// Bytes is the number of bytes read from File.
	Bytes int
}

// RollbackSummary describes the rollback files discovered and executed by
// RollbackSQLMigrations.
type RollbackSummary struct {
	// Discovered is the number of .down.sql rollback files found before
	// filtering by appliedIDs or count.
	Discovered int
	// RolledBack lists rollback migrations executed successfully in execution
	// order.
	RolledBack []RolledBackMigration
}

type rollbackSQLMigration struct {
	ID   string
	File string
}

// RollbackSQLMigrations discovers .down.sql files, maps them to their forward
// migration IDs, and executes selected rollbacks in reverse migration order.
//
// appliedIDs should contain the forward migration IDs already recorded in the
// applied ledger, ordered oldest to newest. When appliedIDs is non-empty, count
// selects the latest count IDs from that ledger; count zero selects all IDs.
// When appliedIDs is empty, count selects that many latest discovered rollback
// files; count zero is a no-op.
func RollbackSQLMigrations(ctx context.Context, source fs.FS, dir string, executor Executor, appliedIDs []string, count int) (RollbackSummary, error) {
	var summary RollbackSummary
	if source == nil {
		return summary, errNilMigrationFS
	}
	if executor == nil {
		return summary, errNilMigrationExecutor
	}
	if count < 0 {
		return summary, errInvalidRollbackCount
	}

	cleanDir, ok := cleanMigrationDir(dir)
	if !ok {
		return summary, errInvalidMigrationDir
	}

	rollbacks, err := discoverSQLRollbackFiles(source, cleanDir)
	if err != nil {
		return summary, err
	}
	summary.Discovered = len(rollbacks)

	selected, err := selectSQLRollbacks(rollbacks, cleanDir, appliedIDs, count)
	if err != nil {
		return summary, err
	}

	for _, rollback := range selected {
		if err := ctx.Err(); err != nil {
			return summary, err
		}

		contents, err := fs.ReadFile(source, rollback.File)
		if err != nil {
			return summary, fmt.Errorf("migrations: read %s: %w", rollback.File, err)
		}
		sql := string(contents)
		if err := executor.Exec(ctx, sql); err != nil {
			return summary, fmt.Errorf("migrations: execute %s: %w", rollback.File, err)
		}

		summary.RolledBack = append(summary.RolledBack, RolledBackMigration{
			ID:    rollback.ID,
			File:  rollback.File,
			Bytes: len(contents),
		})
	}
	return summary, nil
}

func discoverSQLRollbackFiles(source fs.FS, dir string) ([]rollbackSQLMigration, error) {
	var files []rollbackSQLMigration
	err := fs.WalkDir(source, dir, func(name string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() {
			return nil
		}
		if !strings.HasSuffix(path.Base(name), ".down.sql") {
			return nil
		}
		files = append(files, rollbackSQLMigration{
			ID:   rollbackForwardID(dir, name),
			File: name,
		})
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("migrations: discover %s: %w", displayMigrationDir(dir), err)
	}
	sort.Slice(files, func(i, j int) bool {
		return files[i].ID > files[j].ID
	})
	return files, nil
}

func selectSQLRollbacks(rollbacks []rollbackSQLMigration, dir string, appliedIDs []string, count int) ([]rollbackSQLMigration, error) {
	if len(appliedIDs) == 0 {
		if count == 0 {
			return nil, nil
		}
		if count > len(rollbacks) {
			count = len(rollbacks)
		}
		return append([]rollbackSQLMigration(nil), rollbacks[:count]...), nil
	}

	selectedIDs := appliedIDs
	if count > 0 && count < len(selectedIDs) {
		selectedIDs = selectedIDs[len(selectedIDs)-count:]
	}

	selected := make(map[string]struct{}, len(selectedIDs))
	for _, id := range selectedIDs {
		selected[normalizeRollbackSelectionID(dir, id)] = struct{}{}
	}

	result := make([]rollbackSQLMigration, 0, len(selected))
	for _, rollback := range rollbacks {
		if _, ok := selected[rollback.ID]; ok {
			result = append(result, rollback)
			delete(selected, rollback.ID)
		}
	}
	if len(selected) != 0 {
		missing := make([]string, 0, len(selected))
		for id := range selected {
			missing = append(missing, id)
		}
		sort.Strings(missing)
		return nil, fmt.Errorf("%w %q", ErrRollbackMigrationNotFound, missing[0])
	}
	return result, nil
}

func rollbackForwardID(dir, file string) string {
	id := strings.TrimSuffix(file, ".down.sql")
	if dir != "." {
		id = strings.TrimPrefix(id, dir+"/")
	}
	return id
}

func normalizeRollbackSelectionID(dir, id string) string {
	if strings.HasSuffix(id, ".down.sql") {
		id = strings.TrimSuffix(id, ".down.sql")
	} else {
		id = strings.TrimSuffix(id, ".sql")
	}
	if dir != "." {
		id = strings.TrimPrefix(id, dir+"/")
	}
	return id
}
