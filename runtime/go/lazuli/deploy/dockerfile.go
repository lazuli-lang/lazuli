// Package deploy provides small helpers for generating deployment artifacts.
package deploy

import (
	"errors"
	"fmt"
	"path"
	"sort"
	"strconv"
	"strings"
	"unicode"
)

const (
	// DefaultGoVersion is used for the Go builder image when no version is set.
	DefaultGoVersion = "1.25"
	// DefaultBuildWorkDir is the working directory used in the build stage.
	DefaultBuildWorkDir = "/src"
	// DefaultBinaryName is the binary name copied into the runtime stage.
	DefaultBinaryName = "app"
	// DefaultMainPackage is the Go package built by default.
	DefaultMainPackage = "."
	// DefaultGOOS is the target operating system for generated Go builds.
	DefaultGOOS = "linux"

	defaultDistrolessStaticImage = "gcr.io/distroless/static-debian12:nonroot"
	defaultScratchUser           = "65532:65532"
	defaultDistrolessUser        = "nonroot:nonroot"
)

// ErrInvalidDockerfileConfig is returned when a Dockerfile config is unsafe or
// incomplete after defaults are applied.
var ErrInvalidDockerfileConfig = errors.New("lazuli/deploy: invalid dockerfile config")

// GoBuildStage configures the builder stage of a generated Go Dockerfile.
type GoBuildStage struct {
	// GoVersion selects the golang:<version> builder image when Image is empty.
	GoVersion string
	// Image overrides the builder image. When empty, golang:<GoVersion> is used.
	Image string
	// WorkDir is the absolute container path used as the build working directory.
	WorkDir string
	// MainPackage is the single Go package passed to go build.
	MainPackage string
	// BinaryName is the output binary file name.
	BinaryName string
	// GOOS is the target operating system. When empty, linux is used.
	GOOS string
	// GOARCH optionally pins the target architecture.
	GOARCH string
}

// GoRuntimeStage configures the runtime stage of a generated Go Dockerfile.
type GoRuntimeStage struct {
	// Image is the runtime image. When empty, DistrolessStaticRuntime is used.
	Image string
	// User optionally emits a USER instruction in the runtime stage.
	User string
	// Static builds with CGO_ENABLED=0 for static runtime images.
	Static bool
}

// GoDockerfileConfig configures a deterministic multi-stage Dockerfile for a
// Go application.
type GoDockerfileConfig struct {
	Build GoBuildStage

	// Runtime controls the final stage image. Empty uses DistrolessStaticRuntime.
	Runtime GoRuntimeStage

	// Labels emits sorted LABEL instructions in the runtime stage.
	Labels map[string]string
	// Env emits sorted ENV instructions in the runtime stage.
	Env map[string]string
	// Expose emits one sorted EXPOSE instruction with unique TCP ports.
	Expose []int
}

// DefaultGoDockerfileConfig returns the defaults used by GenerateGoDockerfile.
func DefaultGoDockerfileConfig() GoDockerfileConfig {
	return GoDockerfileConfig{
		Build: GoBuildStage{
			GoVersion:   DefaultGoVersion,
			WorkDir:     DefaultBuildWorkDir,
			MainPackage: DefaultMainPackage,
			BinaryName:  DefaultBinaryName,
			GOOS:        DefaultGOOS,
		},
		Runtime: DistrolessStaticRuntime(),
	}
}

// DistrolessStaticRuntime returns the default distroless/static runtime stage.
func DistrolessStaticRuntime() GoRuntimeStage {
	return GoRuntimeStage{
		Image:  defaultDistrolessStaticImage,
		User:   defaultDistrolessUser,
		Static: true,
	}
}

// ScratchStaticRuntime returns a scratch runtime stage for static Go binaries.
func ScratchStaticRuntime() GoRuntimeStage {
	return GoRuntimeStage{
		Image:  "scratch",
		User:   defaultScratchUser,
		Static: true,
	}
}

// CustomRuntime returns a runtime stage for a caller-supplied image.
func CustomRuntime(image string) GoRuntimeStage {
	return GoRuntimeStage{Image: image}
}

