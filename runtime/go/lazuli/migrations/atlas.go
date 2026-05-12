package migrations

import (
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"fmt"
	"io/fs"
	"path"
	"sort"
	"strings"
	"unicode"
)

const (
	// AtlasMigrationChecksumPrefix prefixes Atlas-style SHA-256 checksums.
	AtlasMigrationChecksumPrefix = "h1:"
)

var (
	// ErrInvalidAtlasMigrationFile is returned when a SQL migration filename or
	// manifest entry cannot be interpreted as an Atlas/golang-migrate file.
	ErrInvalidAtlasMigrationFile = errors.New("migrations: invalid atlas migration file")
	// ErrDuplicateAtlasMigrationVersion is returned when multiple entries claim
	// the same migration version for the same apply direction.
	ErrDuplicateAtlasMigrationVersion = errors.New("migrations: duplicate atlas migration version")
	// ErrAtlasMigrationOrderInvalid is returned when migration entries are not
	// listed in deterministic application order.
	ErrAtlasMigrationOrderInvalid = errors.New("migrations: atlas migration order invalid")
)

// AtlasMigrationDirection is the optional golang-migrate direction suffix.
type AtlasMigrationDirection string

const (
	// AtlasMigrationDirectionNone describes Atlas-style <version>_<name>.sql.
	AtlasMigrationDirectionNone AtlasMigrationDirection = ""
	// AtlasMigrationDirectionUp describes golang-migrate .up.sql files.
	AtlasMigrationDirectionUp AtlasMigrationDirection = "up"
	// AtlasMigrationDirectionDown describes golang-migrate .down.sql files.
	AtlasMigrationDirectionDown AtlasMigrationDirection = "down"
)

// AtlasMigrationFile is normalized metadata for one SQL migration file.
type AtlasMigrationFile struct {
	// Path is the slash-separated file path within the fs.FS.
	Path string
	// Version is the leading numeric migration version from the filename.
	Version string
	// Name is the descriptive filename segment after Version.
	Name string
	// Direction is empty for Atlas-style files or up/down for golang-migrate.
	Direction AtlasMigrationDirection
	// Size is the file size in bytes.
	Size int64
	// Checksum is the Atlas-style h1: SHA-256 checksum of the file contents.
	Checksum string
}

// AtlasMigrationManifest is the deterministic manifest representation used by
// Atlas and golang-migrate adapter wiring.
type AtlasMigrationManifest struct {
	Files []AtlasMigrationFile
}

// AtlasMigrationChecksum returns the Atlas-style h1: SHA-256 checksum for data.
func AtlasMigrationChecksum(data []byte) string {
	sum := sha256.Sum256(data)
	return AtlasMigrationChecksumPrefix + base64.StdEncoding.EncodeToString(sum[:])
}

// ParseAtlasMigrationFilePath parses metadata from an Atlas/golang-migrate SQL
// migration path. Supported basenames are <version>_<name>.sql,
// <version>_<name>.up.sql, and <version>_<name>.down.sql.
func ParseAtlasMigrationFilePath(file string) (AtlasMigrationFile, error) {
	clean, ok := cleanAtlasMigrationFilePath(file)
	if !ok {
		return AtlasMigrationFile{}, invalidAtlasMigrationFile("path %q must be a safe SQL migration file path", file)
	}

	base := path.Base(clean)
	if !strings.HasSuffix(base, ".sql") {
		return AtlasMigrationFile{}, invalidAtlasMigrationFile("path %q must end with .sql", file)
	}

	stem := strings.TrimSuffix(base, ".sql")
	direction := AtlasMigrationDirectionNone
	if before, ok := strings.CutSuffix(stem, ".up"); ok {
		stem = before
		direction = AtlasMigrationDirectionUp
	} else if before, ok := strings.CutSuffix(stem, ".down"); ok {
		stem = before
		direction = AtlasMigrationDirectionDown
	}

	version, name, ok := strings.Cut(stem, "_")
	if !ok || version == "" || name == "" {
		return AtlasMigrationFile{}, invalidAtlasMigrationFile("filename %q must match <version>_<name>[.up|.down].sql", base)
	}
	if err := validateAtlasMigrationVersion(version); err != nil {
		return AtlasMigrationFile{}, err
	}
	if err := validateAtlasMigrationName(name); err != nil {
		return AtlasMigrationFile{}, err
	}

	return AtlasMigrationFile{
		Path:      clean,
		Version:   version,
		Name:      name,
		Direction: direction,
	}, nil
}

// NewAtlasMigrationFile returns parsed metadata plus size and checksum for one
// Atlas/golang-migrate SQL migration file.
func NewAtlasMigrationFile(file string, data []byte) (AtlasMigrationFile, error) {
	metadata, err := ParseAtlasMigrationFilePath(file)
	if err != nil {
		return AtlasMigrationFile{}, err
	}
	metadata.Size = int64(len(data))
	metadata.Checksum = AtlasMigrationChecksum(data)
	return metadata, nil
}

