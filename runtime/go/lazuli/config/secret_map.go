package config

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"

	"lazuli.dev/runtime/lazuli/secret"
)

var (
	// ErrInvalidSecretMapping reports a malformed config secret mapping.
	ErrInvalidSecretMapping = errors.New("lazuli/config: invalid secret mapping")
	// ErrSecretMapCollision reports two mappings targeting the same config key
	// or environment variable.
	ErrSecretMapCollision = errors.New("lazuli/config: secret map collision")
)

// SecretRef identifies provider-neutral secret material used for a config key.
type SecretRef = secret.SecretRef

// SecretMapping connects a config key to the environment variable populated
// from SecretRef.
//
// Key defaults to Env when empty. Env defaults to Key when empty, which is only
// valid when Key is also a valid environment variable name. SecretRef defaults
// to the normalized Env variable reference when omitted.
type SecretMapping struct {
	Key       string
	Env       string
	SecretRef SecretRef
	Required  bool
}

// SecretMap is an ordered set of config secret mappings.
type SecretMap []SecretMapping

// SecretEnv returns a required secret mapping whose secret reference defaults
// to env.
func SecretEnv(key, env string) SecretMapping {
	return RequiredSecretEnv(key, env)
}

// SecretRefEnv returns a required secret mapping backed by ref.
func SecretRefEnv(key, env string, ref SecretRef) SecretMapping {
	return RequiredSecretRefEnv(key, env, ref)
}

// RequiredSecretEnv returns a required secret mapping whose secret reference
// defaults to env.
func RequiredSecretEnv(key, env string) SecretMapping {
	return SecretMapping{Key: key, Env: env, Required: true}
}

// OptionalSecretEnv returns an optional secret mapping whose secret reference
// defaults to env.
func OptionalSecretEnv(key, env string) SecretMapping {
	return SecretMapping{Key: key, Env: env}
}

// RequiredSecretRefEnv returns a required secret mapping backed by ref.
func RequiredSecretRefEnv(key, env string, ref SecretRef) SecretMapping {
	return SecretMapping{Key: key, Env: env, SecretRef: ref, Required: true}
}

// OptionalSecretRefEnv returns an optional secret mapping backed by ref.
func OptionalSecretRefEnv(key, env string, ref SecretRef) SecretMapping {
	return SecretMapping{Key: key, Env: env, SecretRef: ref}
}

// NewSecretMap returns a normalized secret map or an error when mappings are
// malformed or collide.
func NewSecretMap(mappings ...SecretMapping) (SecretMap, error) {
	return normalizeSecretMap(mappings)
}

// ValidateSecretMap checks that mappings are well-formed and non-colliding.
func ValidateSecretMap(mappings []SecretMapping) error {
	_, err := normalizeSecretMap(mappings)
	return err
}

// Validate checks that mapping is well-formed.
func (m SecretMapping) Validate() error {
	_, err := normalizeSecretMapping(m, "secret_mapping")
	return err
}

// Validate checks that mappings are well-formed and non-colliding.
func (m SecretMap) Validate() error {
	return ValidateSecretMap([]SecretMapping(m))
}

// Schema returns an env schema for loading mapped secret values. Every field is
// a string and is marked for redaction.
func (m SecretMap) Schema() (Schema, error) {
	normalized, err := normalizeSecretMap([]SecretMapping(m))
	if err != nil {
		return nil, err
	}

	schema := make(Schema, 0, len(normalized))
	for _, mapping := range normalized {
		schema = append(schema, Field{
			Name:     mapping.Key,
			Env:      mapping.Env,
			Type:     String,
			Required: mapping.Required,
			Redact:   true,
		})
	}
	return schema, nil
}

// LoadEnv loads secret values using the mapped env schema.
func (m SecretMap) LoadEnv(lookup LookupFunc) (Values, error) {
	schema, err := m.Schema()
	if err != nil {
		return nil, err
	}
	return LoadEnv(schema, lookup)
}

// RedactKeys returns config keys and env names that should be treated as
// sensitive by inspectors and diagnostics.
func (m SecretMap) RedactKeys() ([]string, error) {
	normalized, err := normalizeSecretMap([]SecretMapping(m))
	if err != nil {
		return nil, err
	}

	keys := make([]string, 0, len(normalized)*2)
	for _, mapping := range normalized {
		keys = append(keys, mapping.Key, mapping.Env)
	}
	return uniqueSortedStrings(keys), nil
}

// EnvVars returns env names keyed by config key.
func (m SecretMap) EnvVars() (map[string]string, error) {
	normalized, err := normalizeSecretMap([]SecretMapping(m))
	if err != nil {
		return nil, err
	}

	out := make(map[string]string, len(normalized))
	for _, mapping := range normalized {
		out[mapping.Key] = mapping.Env
	}
	return out, nil
}

