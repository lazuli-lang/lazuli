package deploy

import (
	"errors"
	"fmt"
	"path"
	"sort"
	"strings"
)

const (
	// DefaultGitHubActionsWorkflowName is used when no workflow name is set.
	DefaultGitHubActionsWorkflowName = "ci"
	// DefaultGitHubActionsBranch is used when no workflow branch filter is set.
	DefaultGitHubActionsBranch = "main"
	// DefaultGitHubActionsRunner is the default hosted runner label.
	DefaultGitHubActionsRunner = "ubuntu-latest"
	// DefaultGitHubActionsGoVersion is the default Go matrix version.
	DefaultGitHubActionsGoVersion = DefaultGoVersion
	// DefaultGitHubActionsRustToolchain is the default Rust matrix toolchain.
	DefaultGitHubActionsRustToolchain = "stable"
	// DefaultGitHubActionsRustTarget is the default Rust matrix target.
	DefaultGitHubActionsRustTarget = "x86_64-unknown-linux-gnu"
)

var (
	// ErrInvalidGitHubActionsConfig reports an invalid GitHub Actions CI config.
	ErrInvalidGitHubActionsConfig = errors.New("lazuli/deploy: invalid github actions config")
)

// GitHubActionsCIConfig configures a deterministic GitHub Actions CI workflow.
type GitHubActionsCIConfig struct {
	Name     string
	Branches []string
	Env      []EnvSpec

	Go   GitHubActionsGoJob
	Rust GitHubActionsRustJob
}

// GitHubActionsGoJob configures the Go CI job.
type GitHubActionsGoJob struct {
	Enabled bool

	Name       string
	RunsOn     string
	Versions   []string
	WorkingDir string

	TestCommand  []string
	BuildCommand []string

	Env                  []EnvSpec
	CacheDependencyFiles []string
}

// GitHubActionsRustJob configures the Rust CI job.
type GitHubActionsRustJob struct {
	Enabled bool

	Name       string
	RunsOn     string
	Toolchains []string
	Targets    []string
	WorkingDir string

	TestCommand  []string
	BuildCommand []string

	Env                  []EnvSpec
	CacheDependencyFiles []string
}

// GitHubActionsCIPlan is the normalized, execution-neutral workflow plan.
type GitHubActionsCIPlan struct {
	Name     string
	Branches []string
	Env      []GitHubActionsEnvBinding
	Jobs     []GitHubActionsCIJob
}

// GitHubActionsCIJob is one normalized CI job.
type GitHubActionsCIJob struct {
	ID         string
	Name       string
	RunsOn     string
	WorkingDir string
	Matrix     map[string][]string
	Cache      GitHubActionsCache
	Commands   []GitHubActionsCommand
	Env        []GitHubActionsEnvBinding
}

// GitHubActionsCache describes one actions/cache step.
type GitHubActionsCache struct {
	Path            []string
	Key             string
	RestoreKey      string
	DependencyFiles []string
}

// GitHubActionsCommand describes one shell command step.
type GitHubActionsCommand struct {
	Name string
	Run  []string
}

// GitHubActionsEnvBinding is an environment binding with redacted display text.
type GitHubActionsEnvBinding struct {
	Name          string
	Value         string
	Secret        bool
	SecretRef     SecretRef
	RedactedValue string
}

// ValidateGitHubActionsCIConfig reports whether config can be rendered.
func ValidateGitHubActionsCIConfig(config GitHubActionsCIConfig) error {
	_, err := BuildGitHubActionsCIPlan(config)
	return err
}

// BuildGitHubActionsCIPlan returns a normalized CI workflow plan without
// touching the filesystem or network.
func BuildGitHubActionsCIPlan(config GitHubActionsCIConfig) (GitHubActionsCIPlan, error) {
	normalized, err := normalizeGitHubActionsCIConfig(config)
	if err != nil {
		return GitHubActionsCIPlan{}, err
	}
	return normalized, nil
}

