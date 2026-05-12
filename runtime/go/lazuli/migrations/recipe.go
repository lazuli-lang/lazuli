package migrations

import (
	"errors"
	"fmt"
	"io"
	"io/fs"
	"path"
	"strconv"
	"strings"
	"unicode"
)

const RecipeManifestFile = "recipe.toml"

// ErrInvalidRecipeManifest is returned when a migration recipe manifest or
// its migrations/recipes/<from>-to-<to>/<recipe> directory is invalid.
var ErrInvalidRecipeManifest = errors.New("migrations: invalid recipe manifest")

// RecipeKind names the migration surface a recipe covers.
type RecipeKind string

const (
	// RecipeKindLanguage covers .lzi language or IR breaking changes.
	RecipeKindLanguage RecipeKind = "language"
	// RecipeKindGo covers Go-side runtime or generated-code breaking changes.
	RecipeKindGo RecipeKind = "go"
)

// RecipePath is the validated directory identity for a recipe rooted at
// migrations/recipes/<from>-to-<to>/<recipe>.
type RecipePath struct {
	// Dir is the slash-separated recipe directory within an fs.FS.
	Dir string
	// FromVersion is the source LZIR_SCHEMA minor version.
	FromVersion string
	// ToVersion is the target LZIR_SCHEMA minor version.
	ToVersion string
	// Name is the recipe directory name.
	Name string
}

// RecipeManifest is the parsed recipe.toml metadata. Name and Dir are filled
// by LoadRecipeManifest when the manifest is loaded from a recipe directory;
// ParseRecipeManifest leaves them empty because recipe.toml does not carry
// path identity.
type RecipeManifest struct {
	FromVersion string
	ToVersion   string
	Kind        RecipeKind
	Name        string
	Dir         string
}

// LoadRecipeManifest reads and validates recipe.toml from a recipe directory.
func LoadRecipeManifest(source fs.FS, dir string) (RecipeManifest, error) {
	if source == nil {
		return RecipeManifest{}, invalidRecipeManifest("FS is required")
	}

	recipePath, err := ParseRecipePath(dir)
	if err != nil {
		return RecipeManifest{}, err
	}

	data, err := fs.ReadFile(source, path.Join(recipePath.Dir, RecipeManifestFile))
	if err != nil {
		return RecipeManifest{}, fmt.Errorf("%w: read %s: %w", ErrInvalidRecipeManifest, path.Join(recipePath.Dir, RecipeManifestFile), err)
	}

	manifest, err := ParseRecipeManifest(strings.NewReader(string(data)))
	if err != nil {
		return RecipeManifest{}, err
	}
	manifest.Name = recipePath.Name
	manifest.Dir = recipePath.Dir

	if err := ValidateRecipeManifestPath(recipePath.Dir, manifest); err != nil {
		return RecipeManifest{}, err
	}
	return manifest, nil
}

// ParseRecipeManifest reads recipe.toml metadata. Only the root string keys
// from-version, to-version, and kind are part of the manifest contract.
func ParseRecipeManifest(r io.Reader) (RecipeManifest, error) {
	if r == nil {
		return RecipeManifest{}, invalidRecipeManifest("reader is nil")
	}

	data, err := io.ReadAll(r)
	if err != nil {
		return RecipeManifest{}, invalidRecipeManifest("read: %v", err)
	}

	fields, err := parseRecipeManifestFields(string(data))
	if err != nil {
		return RecipeManifest{}, err
	}

	manifest := RecipeManifest{
		FromVersion: fields["from-version"],
		ToVersion:   fields["to-version"],
		Kind:        RecipeKind(fields["kind"]),
	}
	if err := manifest.Validate(); err != nil {
		return RecipeManifest{}, err
	}
	return manifest, nil
}

