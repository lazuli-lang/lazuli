package testkit

import (
	"errors"
	"fmt"
	"regexp"
	"sort"
	"strings"
	"time"
)

var (
	// ErrRaceTestPlanInvalid reports invalid race test planner inputs.
	ErrRaceTestPlanInvalid = errors.New("lazuli/testkit: race test plan invalid")
)

// RaceTestPlanOptions configures a side-effect-free go test -race run plan.
type RaceTestPlanOptions struct {
	// Packages are the package patterns to pass to go test. Empty defaults to
	// ./... after package skips are applied.
	Packages []string
	// SkipPackages are exact package paths or /... subtree patterns to omit
	// from Packages. Package skips only filter the package entries supplied to
	// the planner; the planner never expands ./... by running go list.
	SkipPackages []string
	// SkipTests are regular expressions combined into one go test -skip flag.
	SkipTests []string
	// Timeout is emitted as -timeout when positive. Zero omits the flag.
	Timeout time.Duration
	// Env is rendered as sorted KEY=value entries for an exec.Cmd Env field.
	Env map[string]string
}

// RaceTestPlan is the normalized command metadata for a go test -race run.
// The planner never executes the command.
type RaceTestPlan struct {
	// Packages are the selected package patterns after SkipPackages filtering.
	Packages []string
	// SkipPackages are the normalized package skip patterns used during
	// planning.
	SkipPackages []string
	// SkipTests are the normalized test name regexes emitted through -skip.
	SkipTests []string
	// Timeout is the planned go test timeout. Zero means no -timeout flag.
	Timeout time.Duration
	// Env is the deterministic list of KEY=value environment assignments.
	Env []string
}

// PlanRaceTests normalizes package, skip, timeout, and environment settings for
// a future go test -race invocation. It does not execute go commands.
func PlanRaceTests(opts RaceTestPlanOptions) (RaceTestPlan, error) {
	plan := RaceTestPlan{
		Timeout: opts.Timeout,
	}
	if opts.Timeout < 0 {
		return plan, fmt.Errorf("%w: timeout must not be negative", ErrRaceTestPlanInvalid)
	}

	packages := normalizeRaceTestValues(opts.Packages)
	if len(packages) == 0 {
		packages = []string{"./..."}
	}
	skipPackages := normalizeRaceTestValues(opts.SkipPackages)
	packages = SelectRaceTestPackages(packages, skipPackages)
	if len(packages) == 0 {
		return plan, fmt.Errorf("%w: no packages selected", ErrRaceTestPlanInvalid)
	}

	skipTests := normalizeRaceTestValues(opts.SkipTests)
	if _, err := raceTestSkipPattern(skipTests); err != nil {
		return plan, err
	}

	env, err := normalizeRaceTestEnv(opts.Env)
	if err != nil {
		return plan, err
	}

	plan.Packages = cloneStrings(packages)
	plan.SkipPackages = cloneStrings(skipPackages)
	plan.SkipTests = cloneStrings(skipTests)
	plan.Env = env
	return plan, nil
}

// SelectRaceTestPackages trims, deduplicates, sorts, and filters packages by
// exact skip entries or skip entries ending in /....
func SelectRaceTestPackages(packages, skipPackages []string) []string {
	packages = normalizeRaceTestValues(packages)
	if len(packages) == 0 {
		return nil
	}

	skipPackages = normalizeRaceTestValues(skipPackages)
	if len(skipPackages) == 0 {
		return packages
	}

	selected := packages[:0]
	for _, pkg := range packages {
		if raceTestPackageSkipped(pkg, skipPackages) {
			continue
		}
		selected = append(selected, pkg)
	}
	return cloneStrings(selected)
}

// CommandArgv returns a deterministic argv for the planned go test -race
// command.
func (p RaceTestPlan) CommandArgv() []string {
	argv := []string{"go", "test", "-race"}
	if p.Timeout > 0 {
		argv = append(argv, "-timeout="+p.Timeout.String())
	}
	if skip := joinRaceTestSkipPatterns(p.SkipTests); skip != "" {
		argv = append(argv, "-skip="+skip)
	}
	packages := normalizeRaceTestValues(p.Packages)
	if len(packages) == 0 {
		packages = []string{"./..."}
	}
	return append(argv, packages...)
}

// CommandEnv returns deterministic KEY=value environment assignments for the
// planned command.
func (p RaceTestPlan) CommandEnv() []string {
	env := cloneStrings(p.Env)
	sort.Strings(env)
	return env
}

func normalizeRaceTestValues(values []string) []string {
	if len(values) == 0 {
		return nil
	}

	seen := make(map[string]struct{}, len(values))
	normalized := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		normalized = append(normalized, value)
	}
	sort.Strings(normalized)
	return normalized
}

func raceTestPackageSkipped(pkg string, skipPackages []string) bool {
	for _, skip := range skipPackages {
		if pkg == skip {
			return true
		}
		if !strings.HasSuffix(skip, "/...") {
			continue
		}

		prefix := strings.TrimSuffix(skip, "/...")
		if pkg == prefix || strings.HasPrefix(pkg, prefix+"/") {
			return true
		}
	}
	return false
}

func raceTestSkipPattern(patterns []string) (string, error) {
	pattern := joinRaceTestSkipPatterns(patterns)
	if pattern == "" {
		return "", nil
	}
	if _, err := regexp.Compile(pattern); err != nil {
		return "", fmt.Errorf("%w: skip test pattern: %v", ErrRaceTestPlanInvalid, err)
	}
	return pattern, nil
}

func joinRaceTestSkipPatterns(patterns []string) string {
	patterns = normalizeRaceTestValues(patterns)
	if len(patterns) == 0 {
		return ""
	}

	parts := make([]string, len(patterns))
	for i, pattern := range patterns {
		parts[i] = "(?:" + pattern + ")"
	}
	return strings.Join(parts, "|")
}

func normalizeRaceTestEnv(env map[string]string) ([]string, error) {
	if len(env) == 0 {
		return nil, nil
	}

	rawKeys := make([]string, 0, len(env))
	for key := range env {
		rawKeys = append(rawKeys, key)
	}
	sort.Strings(rawKeys)

	values := make(map[string]string, len(env))
	normalizedKeys := make([]string, 0, len(env))
	for _, rawKey := range rawKeys {
		key := strings.TrimSpace(rawKey)
		if err := validateEnvKey(key); err != nil {
			return nil, fmt.Errorf("%w: env key %q: %v", ErrRaceTestPlanInvalid, rawKey, err)
		}
		if _, ok := values[key]; ok {
			return nil, fmt.Errorf("%w: env key %q duplicates after trimming", ErrRaceTestPlanInvalid, rawKey)
		}
		values[key] = env[rawKey]
		normalizedKeys = append(normalizedKeys, key)
	}

	sort.Strings(normalizedKeys)
	entries := make([]string, len(normalizedKeys))
	for i, key := range normalizedKeys {
		entries[i] = key + "=" + values[key]
	}
	return entries, nil
}