// GenerateGoDockerfile returns Dockerfile text for a multi-stage Go build. It
// only generates text; it never invokes Docker.
func GenerateGoDockerfile(config GoDockerfileConfig) (string, error) {
	normalized, err := normalizeGoDockerfileConfig(config)
	if err != nil {
		return "", err
	}

	buildEnv := []string{"GOOS=" + normalized.Build.GOOS}
	if normalized.Runtime.Static {
		buildEnv = append([]string{"CGO_ENABLED=0"}, buildEnv...)
	}
	if normalized.Build.GOARCH != "" {
		buildEnv = append(buildEnv, "GOARCH="+normalized.Build.GOARCH)
	}

	builderOutput := path.Join("/out", normalized.Build.BinaryName)
	runtimeOutput := path.Join("/", normalized.Build.BinaryName)

	var out strings.Builder
	out.WriteString("FROM ")
	out.WriteString(normalized.Build.Image)
	out.WriteString(" AS build\n")
	out.WriteString("WORKDIR ")
	out.WriteString(normalized.Build.WorkDir)
	out.WriteString("\n")
	out.WriteString("COPY go.mod go.sum ./\n")
	out.WriteString("RUN go mod download\n")
	out.WriteString("COPY . .\n")
	out.WriteString("RUN ")
	if len(buildEnv) > 0 {
		out.WriteString(strings.Join(buildEnv, " "))
		out.WriteString(" ")
	}
	out.WriteString("go build -trimpath -ldflags=\"-s -w\" -o ")
	out.WriteString(builderOutput)
	out.WriteString(" ")
	out.WriteString(normalized.Build.MainPackage)
	out.WriteString("\n\n")

	out.WriteString("FROM ")
	out.WriteString(normalized.Runtime.Image)
	out.WriteString("\n")
	writeLabels(&out, normalized.Labels)
	writeEnv(&out, normalized.Env)
	writeExpose(&out, normalized.Expose)
	out.WriteString("COPY --from=build ")
	out.WriteString(builderOutput)
	out.WriteString(" ")
	out.WriteString(runtimeOutput)
	out.WriteString("\n")
	if normalized.Runtime.User != "" {
		out.WriteString("USER ")
		out.WriteString(normalized.Runtime.User)
		out.WriteString("\n")
	}
	out.WriteString("ENTRYPOINT [")
	out.WriteString(strconv.Quote(runtimeOutput))
	out.WriteString("]\n")

	return out.String(), nil
}

// ValidateGoDockerfileConfig reports whether config can be rendered by
// GenerateGoDockerfile after defaults are applied.
func ValidateGoDockerfileConfig(config GoDockerfileConfig) error {
	_, err := normalizeGoDockerfileConfig(config)
	return err
}

func normalizeGoDockerfileConfig(config GoDockerfileConfig) (GoDockerfileConfig, error) {
	defaults := DefaultGoDockerfileConfig()

	config.Build.GoVersion = strings.TrimSpace(config.Build.GoVersion)
	config.Build.Image = strings.TrimSpace(config.Build.Image)
	config.Build.WorkDir = strings.TrimSpace(config.Build.WorkDir)
	config.Build.MainPackage = strings.TrimSpace(config.Build.MainPackage)
	config.Build.BinaryName = strings.TrimSpace(config.Build.BinaryName)
	config.Build.GOOS = strings.TrimSpace(config.Build.GOOS)
	config.Build.GOARCH = strings.TrimSpace(config.Build.GOARCH)
	config.Runtime.Image = strings.TrimSpace(config.Runtime.Image)
	config.Runtime.User = strings.TrimSpace(config.Runtime.User)

	if config.Build.GoVersion == "" {
		config.Build.GoVersion = defaults.Build.GoVersion
	}
	if config.Build.Image == "" {
		config.Build.Image = "golang:" + config.Build.GoVersion
	}
	if config.Build.WorkDir == "" {
		config.Build.WorkDir = defaults.Build.WorkDir
	}
	if config.Build.MainPackage == "" {
		config.Build.MainPackage = defaults.Build.MainPackage
	}
	if config.Build.BinaryName == "" {
		config.Build.BinaryName = defaults.Build.BinaryName
	}
	if config.Build.GOOS == "" {
		config.Build.GOOS = defaults.Build.GOOS
	}
	if config.Runtime.Image == "" {
		config.Runtime.Image = defaults.Runtime.Image
		if config.Runtime.User == "" {
			config.Runtime.User = defaults.Runtime.User
		}
		config.Runtime.Static = defaults.Runtime.Static
	}

	if err := validateGoDockerfileConfig(config); err != nil {
		return GoDockerfileConfig{}, err
	}

	config.Build.WorkDir = path.Clean(config.Build.WorkDir)
	config.Expose = normalizeExpose(config.Expose)
	return config, nil
}

func validateGoDockerfileConfig(config GoDockerfileConfig) error {
	if !safeImageRef(config.Build.Image) {
		return invalidDockerfileConfig("build.image", "must be a non-empty Docker image reference without whitespace or control characters")
	}
	if !safeImageRef(config.Runtime.Image) {
		return invalidDockerfileConfig("runtime.image", "must be a non-empty Docker image reference without whitespace or control characters")
	}
	if !safeContainerPath(config.Build.WorkDir) {
		return invalidDockerfileConfig("build.work_dir", "must be a safe absolute container path")
	}
	if !safeGoPackage(config.Build.MainPackage) {
		return invalidDockerfileConfig("build.main_package", "must be a single safe Go package token")
	}
	if !safeFileName(config.Build.BinaryName) {
		return invalidDockerfileConfig("build.binary_name", "must be a safe file name")
	}
	if !safeGoTarget(config.Build.GOOS) {
		return invalidDockerfileConfig("build.goos", "must contain only letters, digits, or underscores")
	}
	if config.Build.GOARCH != "" && !safeGoTarget(config.Build.GOARCH) {
		return invalidDockerfileConfig("build.goarch", "must contain only letters, digits, or underscores")
	}
	if config.Runtime.User != "" && !safeDockerToken(config.Runtime.User) {
		return invalidDockerfileConfig("runtime.user", "must be a safe Dockerfile token")
	}
	if err := validateLabels(config.Labels); err != nil {
		return err
	}
	if err := validateEnv(config.Env); err != nil {
		return err
	}
	if err := validateExpose(config.Expose); err != nil {
		return err
	}
	return nil
}

