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
	errNilMigrationFS       = errors.New("migrations: FS is required")
	errNilMigrationExecutor = errors.New("migrations: Executor is required")
	errInvalidMigrationDir  = errors.New("migrations: directory must be a safe fs path")
)

// Executor is the minimal adapter-neutral SQL execution contract used by
// Runner. Implementations should execute the given SQL batch against their
// own connection, transaction, or driver abstraction.
type Executor interface {
	Exec(ctx context.Context, sql string) error
}

// AppliedMigration describes a SQL migration file successfully handed to an
// Executor.
type AppliedMigration struct {
	// File is the slash-separated path of the migration file within the fs.FS.
	File string
	// Bytes is the number of bytes read from File.
	Bytes int
}

// Runner applies forward SQL migrations from a filesystem.
type Runner struct {
	// FS contains migration files. It is required.
	FS fs.FS
	// Dir is the directory within FS to scan. Empty means the filesystem root.
	Dir string
	// Executor receives each migration file's SQL contents in sorted order. It
	// is required.
	Executor Executor
}

// NewRunner returns a filesystem-backed SQL migration runner.
func NewRunner(source fs.FS, dir string, executor Executor) Runner {
	return Runner{FS: source, Dir: dir, Executor: executor}
}

// ApplySQLMigrations applies forward SQL migrations from source using executor.
func ApplySQLMigrations(ctx context.Context, source fs.FS, dir string, executor Executor) ([]AppliedMigration, error) {
	return NewRunner(source, dir, executor).Apply(ctx)
}

// Apply discovers forward .sql files, sorts them by path, and executes their
// contents in order. Files ending in .down.sql are ignored for forward apply.
//
// When execution fails, Apply returns the migrations applied before the failed
// file along with an error wrapping the read or execution failure.
func (r Runner) Apply(ctx context.Context) ([]AppliedMigration, error) {
	if r.FS == nil {
		return nil, errNilMigrationFS
	}
	if r.Executor == nil {
		return nil, errNilMigrationExecutor
	}

	dir, ok := cleanMigrationDir(r.Dir)
	if !ok {
		return nil, errInvalidMigrationDir
	}

	files, err := discoverSQLMigrationFiles(r.FS, dir)
	if err != nil {
		return nil, err
	}

	applied := make([]AppliedMigration, 0, len(files))
	for _, file := range files {
		if err := ctx.Err(); err != nil {
			return applied, err
		}

		contents, err := fs.ReadFile(r.FS, file)
		if err != nil {
			return applied, fmt.Errorf("migrations: read %s: %w", file, err)
		}
		sql := string(contents)
		if err := r.Executor.Exec(ctx, sql); err != nil {
			return applied, fmt.Errorf("migrations: execute %s: %w", file, err)
		}

		applied = append(applied, AppliedMigration{
			File:  file,
			Bytes: len(contents),
		})
	}
	return applied, nil
}

func discoverSQLMigrationFiles(source fs.FS, dir string) ([]string, error) {
	var files []string
	err := fs.WalkDir(source, dir, func(name string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() {
			return nil
		}
		base := path.Base(name)
		if strings.HasSuffix(base, ".sql") && !strings.HasSuffix(base, ".down.sql") {
			files = append(files, name)
		}
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("migrations: discover %s: %w", displayMigrationDir(dir), err)
	}
	sort.Strings(files)
	return files, nil
}

func cleanMigrationDir(dir string) (string, bool) {
	if dir == "" {
		return ".", true
	}
	if strings.ContainsAny(dir, "\x00\\") {
		return "", false
	}
	for _, segment := range strings.Split(dir, "/") {
		if segment == ".." {
			return "", false
		}
	}
	clean := strings.TrimPrefix(path.Clean("/"+dir), "/")
	if clean == "" {
		return ".", true
	}
	if !fs.ValidPath(clean) {
		return "", false
	}
	return clean, true
}

func displayMigrationDir(dir string) string {
	if dir == "." {
		return "root"
	}
	return dir
}