// ParseRecipePath validates and parses migrations/recipes/<from>-to-<to>/<recipe>.
func ParseRecipePath(dir string) (RecipePath, error) {
	clean, ok := cleanMigrationDir(dir)
	if !ok || clean == "." {
		return RecipePath{}, invalidRecipeManifest("recipe directory must be a safe migrations/recipes path")
	}

	parts := strings.Split(clean, "/")
	if len(parts) != 4 || parts[0] != "migrations" || parts[1] != "recipes" {
		return RecipePath{}, invalidRecipeManifest("recipe directory %q must match migrations/recipes/<from>-to-<to>/<recipe>", dir)
	}

	fromVersion, toVersion, ok := strings.Cut(parts[2], "-to-")
	if !ok || fromVersion == "" || toVersion == "" {
		return RecipePath{}, invalidRecipeManifest("recipe version window %q must match <from>-to-<to>", parts[2])
	}
	if err := validateRecipeVersion("from-version", fromVersion); err != nil {
		return RecipePath{}, err
	}
	if err := validateRecipeVersion("to-version", toVersion); err != nil {
		return RecipePath{}, err
	}
	if err := validateRecipeVersionOrder(fromVersion, toVersion); err != nil {
		return RecipePath{}, err
	}
	if err := validateRecipeName(parts[3]); err != nil {
		return RecipePath{}, err
	}

	return RecipePath{
		Dir:         clean,
		FromVersion: fromVersion,
		ToVersion:   toVersion,
		Name:        parts[3],
	}, nil
}

// ValidateRecipeManifestPath validates that manifest metadata matches its
// migrations/recipes/<from>-to-<to>/<recipe> directory.
func ValidateRecipeManifestPath(dir string, manifest RecipeManifest) error {
	recipePath, err := ParseRecipePath(dir)
	if err != nil {
		return err
	}
	if err := validateRecipeManifestFields(manifest); err != nil {
		return err
	}
	if manifest.FromVersion != recipePath.FromVersion {
		return invalidRecipeManifest("from-version %q does not match recipe path %q", manifest.FromVersion, recipePath.FromVersion)
	}
	if manifest.ToVersion != recipePath.ToVersion {
		return invalidRecipeManifest("to-version %q does not match recipe path %q", manifest.ToVersion, recipePath.ToVersion)
	}
	if manifest.Name != "" && manifest.Name != recipePath.Name {
		return invalidRecipeManifest("recipe name %q does not match recipe path %q", manifest.Name, recipePath.Name)
	}
	if manifest.Dir != "" && manifest.Dir != recipePath.Dir {
		return invalidRecipeManifest("recipe dir %q does not match recipe path %q", manifest.Dir, recipePath.Dir)
	}
	return nil
}

// Validate checks the recipe.toml metadata fields and optional path identity.
func (m RecipeManifest) Validate() error {
	if err := validateRecipeManifestFields(m); err != nil {
		return err
	}
	if m.Dir != "" {
		return ValidateRecipeManifestPath(m.Dir, m)
	}
	return nil
}

func validateRecipeManifestFields(m RecipeManifest) error {
	if err := validateRecipeVersion("from-version", m.FromVersion); err != nil {
		return err
	}
	if err := validateRecipeVersion("to-version", m.ToVersion); err != nil {
		return err
	}
	if err := validateRecipeVersionOrder(m.FromVersion, m.ToVersion); err != nil {
		return err
	}
	if err := validateRecipeKind(m.Kind); err != nil {
		return err
	}
	if m.Name != "" {
		if err := validateRecipeName(m.Name); err != nil {
			return err
		}
	}
	return nil
}

func parseRecipeManifestFields(data string) (map[string]string, error) {
	fields := map[string]string{}
	for i, line := range strings.Split(data, "\n") {
		lineNo := i + 1
		trimmed := strings.TrimSpace(strings.TrimSuffix(line, "\r"))
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}

		keyPart, valuePart, ok := strings.Cut(trimmed, "=")
		if !ok {
			return nil, invalidRecipeManifest("line %d: expected key = \"value\"", lineNo)
		}
		key := strings.TrimSpace(keyPart)
		if !isRecipeManifestKey(key) {
			return nil, invalidRecipeManifest("line %d: invalid key %q", lineNo, key)
		}
		if key != "from-version" && key != "to-version" && key != "kind" {
			return nil, invalidRecipeManifest("line %d: unknown key %q", lineNo, key)
		}
		if _, exists := fields[key]; exists {
			return nil, invalidRecipeManifest("line %d: duplicate key %q", lineNo, key)
		}

		value, err := parseRecipeManifestString(strings.TrimSpace(valuePart))
		if err != nil {
			return nil, invalidRecipeManifest("line %d: %v", lineNo, err)
		}
		fields[key] = value
	}

	for _, key := range []string{"from-version", "to-version", "kind"} {
		if fields[key] == "" {
			return nil, invalidRecipeManifest("missing %s", key)
		}
	}
	return fields, nil
}