// LoadAtlasMigrationManifest discovers .sql files under dir, hashes them, and
// returns a deterministically ordered manifest.
func LoadAtlasMigrationManifest(source fs.FS, dir string) (AtlasMigrationManifest, error) {
	if source == nil {
		return AtlasMigrationManifest{}, errNilMigrationFS
	}
	root, ok := cleanMigrationDir(dir)
	if !ok {
		return AtlasMigrationManifest{}, errInvalidMigrationDir
	}

	var files []AtlasMigrationFile
	err := fs.WalkDir(source, root, func(name string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() || !strings.HasSuffix(path.Base(name), ".sql") {
			return nil
		}

		data, err := fs.ReadFile(source, name)
		if err != nil {
			return fmt.Errorf("read %s: %w", name, err)
		}
		file, err := NewAtlasMigrationFile(name, data)
		if err != nil {
			return err
		}
		files = append(files, file)
		return nil
	})
	if err != nil {
		if errors.Is(err, ErrInvalidAtlasMigrationFile) {
			return AtlasMigrationManifest{}, err
		}
		return AtlasMigrationManifest{}, fmt.Errorf("migrations: discover atlas migrations in %s: %w", displayMigrationDir(root), err)
	}
	return NewAtlasMigrationManifest(files)
}

// BuildAtlasMigrationManifest returns a validated, deterministically ordered
// manifest from already-hashed migration file metadata.
func BuildAtlasMigrationManifest(files []AtlasMigrationFile) (AtlasMigrationManifest, error) {
	return NewAtlasMigrationManifest(files)
}

// NewAtlasMigrationManifest returns a validated, deterministically ordered
// manifest from already-hashed migration file metadata.
func NewAtlasMigrationManifest(files []AtlasMigrationFile) (AtlasMigrationManifest, error) {
	normalized := make([]AtlasMigrationFile, len(files))
	for i, file := range files {
		item, err := normalizeAtlasMigrationFile(file, true)
		if err != nil {
			return AtlasMigrationManifest{}, fmt.Errorf("migrations: atlas migration file %d: %w", i, err)
		}
		normalized[i] = item
	}

	sort.SliceStable(normalized, func(i, j int) bool {
		return atlasMigrationFileLess(normalized[i], normalized[j])
	})
	if err := validateAtlasMigrationDuplicateVersions(normalized); err != nil {
		return AtlasMigrationManifest{}, err
	}
	return AtlasMigrationManifest{Files: normalized}, nil
}

// ValidateAtlasMigrationOrder checks that files are already listed in
// deterministic order and that versions do not collide.
func ValidateAtlasMigrationOrder(files []AtlasMigrationFile) error {
	normalized := make([]AtlasMigrationFile, len(files))
	for i, file := range files {
		item, err := normalizeAtlasMigrationFile(file, false)
		if err != nil {
			return fmt.Errorf("migrations: atlas migration file %d: %w", i, err)
		}
		if i > 0 && atlasMigrationFileLess(item, normalized[i-1]) {
			return fmt.Errorf("%w: %q must sort before %q", ErrAtlasMigrationOrderInvalid, item.Path, normalized[i-1].Path)
		}
		normalized[i] = item
	}
	return validateAtlasMigrationDuplicateVersions(normalized)
}

// Render returns a deterministic Atlas-style manifest text. The first line is
// the checksum of the following path/checksum lines.
func (m AtlasMigrationManifest) Render() (string, error) {
	return RenderAtlasMigrationManifest(m)
}

// RenderAtlasMigrationManifest returns a deterministic Atlas-style manifest
// text. Rendering sorts and validates a copy, so equivalent inputs produce the
// same output.
func RenderAtlasMigrationManifest(manifest AtlasMigrationManifest) (string, error) {
	normalized, err := NewAtlasMigrationManifest(manifest.Files)
	if err != nil {
		return "", err
	}

	body := renderAtlasMigrationManifestBody(normalized.Files)
	return AtlasMigrationChecksum([]byte(body)) + "\n" + body, nil
}

func renderAtlasMigrationManifestBody(files []AtlasMigrationFile) string {
	var b strings.Builder
	for _, file := range files {
		b.WriteString(file.Path)
		b.WriteByte(' ')
		b.WriteString(file.Checksum)
		b.WriteByte('\n')
	}
	return b.String()
}

func normalizeAtlasMigrationFile(file AtlasMigrationFile, requireChecksum bool) (AtlasMigrationFile, error) {
	metadata, err := ParseAtlasMigrationFilePath(file.Path)
	if err != nil {
		return AtlasMigrationFile{}, err
	}
	if file.Version != "" && file.Version != metadata.Version {
		return AtlasMigrationFile{}, invalidAtlasMigrationFile("version %q does not match path version %q", file.Version, metadata.Version)
	}
	if file.Name != "" && file.Name != metadata.Name {
		return AtlasMigrationFile{}, invalidAtlasMigrationFile("name %q does not match path name %q", file.Name, metadata.Name)
	}
	if file.Direction != "" && file.Direction != metadata.Direction {
		return AtlasMigrationFile{}, invalidAtlasMigrationFile("direction %q does not match path direction %q", file.Direction, metadata.Direction)
	}
	if file.Size < 0 {
		return AtlasMigrationFile{}, invalidAtlasMigrationFile("size must be non-negative")
	}
	if file.Checksum == "" {
		if requireChecksum {
			return AtlasMigrationFile{}, invalidAtlasMigrationFile("checksum is required")
		}
	} else if err := validateAtlasMigrationChecksum(file.Checksum); err != nil {
		return AtlasMigrationFile{}, err
	}

	metadata.Size = file.Size
	metadata.Checksum = file.Checksum
	return metadata, nil
}