// RenderGitHubActionsCIWorkflow renders a deterministic GitHub Actions workflow
// YAML document.
func RenderGitHubActionsCIWorkflow(config GitHubActionsCIConfig) (string, error) {
	plan, err := BuildGitHubActionsCIPlan(config)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	writeScalar(&b, 0, "name", plan.Name)
	b.WriteString("on:\n")
	b.WriteString("  push:\n")
	writeList(&b, 4, "branches", plan.Branches)
	b.WriteString("  pull_request:\n")
	writeList(&b, 4, "branches", plan.Branches)
	if len(plan.Env) > 0 {
		writeGitHubActionsEnv(&b, 0, plan.Env, false)
	}
	b.WriteString("jobs:\n")
	for _, job := range plan.Jobs {
		b.WriteString("  ")
		b.WriteString(job.ID)
		b.WriteString(":\n")
		writeScalar(&b, 4, "name", job.Name)
		writeScalar(&b, 4, "runs-on", job.RunsOn)
		b.WriteString("    strategy:\n")
		b.WriteString("      fail-fast: false\n")
		b.WriteString("      matrix:\n")
		writeGitHubActionsMatrix(&b, 8, job.Matrix)
		if len(job.Env) > 0 {
			writeGitHubActionsEnv(&b, 4, job.Env, false)
		}
		if job.WorkingDir != "" {
			b.WriteString("    defaults:\n")
			b.WriteString("      run:\n")
			writeScalar(&b, 8, "working-directory", job.WorkingDir)
		}
		b.WriteString("    steps:\n")
		writeGitHubActionsStepUses(&b, "Checkout", "actions/checkout@v4", nil)
		switch job.ID {
		case "go":
			writeGitHubActionsStepUses(&b, "Setup Go", "actions/setup-go@v5", map[string]string{
				"go-version": "${{ matrix.go-version }}",
			})
		case "rust":
			writeGitHubActionsStepUses(&b, "Setup Rust", "dtolnay/rust-toolchain@stable", map[string]string{
				"toolchain": "${{ matrix.rust-toolchain }}",
				"target":    "${{ matrix.rust-target }}",
			})
		}
		writeGitHubActionsCacheStep(&b, job.Cache)
		for _, command := range job.Commands {
			writeGitHubActionsRunStep(&b, command)
		}
	}
	return b.String(), nil
}

// RenderGitHubActionsCISummary renders deterministic Markdown with secret
// values redacted.
func RenderGitHubActionsCISummary(config GitHubActionsCIConfig) (string, error) {
	plan, err := BuildGitHubActionsCIPlan(config)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString("# GitHub Actions CI: ")
	b.WriteString(releaseMarkdownHeading(plan.Name))
	b.WriteString("\n\n")
	b.WriteString("| Field | Value |\n")
	b.WriteString("| --- | --- |\n")
	releaseWriteMarkdownRow(&b, "Branches", strings.Join(plan.Branches, ", "))
	if len(plan.Env) > 0 {
		b.WriteByte('\n')
		b.WriteString("## Workflow Environment\n\n")
		writeGitHubActionsSummaryEnv(&b, plan.Env)
	}
	for _, job := range plan.Jobs {
		b.WriteByte('\n')
		b.WriteString("## ")
		b.WriteString(releaseMarkdownHeading(job.Name))
		b.WriteString("\n\n")
		b.WriteString("| Field | Value |\n")
		b.WriteString("| --- | --- |\n")
		releaseWriteMarkdownRow(&b, "Runs on", job.RunsOn)
		releaseWriteMarkdownRow(&b, "Working directory", summaryValue(job.WorkingDir, "."))
		releaseWriteMarkdownRow(&b, "Cache key", job.Cache.Key)
		releaseWriteMarkdownRow(&b, "Restore key", job.Cache.RestoreKey)
		for _, key := range sortedMatrixKeys(job.Matrix) {
			releaseWriteMarkdownRow(&b, "Matrix "+key, strings.Join(job.Matrix[key], ", "))
		}
		if len(job.Env) > 0 {
			b.WriteByte('\n')
			writeGitHubActionsSummaryEnv(&b, job.Env)
		}
		b.WriteByte('\n')
		b.WriteString("| Step | Command |\n")
		b.WriteString("| --- | --- |\n")
		for _, command := range job.Commands {
			releaseWriteMarkdownRow(&b, command.Name, strings.Join(command.Run, " && "))
		}
	}
	return b.String(), nil
}