func parseRecipeManifestString(value string) (string, error) {
	if !strings.HasPrefix(value, "\"") {
		return "", errors.New("value must be a quoted string")
	}

	escaped := false
	end := -1
	for i := 1; i < len(value); i++ {
		switch {
		case escaped:
			escaped = false
		case value[i] == '\\':
			escaped = true
		case value[i] == '"':
			end = i
			i = len(value)
		}
	}
	if end == -1 {
		return "", errors.New("unterminated quoted string")
	}

	decoded, err := strconv.Unquote(value[:end+1])
	if err != nil {
		return "", fmt.Errorf("invalid quoted string: %v", err)
	}

	rest := strings.TrimSpace(value[end+1:])
	if rest != "" && !strings.HasPrefix(rest, "#") {
		return "", errors.New("unexpected trailing data")
	}
	return decoded, nil
}

func isRecipeManifestKey(key string) bool {
	if key == "" {
		return false
	}
	for _, r := range key {
		if r == '-' || r == '_' || unicode.IsLetter(r) || unicode.IsDigit(r) {
			continue
		}
		return false
	}
	return true
}

func validateRecipeVersion(field, version string) error {
	_, _, err := parseRecipeMinorVersion(version)
	if err != nil {
		return invalidRecipeManifest("%s %q must be a minor version like 0.12: %v", field, version, err)
	}
	return nil
}

func validateRecipeVersionOrder(fromVersion, toVersion string) error {
	fromMajor, fromMinor, err := parseRecipeMinorVersion(fromVersion)
	if err != nil {
		return invalidRecipeManifest("from-version %q must be a minor version like 0.12: %v", fromVersion, err)
	}
	toMajor, toMinor, err := parseRecipeMinorVersion(toVersion)
	if err != nil {
		return invalidRecipeManifest("to-version %q must be a minor version like 0.12: %v", toVersion, err)
	}
	if fromMajor > toMajor || (fromMajor == toMajor && fromMinor >= toMinor) {
		return invalidRecipeManifest("from-version %q must be earlier than to-version %q", fromVersion, toVersion)
	}
	return nil
}

func parseRecipeMinorVersion(version string) (int, int, error) {
	parts := strings.Split(version, ".")
	if len(parts) != 2 {
		return 0, 0, errors.New("expected exactly two numeric segments")
	}

	major, err := parseRecipeVersionSegment(parts[0])
	if err != nil {
		return 0, 0, fmt.Errorf("major: %v", err)
	}
	minor, err := parseRecipeVersionSegment(parts[1])
	if err != nil {
		return 0, 0, fmt.Errorf("minor: %v", err)
	}
	return major, minor, nil
}

func parseRecipeVersionSegment(segment string) (int, error) {
	if segment == "" {
		return 0, errors.New("empty segment")
	}
	if len(segment) > 1 && segment[0] == '0' {
		return 0, errors.New("leading zero")
	}
	for _, r := range segment {
		if r < '0' || r > '9' {
			return 0, errors.New("non-numeric segment")
		}
	}
	value, err := strconv.Atoi(segment)
	if err != nil {
		return 0, err
	}
	return value, nil
}

func validateRecipeKind(kind RecipeKind) error {
	switch kind {
	case RecipeKindLanguage, RecipeKindGo:
		return nil
	default:
		return invalidRecipeManifest("kind %q must be %q or %q", kind, RecipeKindLanguage, RecipeKindGo)
	}
}

func validateRecipeName(name string) error {
	if name == "" {
		return invalidRecipeManifest("recipe name is required")
	}
	for i, r := range name {
		valid := r == '-' || r == '_' || (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9')
		if !valid {
			return invalidRecipeManifest("recipe name %q must use lowercase letters, digits, hyphen, or underscore", name)
		}
		if i == 0 && (r == '-' || r == '_') {
			return invalidRecipeManifest("recipe name %q must start with a lowercase letter or digit", name)
		}
	}
	if strings.HasSuffix(name, "-") || strings.HasSuffix(name, "_") {
		return invalidRecipeManifest("recipe name %q must end with a lowercase letter or digit", name)
	}
	return nil
}

func invalidRecipeManifest(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidRecipeManifest, fmt.Sprintf(format, args...))
}
