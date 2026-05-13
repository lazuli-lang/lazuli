package deploy

import (
	"errors"
	"fmt"
	"path"
	"sort"
	"strings"
	"unicode"
)

const (
	// DefaultGOARCH is the target architecture used by cross-compile plans when
	// no target is provided.
	DefaultGOARCH = "amd64"
	// DefaultCrossCompileOutputDir is where planned build outputs are written.
	DefaultCrossCompileOutputDir = "dist"

	// DefaultCGOMode is the CGO mode used by cross-compile plans.
	DefaultCGOMode = CGOModeDisabled
)

// CGOMode controls whether a planned Go build sets CGO_ENABLED.
type CGOMode string

const (
	// CGOModeDefault omits CGO_ENABLED and lets the Go toolchain decide.
	CGOModeDefault CGOMode = "default"
	// CGOModeDisabled plans builds with CGO_ENABLED=0.
	CGOModeDisabled CGOMode = "disabled"
	// CGOModeEnabled plans builds with CGO_ENABLED=1.
	CGOModeEnabled CGOMode = "enabled"
)

// ErrInvalidCrossCompileConfig reports an invalid cross-compile planning
// config.
var ErrInvalidCrossCompileConfig = errors.New("lazuli/deploy: invalid cross compile config")

// GoBuildTarget identifies one GOOS/GOARCH build target.
type GoBuildTarget struct {
	GOOS   string
	GOARCH string
}

// CrossCompileConfig configures deterministic Go build commands for a set of
// GOOS/GOARCH targets. Plans only describe commands; they never run builds.
type CrossCompileConfig struct {
	// Targets is sorted and deduplicated. Empty uses linux/amd64.
	Targets []GoBuildTarget
	// MainPackage is the single Go package passed to go build.
	MainPackage string
	// BinaryName is the base output binary name.
	BinaryName string
	// OutputDir is the directory prefix for target-specific binary outputs.
	OutputDir string
	// CGO controls CGO_ENABLED. Empty uses DefaultCGOMode.
	CGO CGOMode
	// BuildTags are sorted, deduplicated, and passed as -tags.
	BuildTags []string
	// LDFlags are trimmed and passed as one deterministic -ldflags argument.
	LDFlags []string
}

// CrossCompilePlan is a deterministic, execution-neutral set of Go build
// commands.
type CrossCompilePlan struct {
	Builds      []CrossCompileBuild
	MainPackage string
	BinaryName  string
	OutputDir   string
	CGO         CGOMode
	BuildTags   []string
	LDFlags     []string
}

// CrossCompileBuild is one planned Go build command.
type CrossCompileBuild struct {
	Target       GoBuildTarget
	Environment  []EnvVar
	Argv         []string
	Output       string
	MainPackage  string
	CGO          CGOMode
	BuildTags    []string
	LDFlags      []string
	StaticBinary bool
}

// GoTarget returns a GOOS/GOARCH build target.
func GoTarget(goos, goarch string) GoBuildTarget {
	return GoBuildTarget{GOOS: goos, GOARCH: goarch}
}

// DefaultGoTargetMatrix returns the default cross-compile target matrix.
func DefaultGoTargetMatrix() []GoBuildTarget {
	return []GoBuildTarget{GoTarget(DefaultGOOS, DefaultGOARCH)}
}

// BuildGoTargetMatrix returns the sorted, deduplicated cross product of GOOS
// and GOARCH values.
func BuildGoTargetMatrix(gooses, goarches []string) ([]GoBuildTarget, error) {
	normalizedGOOS, err := normalizeGoTargetValues(gooses, "goos")
	if err != nil {
		return nil, err
	}
	normalizedGOARCH, err := normalizeGoTargetValues(goarches, "goarch")
	if err != nil {
		return nil, err
	}

	targets := make([]GoBuildTarget, 0, len(normalizedGOOS)*len(normalizedGOARCH))
	for _, goos := range normalizedGOOS {
		for _, goarch := range normalizedGOARCH {
			targets = append(targets, GoTarget(goos, goarch))
		}
	}
	return targets, nil
}