func normalizeGitHubActionsCIConfig(config GitHubActionsCIConfig) (GitHubActionsCIPlan, error) {
	var errs []error

	name := strings.TrimSpace(config.Name)
	if name == "" {
		name = DefaultGitHubActionsWorkflowName
	}
	if hasControlRune(name) {
		errs = append(errs, invalidGitHubActionsConfig("name", "cannot contain control characters"))
	}

	branches, branchErrs := normalizeGitHubActionsList(config.Branches, "branches", DefaultGitHubActionsBranch, safeGitHubActionsBranch)
	errs = append(errs, branchErrs...)

	env, err := normalizeGitHubActionsEnv(config.Env, "env")
	if err != nil {
		errs = append(errs, err)
	}

	var jobs []GitHubActionsCIJob
	if config.Go.Enabled {
		job, jobErrs := normalizeGitHubActionsGoJob(config.Go)
		errs = append(errs, jobErrs...)
		if job.ID != "" {
			jobs = append(jobs, job)
		}
	}
	if config.Rust.Enabled {
		job, jobErrs := normalizeGitHubActionsRustJob(config.Rust)
		errs = append(errs, jobErrs...)
		if job.ID != "" {
			jobs = append(jobs, job)
		}
	}
	if len(jobs) == 0 {
		errs = append(errs, invalidGitHubActionsConfig("jobs", "at least one job must be enabled"))
	}

	if err := errors.Join(errs...); err != nil {
		return GitHubActionsCIPlan{}, err
	}
	sort.SliceStable(jobs, func(i, j int) bool {
		return jobs[i].ID < jobs[j].ID
	})
	return GitHubActionsCIPlan{Name: name, Branches: branches, Env: env, Jobs: jobs}, nil
}

func normalizeGitHubActionsGoJob(job GitHubActionsGoJob) (GitHubActionsCIJob, []error) {
	var errs []error
	name := githubActionsDefault(job.Name, "Go")
	runsOn := githubActionsDefault(job.RunsOn, DefaultGitHubActionsRunner)
	versions, versionErrs := normalizeGitHubActionsList(job.Versions, "go.versions", DefaultGitHubActionsGoVersion, safeGitHubActionsVersion)
	errs = append(errs, versionErrs...)
	workDir, err := normalizeGitHubActionsWorkingDir(job.WorkingDir, "go.working_dir")
	if err != nil {
		errs = append(errs, err)
	}
	env, err := normalizeGitHubActionsEnv(job.Env, "go.env")
	if err != nil {
		errs = append(errs, err)
	}
	deps, depErrs := normalizeGitHubActionsPaths(job.CacheDependencyFiles, "go.cache_dependency_files", []string{"go.sum"})
	errs = append(errs, depErrs...)
	commands, commandErrs := normalizeGitHubActionsCommands([]GitHubActionsCommand{
		{Name: "Test", Run: defaultGitHubActionsCommand(job.TestCommand, []string{"go test ./..."})},
		{Name: "Build", Run: defaultGitHubActionsCommand(job.BuildCommand, []string{"go build ./..."})},
	}, "go.commands")
	errs = append(errs, commandErrs...)

	if !safeGitHubActionsRunner(runsOn) {
		errs = append(errs, invalidGitHubActionsConfig("go.runs_on", "must be a safe runner label"))
	}
	if hasControlRune(name) || strings.TrimSpace(name) == "" {
		errs = append(errs, invalidGitHubActionsConfig("go.name", "must be non-empty without control characters"))
	}

	return GitHubActionsCIJob{
		ID:         "go",
		Name:       name,
		RunsOn:     runsOn,
		WorkingDir: workDir,
		Matrix:     map[string][]string{"go-version": versions},
		Cache: GitHubActionsCache{
			Path:            []string{"~/.cache/go-build", "~/go/pkg/mod"},
			Key:             "${{ runner.os }}-go-${{ matrix.go-version }}-${{ hashFiles('" + strings.Join(deps, "', '") + "') }}",
			RestoreKey:      "${{ runner.os }}-go-${{ matrix.go-version }}-",
			DependencyFiles: deps,
		},
		Commands: commands,
		Env:      env,
	}, errs
}