func validateLabels(labels map[string]string) error {
	for _, key := range sortedMapKeys(labels) {
		if !safeLabelKey(key) {
			return invalidDockerfileConfig("labels."+key, "label key must be non-empty and cannot contain whitespace, equals signs, or control characters")
		}
		if hasControlRune(labels[key]) {
			return invalidDockerfileConfig("labels."+key, "label value cannot contain control characters")
		}
	}
	return nil
}

func validateEnv(env map[string]string) error {
	for _, key := range sortedMapKeys(env) {
		if !safeEnvKey(key) {
			return invalidDockerfileConfig("env."+key, "env key must match [A-Za-z_][A-Za-z0-9_]*")
		}
		if hasControlRune(env[key]) {
			return invalidDockerfileConfig("env."+key, "env value cannot contain control characters")
		}
	}
	return nil
}

func validateExpose(ports []int) error {
	for _, port := range ports {
		if port < 1 || port > 65535 {
			return invalidDockerfileConfig("expose", fmt.Sprintf("port %d is outside 1-65535", port))
		}
	}
	return nil
}

func writeLabels(out *strings.Builder, labels map[string]string) {
	for _, key := range sortedMapKeys(labels) {
		out.WriteString("LABEL ")
		out.WriteString(key)
		out.WriteString("=")
		out.WriteString(strconv.Quote(labels[key]))
		out.WriteString("\n")
	}
}

func writeEnv(out *strings.Builder, env map[string]string) {
	for _, key := range sortedMapKeys(env) {
		out.WriteString("ENV ")
		out.WriteString(key)
		out.WriteString("=")
		out.WriteString(strconv.Quote(env[key]))
		out.WriteString("\n")
	}
}

func writeExpose(out *strings.Builder, ports []int) {
	if len(ports) == 0 {
		return
	}
	out.WriteString("EXPOSE")
	for _, port := range ports {
		out.WriteString(" ")
		out.WriteString(strconv.Itoa(port))
	}
	out.WriteString("\n")
}

func normalizeExpose(ports []int) []int {
	if len(ports) == 0 {
		return nil
	}

	seen := make(map[int]struct{}, len(ports))
	normalized := make([]int, 0, len(ports))
	for _, port := range ports {
		if _, ok := seen[port]; ok {
			continue
		}
		seen[port] = struct{}{}
		normalized = append(normalized, port)
	}
	sort.Ints(normalized)
	return normalized
}

func sortedMapKeys(values map[string]string) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func safeImageRef(value string) bool {
	return value != "" && safeDockerToken(value)
}

func safeDockerToken(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) || strings.ContainsRune("\\\"'`$;&|<>", r) {
			return false
		}
	}
	return true
}

func safeContainerPath(value string) bool {
	if value == "" || !strings.HasPrefix(value, "/") || strings.Contains(value, "\\") {
		return false
	}
	if hasControlRune(value) || strings.ContainsAny(value, "\"'") {
		return false
	}
	for _, segment := range strings.Split(value, "/") {
		if segment == ".." {
			return false
		}
	}
	cleaned := path.Clean(value)
	return cleaned != "." && cleaned != "/"
}

func safeGoPackage(value string) bool {
	if value == "" || strings.Contains(value, "...") {
		return false
	}
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) || strings.ContainsRune("\\\"'`$;&|<>", r) {
			return false
		}
	}
	return true
}

func safeFileName(value string) bool {
	if value == "" || value == "." || value == ".." {
		return false
	}
	for _, r := range value {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r >= '0' && r <= '9':
		case r == '.', r == '_', r == '-':
		default:
			return false
		}
	}
	return true
}

func safeGoTarget(value string) bool {
	if value == "" {
		return false
	}
	for _, r := range value {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r >= '0' && r <= '9':
		case r == '_':
		default:
			return false
		}
	}
	return true
}

func safeLabelKey(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) || r == '=' || r == '"' || r == '\'' {
			return false
		}
	}
	return true
}

func safeEnvKey(value string) bool {
	if value == "" {
		return false
	}
	for i, r := range value {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r == '_':
		case i > 0 && r >= '0' && r <= '9':
		default:
			return false
		}
	}
	return true
}

func hasControlRune(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func invalidDockerfileConfig(field, message string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidDockerfileConfig, field, message)
}