// BuildCrossCompilePlan returns deterministic go build argv and environment
// metadata for each configured GOOS/GOARCH target.
func BuildCrossCompilePlan(config CrossCompileConfig) (CrossCompilePlan, error) {
	normalized, err := normalizeCrossCompileConfig(config)
	if err != nil {
		return CrossCompilePlan{}, err
	}

	builds := make([]CrossCompileBuild, 0, len(normalized.Targets))
	for _, target := range normalized.Targets {
		output := path.Join(normalized.OutputDir, targetOutputBinaryName(normalized.BinaryName, target))
		env := goBuildEnvironment(target, normalized.CGO)
		builds = append(builds, CrossCompileBuild{
			Target:       target,
			Environment:  env,
			Argv:         goBuildArgv(normalized, output),
			Output:       output,
			MainPackage:  normalized.MainPackage,
			CGO:          normalized.CGO,
			BuildTags:    append([]string(nil), normalized.BuildTags...),
			LDFlags:      append([]string(nil), normalized.LDFlags...),
			StaticBinary: staticBinaryForTarget(target, normalized.CGO),
		})
	}

	return CrossCompilePlan{
		Builds:      builds,
		MainPackage: normalized.MainPackage,
		BinaryName:  normalized.BinaryName,
		OutputDir:   normalized.OutputDir,
		CGO:         normalized.CGO,
		BuildTags:   append([]string(nil), normalized.BuildTags...),
		LDFlags:     append([]string(nil), normalized.LDFlags...),
	}, nil
}

// ValidateCrossCompileConfig reports whether config can be planned after
// defaults are applied.
func ValidateCrossCompileConfig(config CrossCompileConfig) error {
	_, err := normalizeCrossCompileConfig(config)
	return err
}

func normalizeCrossCompileConfig(config CrossCompileConfig) (CrossCompileConfig, error) {
	var errs []error

	targets, err := normalizeGoBuildTargets(config.Targets, "targets")
	if err != nil {
		errs = append(errs, err)
	}
	config.Targets = targets

	config.MainPackage = strings.TrimSpace(config.MainPackage)
	if config.MainPackage == "" {
		config.MainPackage = DefaultMainPackage
	}
	if !safeGoPackage(config.MainPackage) {
		errs = append(errs, invalidCrossCompileConfig("main_package", "must be a single safe Go package token"))
	}

	config.BinaryName = strings.TrimSpace(config.BinaryName)
	if config.BinaryName == "" {
		config.BinaryName = DefaultBinaryName
	}
	if !safeFileName(config.BinaryName) {
		errs = append(errs, invalidCrossCompileConfig("binary_name", "must be a safe file name"))
	}

	config.OutputDir = strings.TrimSpace(config.OutputDir)
	if config.OutputDir == "" {
		config.OutputDir = DefaultCrossCompileOutputDir
	}
	if !safeOutputDir(config.OutputDir) {
		errs = append(errs, invalidCrossCompileConfig("output_dir", "must be a safe relative or absolute path"))
	}
	config.OutputDir = path.Clean(config.OutputDir)

	config.CGO = normalizeCGOMode(config.CGO)
	if !validCGOMode(config.CGO) {
		errs = append(errs, invalidCrossCompileConfig("cgo", "must be default, disabled, or enabled"))
	}

	tags, tagErrs := normalizeBuildTags(config.BuildTags, "build_tags")
	errs = append(errs, tagErrs...)
	config.BuildTags = tags

	ldflags, ldflagErrs := normalizeLDFlags(config.LDFlags, "ldflags")
	errs = append(errs, ldflagErrs...)
	config.LDFlags = ldflags

	if err := errors.Join(errs...); err != nil {
		return CrossCompileConfig{}, err
	}
	return config, nil
}