func normalizeGitHubActionsRustJob(job GitHubActionsRustJob) (GitHubActionsCIJob, []error) {
	var errs []error
	name := githubActionsDefault(job.Name, "Rust")
	runsOn := githubActionsDefault(job.RunsOn, DefaultGitHubActionsRunner)
	toolchains, toolchainErrs := normalizeGitHubActionsList(job.Toolchains, "rust.toolchains", DefaultGitHubActionsRustToolchain, safeGitHubActionsVersion)
	errs = append(errs, toolchainErrs...)
	targets, targetErrs := normalizeGitHubActionsList(job.Targets, "rust.targets", DefaultGitHubActionsRustTarget, safeGitHubActionsRustTarget)
	errs = append(errs, targetErrs...)
	workDir, err := normalizeGitHubActionsWorkingDir(job.WorkingDir, "rust.working_dir")
	if err != nil {
		errs = append(errs, err)
	}
	env, err := normalizeGitHubActionsEnv(job.Env, "rust.env")
	if err != nil {
		errs = append(errs, err)
	}
	deps, depErrs := normalizeGitHubActionsPaths(job.CacheDependencyFiles, "rust.cache_dependency_files", []string{"Cargo.lock"})
	errs = append(errs, depErrs...)
	commands, commandErrs := normalizeGitHubActionsCommands([]GitHubActionsCommand{
		{Name: "Test", Run: defaultGitHubActionsCommand(job.TestCommand, []string{"cargo test --locked --target ${{ matrix.rust-target }}"})},
		{Name: "Build", Run: defaultGitHubActionsCommand(job.BuildCommand, []string{"cargo build --locked --target ${{ matrix.rust-target }}"})},
	}, "rust.commands")
	errs = append(errs, commandErrs...)

	if !safeGitHubActionsRunner(runsOn) {
		errs = append(errs, invalidGitHubActionsConfig("rust.runs_on", "must be a safe runner label"))
	}
	if hasControlRune(name) || strings.TrimSpace(name) == "" {
		errs = append(errs, invalidGitHubActionsConfig("rust.name", "must be non-empty without control characters"))
	}

	return GitHubActionsCIJob{
		ID:         "rust",
		Name:       name,
		RunsOn:     runsOn,
		WorkingDir: workDir,
		Matrix: map[string][]string{
			"rust-target":    targets,
			"rust-toolchain": toolchains,
		},
		Cache: GitHubActionsCache{
			Path:            []string{"~/.cargo/bin", "~/.cargo/git", "~/.cargo/registry", "target"},
			Key:             "${{ runner.os }}-rust-${{ matrix.rust-toolchain }}-${{ matrix.rust-target }}-${{ hashFiles('" + strings.Join(deps, "', '") + "') }}",
			RestoreKey:      "${{ runner.os }}-rust-${{ matrix.rust-toolchain }}-${{ matrix.rust-target }}-",
			DependencyFiles: deps,
		},
		Commands: commands,
		Env:      env,
	}, errs
}

func normalizeGitHubActionsEnv(specs []EnvSpec, field string) ([]GitHubActionsEnvBinding, error) {
	normalized, err := normalizeEnvSpecs(specs, field)
	if err != nil {
		return nil, fmt.Errorf("%w: %s: %v", ErrInvalidGitHubActionsConfig, field, err)
	}

	out := make([]GitHubActionsEnvBinding, 0, len(normalized))
	for _, spec := range normalized {
		binding := GitHubActionsEnvBinding{
			Name:      spec.Name,
			Value:     spec.Value,
			Secret:    spec.isSecret,
			SecretRef: spec.SecretRef,
		}
		if binding.Secret {
			if !safeGitHubActionsSecretName(binding.SecretRef.Name) {
				return nil, invalidGitHubActionsConfig(field+"."+binding.Name, "secret ref name must be a GitHub Actions secret name")
			}
			binding.Value = "${{ secrets." + binding.SecretRef.Name + " }}"
			binding.RedactedValue = DefaultEnvSecretMask
		} else {
			binding.RedactedValue = binding.Value
		}
		out = append(out, binding)
	}
	return out, nil
}

