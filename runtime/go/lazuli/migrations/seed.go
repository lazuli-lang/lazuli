package migrations

import (
	"context"
	"fmt"
	"io/fs"
	"path"
	"sort"
	"strings"
)

// AppliedSeed describes a SQL seed file successfully handed to an Executor.
type AppliedSeed struct {
	// File is the slash-separated path of the seed file within the fs.FS.
	File string
	// Bytes is the number of bytes read from File.
	Bytes int
}

// SeedLoader applies SQL seed files from a filesystem.
type SeedLoader struct {
	// FS contains seed files. It is required.
	FS fs.FS
	// Dir is the directory within FS to scan. Empty means the filesystem root.
	Dir string
	// Executor receives each seed file's SQL contents in sorted order. It is
	// required.
	Executor Executor
}

// NewSeedLoader returns a filesystem-backed SQL seed loader.
func NewSeedLoader(source fs.FS, dir string, executor Executor) SeedLoader {
	return SeedLoader{FS: source, Dir: dir, Executor: executor}
}

// LoadSQLSeeds applies SQL seed files from source using executor.
func LoadSQLSeeds(ctx context.Context, source fs.FS, dir string, executor Executor) ([]AppliedSeed, error) {
	return NewSeedLoader(source, dir, executor).Load(ctx)
}

// Load discovers .sql seed files, sorts them by path, and executes their
// contents in order. Non-.sql files are ignored.
//
// When execution fails, Load returns the seeds applied before the failed file
// along with an error wrapping the read or execution failure.
func (l SeedLoader) Load(ctx context.Context) ([]AppliedSeed, error) {
	if l.FS == nil {
		return nil, errNilMigrationFS
	}
	if l.Executor == nil {
		return nil, errNilMigrationExecutor
	}

	dir, ok := cleanMigrationDir(l.Dir)
	if !ok {
		return nil, errInvalidMigrationDir
	}

	if err := ctx.Err(); err != nil {
		return nil, err
	}

	files, err := discoverSQLSeedFiles(ctx, l.FS, dir)
	if err != nil {
		return nil, err
	}

	applied := make([]AppliedSeed, 0, len(files))
	for _, file := range files {
		if err := ctx.Err(); err != nil {
			return applied, err
		}

		contents, err := fs.ReadFile(l.FS, file)
		if err != nil {
			return applied, fmt.Errorf("migrations: read seed %s: %w", file, err)
		}
		if err := ctx.Err(); err != nil {
			return applied, err
		}

		sql := string(contents)
		if err := l.Executor.Exec(ctx, sql); err != nil {
			return applied, fmt.Errorf("migrations: execute seed %s: %w", file, err)
		}

		applied = append(applied, AppliedSeed{
			File:  file,
			Bytes: len(contents),
		})
	}
	return applied, nil
}

func discoverSQLSeedFiles(ctx context.Context, source fs.FS, dir string) ([]string, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	var files []string
	err := fs.WalkDir(source, dir, func(name string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if err := ctx.Err(); err != nil {
			return err
		}
		if entry.IsDir() {
			return nil
		}
		if strings.HasSuffix(path.Base(name), ".sql") {
			files = append(files, name)
		}
		return nil
	})
	if err != nil {
		if ctxErr := ctx.Err(); ctxErr != nil {
			return nil, ctxErr
		}
		return nil, fmt.Errorf("migrations: discover seeds %s: %w", displayMigrationDir(dir), err)
	}
	sort.Strings(files)
	return files, nil
}