func normalizeGoBuildTargets(targets []GoBuildTarget, field string) ([]GoBuildTarget, error) {
	if len(targets) == 0 {
		return DefaultGoTargetMatrix(), nil
	}

	normalized := make([]GoBuildTarget, 0, len(targets))
	seen := make(map[string]struct{}, len(targets))
	var errs []error
	for i, target := range targets {
		itemField := fmt.Sprintf("%s[%d]", field, i)
		target.GOOS = strings.TrimSpace(target.GOOS)
		target.GOARCH = strings.TrimSpace(target.GOARCH)
		if !safeGoTarget(target.GOOS) {
			errs = append(errs, invalidCrossCompileConfig(itemField+".goos", "must contain only letters, digits, or underscores"))
			continue
		}
		if !safeGoTarget(target.GOARCH) {
			errs = append(errs, invalidCrossCompileConfig(itemField+".goarch", "must contain only letters, digits, or underscores"))
			continue
		}
		key := target.GOOS + "/" + target.GOARCH
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		normalized = append(normalized, target)
	}
	sortGoBuildTargets(normalized)

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizeGoTargetValues(values []string, field string) ([]string, error) {
	if len(values) == 0 {
		return nil, invalidCrossCompileConfig(field, "at least one value is required")
	}

	normalized := make([]string, 0, len(values))
	seen := make(map[string]struct{}, len(values))
	var errs []error
	for i, value := range values {
		value = strings.TrimSpace(value)
		if !safeGoTarget(value) {
			errs = append(errs, invalidCrossCompileConfig(fmt.Sprintf("%s[%d]", field, i), "must contain only letters, digits, or underscores"))
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		normalized = append(normalized, value)
	}
	sort.Strings(normalized)

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func sortGoBuildTargets(targets []GoBuildTarget) {
	sort.SliceStable(targets, func(i, j int) bool {
		if targets[i].GOOS != targets[j].GOOS {
			return targets[i].GOOS < targets[j].GOOS
		}
		return targets[i].GOARCH < targets[j].GOARCH
	})
}

func normalizeCGOMode(mode CGOMode) CGOMode {
	mode = CGOMode(strings.ToLower(strings.TrimSpace(string(mode))))
	if mode == "" {
		return DefaultCGOMode
	}
	return mode
}

func validCGOMode(mode CGOMode) bool {
	switch mode {
	case CGOModeDefault, CGOModeDisabled, CGOModeEnabled:
		return true
	default:
		return false
	}
}

func normalizeBuildTags(tags []string, field string) ([]string, []error) {
	normalized := make([]string, 0, len(tags))
	seen := make(map[string]struct{}, len(tags))
	var errs []error
	for i, tag := range tags {
		tag = strings.TrimSpace(tag)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if tag == "" {
			errs = append(errs, invalidCrossCompileConfig(itemField, "value is required"))
			continue
		}
		if !safeBuildTag(tag) {
			errs = append(errs, invalidCrossCompileConfig(itemField, "must be a safe Go build tag"))
			continue
		}
		if _, ok := seen[tag]; ok {
			continue
		}
		seen[tag] = struct{}{}
		normalized = append(normalized, tag)
	}
	sort.Strings(normalized)
	return normalized, errs
}

func normalizeLDFlags(flags []string, field string) ([]string, []error) {
	normalized := make([]string, 0, len(flags))
	var errs []error
	for i, flag := range flags {
		flag = strings.TrimSpace(flag)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if flag == "" {
			errs = append(errs, invalidCrossCompileConfig(itemField, "value is required"))
			continue
		}
		if hasControlRune(flag) {
			errs = append(errs, invalidCrossCompileConfig(itemField, "cannot contain control characters"))
			continue
		}
		normalized = append(normalized, flag)
	}
	return normalized, errs
}

func goBuildEnvironment(target GoBuildTarget, cgo CGOMode) []EnvVar {
	env := make([]EnvVar, 0, 3)
	switch cgo {
	case CGOModeDisabled:
		env = append(env, Env("CGO_ENABLED", "0"))
	case CGOModeEnabled:
		env = append(env, Env("CGO_ENABLED", "1"))
	}
	env = append(env,
		Env("GOOS", target.GOOS),
		Env("GOARCH", target.GOARCH),
	)
	return env
}

func goBuildArgv(config CrossCompileConfig, output string) []string {
	argv := []string{"go", "build", "-trimpath"}
	if len(config.BuildTags) > 0 {
		argv = append(argv, "-tags", strings.Join(config.BuildTags, ","))
	}
	if len(config.LDFlags) > 0 {
		argv = append(argv, "-ldflags", strings.Join(config.LDFlags, " "))
	}
	argv = append(argv, "-o", output, config.MainPackage)
	return argv
}

func targetOutputBinaryName(binaryName string, target GoBuildTarget) string {
	name := fmt.Sprintf("%s_%s_%s", binaryName, target.GOOS, target.GOARCH)
	if target.GOOS == "windows" && !strings.HasSuffix(strings.ToLower(name), ".exe") {
		name += ".exe"
	}
	return name
}

func staticBinaryForTarget(target GoBuildTarget, cgo CGOMode) bool {
	return target.GOOS == "linux" && cgo == CGOModeDisabled
}

func safeOutputDir(value string) bool {
	if value == "" || strings.Contains(value, "\\") || hasControlRune(value) {
		return false
	}
	if strings.ContainsAny(value, "\"'`$;&|<>") {
		return false
	}
	for _, segment := range strings.Split(value, "/") {
		if segment == ".." {
			return false
		}
	}
	return path.Clean(value) != ""
}

func safeBuildTag(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	for _, r := range value {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r >= '0' && r <= '9':
		case r == '_', r == '.':
		default:
			return false
		}
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return false
		}
	}
	return true
}

func invalidCrossCompileConfig(field, message string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidCrossCompileConfig, field, message)
}