func normalizeGitHubActionsList(values []string, field, fallback string, valid func(string) bool) ([]string, []error) {
	if len(values) == 0 {
		values = []string{fallback}
	}
	out := make([]string, 0, len(values))
	seen := make(map[string]struct{}, len(values))
	var errs []error
	for i, value := range values {
		value = strings.TrimSpace(value)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if value == "" {
			errs = append(errs, invalidGitHubActionsConfig(itemField, "value is required"))
			continue
		}
		if !valid(value) {
			errs = append(errs, invalidGitHubActionsConfig(itemField, "contains unsafe characters"))
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out, errs
}

func normalizeGitHubActionsPaths(values []string, field string, fallback []string) ([]string, []error) {
	if len(values) == 0 {
		values = fallback
	}
	out := make([]string, 0, len(values))
	seen := make(map[string]struct{}, len(values))
	var errs []error
	for i, value := range values {
		value = strings.TrimSpace(value)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if !safeGitHubActionsPath(value) {
			errs = append(errs, invalidGitHubActionsConfig(itemField, "must be a safe relative path or glob"))
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out, errs
}

func normalizeGitHubActionsWorkingDir(value, field string) (string, error) {
	value = strings.TrimSpace(value)
	if value == "" {
		return "", nil
	}
	if !safeGitHubActionsPath(value) || strings.ContainsAny(value, "*?[]") {
		return "", invalidGitHubActionsConfig(field, "must be a safe relative path")
	}
	return path.Clean(value), nil
}

func normalizeGitHubActionsCommands(commands []GitHubActionsCommand, field string) ([]GitHubActionsCommand, []error) {
	out := make([]GitHubActionsCommand, 0, len(commands))
	var errs []error
	for i, command := range commands {
		command.Name = strings.TrimSpace(command.Name)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if command.Name == "" || hasControlRune(command.Name) {
			errs = append(errs, invalidGitHubActionsConfig(itemField+".name", "must be non-empty without control characters"))
			continue
		}
		run, runErrs := normalizeGitHubActionsCommand(command.Run, itemField+".run")
		errs = append(errs, runErrs...)
		if len(run) == 0 {
			continue
		}
		command.Run = run
		out = append(out, command)
	}
	return out, errs
}

func normalizeGitHubActionsCommand(command []string, field string) ([]string, []error) {
	var errs []error
	out := make([]string, 0, len(command))
	for i, part := range command {
		part = strings.TrimSpace(part)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if part == "" {
			errs = append(errs, invalidGitHubActionsConfig(itemField, "value is required"))
			continue
		}
		if hasControlRune(part) {
			errs = append(errs, invalidGitHubActionsConfig(itemField, "cannot contain control characters"))
			continue
		}
		out = append(out, part)
	}
	if len(out) == 0 {
		errs = append(errs, invalidGitHubActionsConfig(field, "at least one command is required"))
	}
	return out, errs
}

func defaultGitHubActionsCommand(value, fallback []string) []string {
	if len(value) == 0 {
		return append([]string(nil), fallback...)
	}
	return append([]string(nil), value...)
}

func githubActionsDefault(value, fallback string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return fallback
	}
	return value
}

func writeGitHubActionsMatrix(b *strings.Builder, indent int, matrix map[string][]string) {
	for _, key := range sortedMatrixKeys(matrix) {
		writeList(b, indent, key, matrix[key])
	}
}

func writeGitHubActionsEnv(b *strings.Builder, indent int, env []GitHubActionsEnvBinding, redacted bool) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString("env:\n")
	for _, binding := range env {
		b.WriteString(strings.Repeat(" ", indent+2))
		b.WriteString(binding.Name)
		b.WriteString(": ")
		value := binding.Value
		if redacted {
			value = binding.RedactedValue
		}
		b.WriteString(quoteYAML(value))
		b.WriteByte('\n')
	}
}

func writeGitHubActionsStepUses(b *strings.Builder, name, uses string, with map[string]string) {
	b.WriteString("      - name: ")
	b.WriteString(quoteYAML(name))
	b.WriteByte('\n')
	b.WriteString("        uses: ")
	b.WriteString(quoteYAML(uses))
	b.WriteByte('\n')
	if len(with) == 0 {
		return
	}
	b.WriteString("        with:\n")
	for _, key := range sortedMapKeys(with) {
		b.WriteString("          ")
		b.WriteString(key)
		b.WriteString(": ")
		b.WriteString(quoteYAML(with[key]))
		b.WriteByte('\n')
	}
}

func writeGitHubActionsCacheStep(b *strings.Builder, cache GitHubActionsCache) {
	b.WriteString("      - name: \"Cache dependencies\"\n")
	b.WriteString("        uses: \"actions/cache@v4\"\n")
	b.WriteString("        with:\n")
	b.WriteString("          path: |\n")
	for _, value := range cache.Path {
		b.WriteString("            ")
		b.WriteString(value)
		b.WriteByte('\n')
	}
	b.WriteString("          key: ")
	b.WriteString(quoteYAML(cache.Key))
	b.WriteByte('\n')
	b.WriteString("          restore-keys: ")
	b.WriteString(quoteYAML(cache.RestoreKey))
	b.WriteByte('\n')
}

func writeGitHubActionsRunStep(b *strings.Builder, command GitHubActionsCommand) {
	b.WriteString("      - name: ")
	b.WriteString(quoteYAML(command.Name))
	b.WriteByte('\n')
	b.WriteString("        run: |\n")
	for _, line := range command.Run {
		b.WriteString("          ")
		b.WriteString(line)
		b.WriteByte('\n')
	}
}

func writeGitHubActionsSummaryEnv(b *strings.Builder, env []GitHubActionsEnvBinding) {
	b.WriteString("| Name | Value |\n")
	b.WriteString("| --- | --- |\n")
	for _, binding := range env {
		releaseWriteMarkdownRow(b, binding.Name, binding.RedactedValue)
	}
}

func sortedMatrixKeys(matrix map[string][]string) []string {
	keys := make([]string, 0, len(matrix))
	for key := range matrix {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func summaryValue(value, fallback string) string {
	if strings.TrimSpace(value) == "" {
		return fallback
	}
	return value
}

func safeGitHubActionsBranch(value string) bool {
	return safeGitHubActionsToken(value)
}

func safeGitHubActionsVersion(value string) bool {
	return safeGitHubActionsToken(value)
}

func safeGitHubActionsRustTarget(value string) bool {
	return safeGitHubActionsToken(value)
}

func safeGitHubActionsRunner(value string) bool {
	return safeGitHubActionsToken(value)
}

func safeGitHubActionsToken(value string) bool {
	if value == "" || strings.TrimSpace(value) != value || hasControlRune(value) {
		return false
	}
	for _, r := range value {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r >= '0' && r <= '9':
		case strings.ContainsRune("._/@+-", r):
		default:
			return false
		}
	}
	return true
}

func safeGitHubActionsSecretName(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	for i, r := range value {
		ok := r == '_' || (r >= 'A' && r <= 'Z') || (i > 0 && r >= '0' && r <= '9')
		if !ok {
			return false
		}
	}
	return true
}

func safeGitHubActionsPath(value string) bool {
	if value == "" || strings.TrimSpace(value) != value || strings.Contains(value, "\\") || hasControlRune(value) {
		return false
	}
	if strings.HasPrefix(value, "/") || strings.ContainsAny(value, "\"'`$;&|<>") {
		return false
	}
	for _, segment := range strings.Split(value, "/") {
		if segment == ".." {
			return false
		}
	}
	return path.Clean(value) != "."
}

func invalidGitHubActionsConfig(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidGitHubActionsConfig, field, detail)
}