// SecretRefs returns secret references keyed by config key.
func (m SecretMap) SecretRefs() (map[string]SecretRef, error) {
	normalized, err := normalizeSecretMap([]SecretMapping(m))
	if err != nil {
		return nil, err
	}

	out := make(map[string]SecretRef, len(normalized))
	for _, mapping := range normalized {
		out[mapping.Key] = mapping.SecretRef
	}
	return out, nil
}

// EnvSecretRefs returns secret references keyed by env name.
func (m SecretMap) EnvSecretRefs() (map[string]SecretRef, error) {
	normalized, err := normalizeSecretMap([]SecretMapping(m))
	if err != nil {
		return nil, err
	}

	out := make(map[string]SecretRef, len(normalized))
	for _, mapping := range normalized {
		out[mapping.Env] = mapping.SecretRef
	}
	return out, nil
}

func normalizeSecretMap(mappings []SecretMapping) (SecretMap, error) {
	normalized := make(SecretMap, 0, len(mappings))
	seenKeys := make(map[string]int, len(mappings))
	seenEnvs := make(map[string]int, len(mappings))
	var errs []error

	for i, mapping := range mappings {
		itemField := fmt.Sprintf("secret_map[%d]", i)
		clean, err := normalizeSecretMapping(mapping, itemField)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		collides := false
		if first, ok := seenKeys[clean.Key]; ok {
			errs = append(errs, fmt.Errorf("%w: %s.key %q also appears at secret_map[%d]", ErrSecretMapCollision, itemField, clean.Key, first))
			collides = true
		} else {
			seenKeys[clean.Key] = i
		}
		if first, ok := seenEnvs[clean.Env]; ok {
			errs = append(errs, fmt.Errorf("%w: %s.env %q also appears at secret_map[%d]", ErrSecretMapCollision, itemField, clean.Env, first))
			collides = true
		} else {
			seenEnvs[clean.Env] = i
		}
		if collides {
			continue
		}
		normalized = append(normalized, clean)
	}

	return normalized, errors.Join(errs...)
}

func normalizeSecretMapping(mapping SecretMapping, field string) (SecretMapping, error) {
	mapping.Key = strings.TrimSpace(mapping.Key)
	mapping.Env = strings.TrimSpace(mapping.Env)
	if mapping.Key == "" {
		mapping.Key = mapping.Env
	}
	if mapping.Env == "" {
		mapping.Env = mapping.Key
	}

	var errs []error
	if mapping.Key == "" {
		errs = append(errs, invalidSecretMapping(field+".key", "value is required"))
	} else if !safeSecretConfigKey(mapping.Key) {
		errs = append(errs, invalidSecretMapping(field+".key", fmt.Sprintf("%q is invalid", mapping.Key)))
	}
	if mapping.Env == "" {
		errs = append(errs, invalidSecretMapping(field+".env", "value is required"))
	} else if !validSecretEnvName(mapping.Env) {
		errs = append(errs, invalidSecretMapping(field+".env", fmt.Sprintf("%q is invalid", mapping.Env)))
	}

	ref, err := normalizeSecretMappingRef(mapping.SecretRef, mapping.Env)
	if err != nil {
		errs = append(errs, invalidSecretMapping(field+".secret_ref", err.Error()))
	}
	mapping.SecretRef = ref

	return mapping, errors.Join(errs...)
}

func normalizeSecretMappingRef(ref SecretRef, env string) (SecretRef, error) {
	ref.Name = strings.TrimPrefix(strings.TrimSpace(ref.Name), "env.")
	ref.Version = secret.VersionLabel(strings.TrimSpace(string(ref.Version)))
	if ref.Name == "" && ref.Version == "" && env != "" {
		ref = secret.Env(env)
	}
	if ref.Name == "" {
		return SecretRef{}, errors.New("name is required")
	}
	if !safeSecretRefPart(ref.Name) {
		return SecretRef{}, fmt.Errorf("name %q is invalid", ref.Name)
	}
	if ref.Version != "" && !safeSecretRefPart(string(ref.Version)) {
		return SecretRef{}, fmt.Errorf("version %q is invalid", ref.Version)
	}
	return ref, nil
}

func safeSecretConfigKey(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return false
		}
	}
	return true
}

func validSecretEnvName(value string) bool {
	if value == "" {
		return false
	}
	for i, r := range value {
		ok := r == '_' || (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (i > 0 && r >= '0' && r <= '9')
		if !ok {
			return false
		}
	}
	return true
}

func safeSecretRefPart(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return false
		}
	}
	return true
}

func uniqueSortedStrings(values []string) []string {
	seen := make(map[string]struct{}, len(values))
	out := make([]string, 0, len(values))
	for _, value := range values {
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out
}

func invalidSecretMapping(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidSecretMapping, field, detail)
}