func validateAtlasMigrationDuplicateVersions(files []AtlasMigrationFile) error {
	type seenVersion struct {
		path string
	}

	seen := map[string]seenVersion{}
	for _, file := range files {
		version := atlasMigrationCanonicalVersion(file.Version)
		key := version + "\x00" + string(file.Direction)
		if previous, ok := seen[key]; ok {
			return fmt.Errorf("%w %q in %q and %q", ErrDuplicateAtlasMigrationVersion, file.Version, previous.path, file.Path)
		}
		seen[key] = seenVersion{path: file.Path}

		if file.Direction == AtlasMigrationDirectionNone {
			for _, direction := range []AtlasMigrationDirection{AtlasMigrationDirectionUp, AtlasMigrationDirectionDown} {
				if previous, ok := seen[version+"\x00"+string(direction)]; ok {
					return fmt.Errorf("%w %q in %q and %q", ErrDuplicateAtlasMigrationVersion, file.Version, previous.path, file.Path)
				}
			}
			continue
		}
		if previous, ok := seen[version+"\x00"+string(AtlasMigrationDirectionNone)]; ok {
			return fmt.Errorf("%w %q in %q and %q", ErrDuplicateAtlasMigrationVersion, file.Version, previous.path, file.Path)
		}
	}
	return nil
}

func atlasMigrationFileLess(a, b AtlasMigrationFile) bool {
	if a.Version != b.Version {
		return atlasMigrationVersionLess(a.Version, b.Version)
	}
	if a.Direction != b.Direction {
		return atlasMigrationDirectionRank(a.Direction) < atlasMigrationDirectionRank(b.Direction)
	}
	if a.Path != b.Path {
		return a.Path < b.Path
	}
	return a.Checksum < b.Checksum
}

func atlasMigrationVersionLess(a, b string) bool {
	a = atlasMigrationCanonicalVersion(a)
	b = atlasMigrationCanonicalVersion(b)
	if len(a) != len(b) {
		return len(a) < len(b)
	}
	return a < b
}

func atlasMigrationCanonicalVersion(version string) string {
	trimmed := strings.TrimLeft(version, "0")
	if trimmed == "" {
		return "0"
	}
	return trimmed
}

func atlasMigrationDirectionRank(direction AtlasMigrationDirection) int {
	switch direction {
	case AtlasMigrationDirectionNone:
		return 0
	case AtlasMigrationDirectionUp:
		return 1
	case AtlasMigrationDirectionDown:
		return 2
	default:
		return 3
	}
}

func cleanAtlasMigrationFilePath(file string) (string, bool) {
	clean, ok := cleanMigrationDir(file)
	if !ok || clean == "." {
		return "", false
	}
	for _, r := range clean {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return "", false
		}
	}
	return clean, true
}

func validateAtlasMigrationVersion(version string) error {
	for _, r := range version {
		if r < '0' || r > '9' {
			return invalidAtlasMigrationFile("version %q must be numeric", version)
		}
	}
	return nil
}

func validateAtlasMigrationName(name string) error {
	if name == "." || name == ".." {
		return invalidAtlasMigrationFile("name %q is not allowed", name)
	}
	for _, r := range name {
		switch {
		case r >= 'a' && r <= 'z',
			r >= 'A' && r <= 'Z',
			r >= '0' && r <= '9',
			r == '_',
			r == '-',
			r == '.':
			continue
		case unicode.IsSpace(r) || unicode.IsControl(r):
			return invalidAtlasMigrationFile("name %q must not contain whitespace or control characters", name)
		default:
			return invalidAtlasMigrationFile("name %q contains unsupported character %q", name, r)
		}
	}
	return nil
}

func validateAtlasMigrationChecksum(checksum string) error {
	if !strings.HasPrefix(checksum, AtlasMigrationChecksumPrefix) {
		return invalidAtlasMigrationFile("checksum must use %s", AtlasMigrationChecksumPrefix)
	}
	decoded, err := base64.StdEncoding.DecodeString(strings.TrimPrefix(checksum, AtlasMigrationChecksumPrefix))
	if err != nil {
		return invalidAtlasMigrationFile("checksum is not base64: %v", err)
	}
	if len(decoded) != sha256.Size {
		return invalidAtlasMigrationFile("checksum must encode a SHA-256 digest")
	}
	return nil
}

func invalidAtlasMigrationFile(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidAtlasMigrationFile, fmt.Sprintf(format, args...))
}
