package deploy

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"

	"lazuli.dev/runtime/lazuli/secret"
)

const (
	// DefaultEnvSecretMask is used by redacted env previews for secret-backed
	// variables.
	DefaultEnvSecretMask = "[REDACTED]"
)

var (
	// ErrInvalidEnvSpec reports invalid deploy env/secrets injection specs.
	ErrInvalidEnvSpec = errors.New("lazuli/deploy: invalid env spec")
)

// SecretRef identifies externally managed secret material used to populate an
// environment variable at deploy time.
type SecretRef = secret.SecretRef

// EnvSpec describes one environment variable to inject during deployment.
//
// Literal variables use Value. Secret-backed variables set Secret with
// SecretRef and must not carry a Value, so dotenv rendering cannot accidentally
// write a secret value. A non-empty SecretRef also marks the spec as secret for
// hand-written structs.
type EnvSpec struct {
	Name      string
	Value     string
	Secret    bool
	SecretRef SecretRef
}

// EnvValue returns a literal deploy environment variable spec.
func EnvValue(name, value string) EnvSpec {
	return EnvSpec{Name: name, Value: value}
}

// Secret returns a deploy secret reference.
func Secret(name string) SecretRef {
	return secret.Ref(name)
}

// VersionedSecret returns a deploy secret reference pinned to version.
func VersionedSecret(name, version string) SecretRef {
	return secret.Ref(name).WithVersion(secret.VersionLabel(version))
}

// SecretEnv returns a secret-backed deploy environment variable spec.
func SecretEnv(name, secretName string) EnvSpec {
	return SecretRefEnv(name, Secret(secretName))
}

// SecretRefEnv returns a secret-backed deploy environment variable spec.
func SecretRefEnv(name string, ref SecretRef) EnvSpec {
	return EnvSpec{Name: name, Secret: true, SecretRef: ref}
}

// ValidateEnvSpecs validates deploy env/secrets injection specs.
func ValidateEnvSpecs(specs []EnvSpec) error {
	_, err := normalizeEnvSpecs(specs, "env_specs")
	return err
}

// RenderEnvPreview renders deterministic dotenv-style preview lines. Literal
// values are shown as rendered. Secret-backed values are replaced with
// DefaultEnvSecretMask and annotated with their secret reference.
func RenderEnvPreview(specs []EnvSpec) (string, error) {
	return RenderEnvPreviewWithMask(specs, DefaultEnvSecretMask)
}

// RenderEnvPreviewWithMask renders deterministic dotenv-style preview lines
// using mask for secret-backed values. Empty mask uses DefaultEnvSecretMask.
func RenderEnvPreviewWithMask(specs []EnvSpec, mask string) (string, error) {
	if mask == "" {
		mask = DefaultEnvSecretMask
	}
	normalized, err := normalizeEnvSpecs(specs, "env_specs")
	if err != nil {
		return "", err
	}

	var b strings.Builder
	for _, spec := range normalized {
		b.WriteString(spec.Name)
		b.WriteByte('=')
		if spec.isSecret {
			b.WriteString(quoteEnvValue(mask))
			b.WriteString(" # secret: ")
			b.WriteString(renderSecretRef(spec.SecretRef))
		} else {
			b.WriteString(quoteEnvValue(spec.Value))
		}
		b.WriteByte('\n')
	}
	return b.String(), nil
}

// RenderEnvDotenv renders deterministic KEY=value dotenv lines for literal
// deploy env vars. Secret-backed variables are omitted so secret values are
// never written to dotenv output.
func RenderEnvDotenv(specs []EnvSpec) (string, error) {
	normalized, err := normalizeEnvSpecs(specs, "env_specs")
	if err != nil {
		return "", err
	}

	vars := make([]EnvVar, 0, len(normalized))
	for _, spec := range normalized {
		if spec.isSecret {
			continue
		}
		vars = append(vars, Env(spec.Name, spec.Value))
	}
	return RenderEnv(vars)
}

type normalizedEnvSpec struct {
	Name      string
	Value     string
	SecretRef SecretRef
	isSecret  bool
}

func normalizeEnvSpecs(specs []EnvSpec, field string) ([]normalizedEnvSpec, error) {
	normalized := make([]normalizedEnvSpec, 0, len(specs))
	seen := make(map[string]struct{}, len(specs))
	var errs []error

	for i, spec := range specs {
		itemField := fmt.Sprintf("%s[%d]", field, i)
		spec.Name = strings.TrimSpace(spec.Name)
		if spec.Name == "" {
			errs = append(errs, fmt.Errorf("%s.name is required", itemField))
			continue
		}
		if !validEnvName(spec.Name) {
			errs = append(errs, fmt.Errorf("%s.name %q is invalid", itemField, spec.Name))
			continue
		}
		if _, ok := seen[spec.Name]; ok {
			errs = append(errs, fmt.Errorf("%s.name duplicate %q", itemField, spec.Name))
			continue
		}
		seen[spec.Name] = struct{}{}

		out := normalizedEnvSpec{
			Name:     spec.Name,
			Value:    spec.Value,
			isSecret: spec.Secret || hasSecretRef(spec.SecretRef),
		}
		if out.isSecret {
			if spec.Value != "" {
				errs = append(errs, fmt.Errorf("%s.value must be empty when secret_ref is set", itemField))
				continue
			}
			ref, err := normalizeDeploySecretRef(spec.SecretRef)
			if err != nil {
				errs = append(errs, fmt.Errorf("%s.secret_ref: %w", itemField, err))
				continue
			}
			out.SecretRef = ref
		}
		normalized = append(normalized, out)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInvalidEnvSpec, err)
	}

	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, nil
}

func hasSecretRef(ref SecretRef) bool {
	return strings.TrimSpace(ref.Name) != "" || strings.TrimSpace(string(ref.Version)) != ""
}

func normalizeDeploySecretRef(ref SecretRef) (SecretRef, error) {
	ref.Name = strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(ref.Name), "env."))
	ref.Version = secret.VersionLabel(strings.TrimSpace(string(ref.Version)))
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

func renderSecretRef(ref SecretRef) string {
	if ref.Version == "" {
		return ref.Name
	}
	return ref.Name + "@" + string(ref.Version)
}
