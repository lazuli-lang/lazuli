package deploy

import (
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode"
)

const (
	FlyDefaultRegion      = "iad"
	FlyDefaultPort        = 8080
	FlyDefaultProcessName = "app"
	FlyDefaultCPUKind     = "shared"
	FlyDefaultCPUs        = 1
	FlyDefaultMemoryMB    = 256

	FlyMinCPUs     = 1
	FlyMaxCPUs     = 64
	FlyMinMemoryMB = 128
	FlyMaxMemoryMB = 262144

	FlyHealthProtocolHTTP  = "http"
	FlyHealthProtocolHTTPS = "https"
	FlyDefaultHealthMethod = "GET"
	FlyDefaultHealthPath   = "/healthz"
	FlyDefaultHealthStatus = 200
	FlyDefaultHealthEvery  = 10 * time.Second
	FlyDefaultHealthWait   = 2 * time.Second
	FlyDefaultHealthGrace  = 5 * time.Second
)

// ErrInvalidFlyConfig reports invalid Fly.io descriptor input.
var ErrInvalidFlyConfig = errors.New("lazuli/deploy: invalid fly config")

// FlyAppConfig describes a Fly.io app deployment without executing flyctl or
// provider APIs.
type FlyAppConfig struct {
	App           string
	PrimaryRegion string
	Image         string
	Port          int
	Env           []EnvSpec
	Resources     FlyResources
	Processes     []FlyProcessGroup
	HealthChecks  []FlyHealthCheck
}

// FlyResources describes Fly Machine VM resources.
type FlyResources struct {
	CPUKind  string
	CPUs     int
	MemoryMB int
}

// FlyProcessGroup describes one Fly process group and its command.
type FlyProcessGroup struct {
	Name      string
	Command   []string
	Count     int
	Resources FlyResources
}

// FlyHealthCheck describes one Fly HTTP health check. It is metadata only; the
// helpers never execute checks.
type FlyHealthCheck struct {
	Name           string
	Protocol       string
	Method         string
	Path           string
	Port           int
	ExpectedStatus int
	Interval       time.Duration
	Timeout        time.Duration
	GracePeriod    time.Duration
}

// FlyDeployPlan is a normalized dry-run Fly.io deploy descriptor.
type FlyDeployPlan struct {
	App           string
	PrimaryRegion string
	Image         string
	Port          int
	Env           []FlyEnvBinding
	Secrets       []FlySecretBinding
	Resources     FlyResources
	Processes     []FlyProcessGroupPlan
	HealthChecks  []FlyHealthCheck
}

// FlyEnvBinding is a literal environment binding planned for fly.toml.
type FlyEnvBinding struct {
	Name          string
	Value         string
	RedactedValue string
}

// FlySecretBinding records a secret-backed environment variable that must be
// supplied by a Fly secrets adapter outside fly.toml rendering.
type FlySecretBinding struct {
	Name      string
	SecretRef SecretRef
}

// FlyProcessGroupPlan is normalized process group metadata.
type FlyProcessGroupPlan struct {
	Name      string
	Command   []string
	Rendered  string
	Count     int
	Resources FlyResources
}

// FlySummary is a deterministic, log-safe view of a Fly deploy plan.
type FlySummary struct {
	App           string
	PrimaryRegion string
	Image         string
	Port          int
	Env           []FlyEnvBinding
	Secrets       []FlySecretBinding
	Resources     FlyResources
	Processes     []FlyProcessSummary
	HealthChecks  []FlyHealthCheckSummary
}

// FlyProcessSummary is process metadata without command arguments.
type FlyProcessSummary struct {
	Name      string
	Count     int
	Resources FlyResources
}

// FlyHealthCheckSummary is health check metadata suitable for logs.
type FlyHealthCheckSummary struct {
	Name           string
	Protocol       string
	Method         string
	Path           string
	Port           int
	ExpectedStatus int
	Interval       time.Duration
	Timeout        time.Duration
	GracePeriod    time.Duration
}

// Normalize returns a trimmed config with defaults applied.
func (c FlyAppConfig) Normalize() FlyAppConfig {
	return normalizeFlyAppConfigFields(c)
}

// Validate checks whether the config can produce a deploy plan.
func (c FlyAppConfig) Validate() error {
	return ValidateFlyAppConfig(c)
}

// Plan returns a normalized dry-run Fly.io deploy descriptor.
func (c FlyAppConfig) Plan() (FlyDeployPlan, error) {
	return PlanFlyApp(c)
}

// RenderTOML renders the normalized plan's fly.toml.
func (p FlyDeployPlan) RenderTOML() (string, error) {
	return RenderFlyTOML(p)
}

// RedactedSummary returns plan metadata with secret-bearing values redacted.
func (p FlyDeployPlan) RedactedSummary() FlySummary {
	return RedactFlyDeployPlan(p)
}

// NormalizeFlyAppConfig trims input and applies stable defaults.
func NormalizeFlyAppConfig(config FlyAppConfig) FlyAppConfig {
	return config.Normalize()
}

// ValidateFlyAppConfig checks Fly.io descriptor input.
func ValidateFlyAppConfig(config FlyAppConfig) error {
	_, err := PlanFlyApp(config)
	return err
}

// PlanFlyApp returns a normalized deploy descriptor. It does not call flyctl or
// Fly.io APIs.
func PlanFlyApp(config FlyAppConfig) (FlyDeployPlan, error) {
	normalized, err := normalizeFlyAppConfig(config)
	if err != nil {
		return FlyDeployPlan{}, err
	}

	env, secrets, err := normalizeFlyEnvBindings(normalized.Env)
	if err != nil {
		return FlyDeployPlan{}, err
	}
	processes, err := normalizeFlyProcesses(normalized.Processes, normalized.Resources)
	if err != nil {
		return FlyDeployPlan{}, err
	}
	checks, err := normalizeFlyHealthChecks(normalized.HealthChecks, normalized.Port)
	if err != nil {
		return FlyDeployPlan{}, err
	}

	plan := FlyDeployPlan{
		App:           normalized.App,
		PrimaryRegion: normalized.PrimaryRegion,
		Image:         normalized.Image,
		Port:          normalized.Port,
		Env:           env,
		Secrets:       secrets,
		Resources:     normalized.Resources,
		Processes:     processes,
		HealthChecks:  checks,
	}
	return plan, ValidateFlyDeployPlan(plan)
}

// ValidateFlyDeployPlan validates a normalized Fly.io deploy plan.
func ValidateFlyDeployPlan(plan FlyDeployPlan) error {
	var errs []error
	if !validFlyAppName(plan.App) {
		errs = append(errs, invalidFly("app", fmt.Sprintf("invalid app name %q", plan.App)))
	}
	if !validFlyRegion(plan.PrimaryRegion) {
		errs = append(errs, invalidFly("primary_region", fmt.Sprintf("invalid region %q", plan.PrimaryRegion)))
	}
	if !validFlyImage(plan.Image) {
		errs = append(errs, invalidFly("image", "must be a non-empty image reference without whitespace or control characters"))
	}
	if plan.Port < 1 || plan.Port > 65535 {
		errs = append(errs, invalidFly("port", "must be between 1 and 65535"))
	}
	errs = append(errs, validateFlyResources(plan.Resources, "resources")...)
	if _, _, err := normalizeFlyEnvBindings(flyEnvSpecs(plan.Env, plan.Secrets)); err != nil {
		errs = append(errs, invalidFly("env", err.Error()))
	}
	if len(plan.Processes) == 0 {
		errs = append(errs, invalidFly("processes", "at least one process is required"))
	}
	if _, err := normalizeFlyProcesses(flyProcessConfigs(plan.Processes), plan.Resources); err != nil {
		errs = append(errs, invalidFly("processes", err.Error()))
	}
	if _, err := normalizeFlyHealthChecks(plan.HealthChecks, plan.Port); err != nil {
		errs = append(errs, invalidFly("health_checks", err.Error()))
	}
	return errors.Join(errs...)
}

// RenderFlyTOML renders a deterministic fly.toml subset from a normalized plan.
func RenderFlyTOML(plan FlyDeployPlan) (string, error) {
	if err := ValidateFlyDeployPlan(plan); err != nil {
		return "", err
	}

	var b strings.Builder
	writeFlyTOMLScalar(&b, "app", plan.App)
	writeFlyTOMLScalar(&b, "primary_region", plan.PrimaryRegion)
	b.WriteByte('\n')

	b.WriteString("[build]\n")
	writeFlyTOMLScalar(&b, "image", plan.Image)
	b.WriteByte('\n')

	if len(plan.Env) > 0 {
		b.WriteString("[env]\n")
		for _, env := range plan.Env {
			writeFlyTOMLKeyValue(&b, env.Name, env.Value)
		}
		b.WriteByte('\n')
	}

	b.WriteString("[processes]\n")
	for _, process := range plan.Processes {
		writeFlyTOMLKeyValue(&b, process.Name, process.Rendered)
	}
	b.WriteByte('\n')

	b.WriteString("[http_service]\n")
	b.WriteString("internal_port = ")
	b.WriteString(strconv.Itoa(plan.Port))
	b.WriteByte('\n')
	b.WriteString("force_https = true\n")
	b.WriteString("auto_stop_machines = \"stop\"\n")
	b.WriteString("auto_start_machines = true\n")
	b.WriteString("min_machines_running = 0\n")
	writeFlyStringArray(&b, "processes", []string{plan.Processes[0].Name})
	if len(plan.HealthChecks) > 0 {
		for _, check := range plan.HealthChecks {
			b.WriteString("\n  [[http_service.checks]]\n")
			writeFlyTOMLIndentedScalar(&b, 2, "name", check.Name)
			writeFlyTOMLIndentedScalar(&b, 2, "protocol", check.Protocol)
			writeFlyTOMLIndentedScalar(&b, 2, "method", check.Method)
			writeFlyTOMLIndentedScalar(&b, 2, "path", check.Path)
			b.WriteString("  port = ")
			b.WriteString(strconv.Itoa(check.Port))
			b.WriteByte('\n')
			b.WriteString("  expected_status = ")
			b.WriteString(strconv.Itoa(check.ExpectedStatus))
			b.WriteByte('\n')
			writeFlyTOMLIndentedScalar(&b, 2, "interval", renderFlyDuration(check.Interval))
			writeFlyTOMLIndentedScalar(&b, 2, "timeout", renderFlyDuration(check.Timeout))
			writeFlyTOMLIndentedScalar(&b, 2, "grace_period", renderFlyDuration(check.GracePeriod))
		}
	}
	b.WriteByte('\n')

	for _, process := range plan.Processes {
		b.WriteString("\n[[vm]]\n")
		writeFlyTOMLScalar(&b, "memory", strconv.Itoa(process.Resources.MemoryMB)+"mb")
		writeFlyTOMLScalar(&b, "cpu_kind", process.Resources.CPUKind)
		b.WriteString("cpus = ")
		b.WriteString(strconv.Itoa(process.Resources.CPUs))
		b.WriteByte('\n')
		writeFlyStringArray(&b, "processes", []string{process.Name})
	}

	return b.String(), nil
}

// RedactFlyDeployPlan returns a stable, log-safe deploy summary.
func RedactFlyDeployPlan(plan FlyDeployPlan) FlySummary {
	env := make([]FlyEnvBinding, 0, len(plan.Env))
	for _, binding := range plan.Env {
		env = append(env, FlyEnvBinding{
			Name:          binding.Name,
			Value:         binding.RedactedValue,
			RedactedValue: binding.RedactedValue,
		})
	}
	secrets := append([]FlySecretBinding(nil), plan.Secrets...)
	processes := make([]FlyProcessSummary, 0, len(plan.Processes))
	for _, process := range plan.Processes {
		processes = append(processes, FlyProcessSummary{
			Name:      process.Name,
			Count:     process.Count,
			Resources: process.Resources,
		})
	}
	checks := make([]FlyHealthCheckSummary, 0, len(plan.HealthChecks))
	for _, check := range plan.HealthChecks {
		checks = append(checks, FlyHealthCheckSummary{
			Name:           check.Name,
			Protocol:       check.Protocol,
			Method:         check.Method,
			Path:           check.Path,
			Port:           check.Port,
			ExpectedStatus: check.ExpectedStatus,
			Interval:       check.Interval,
			Timeout:        check.Timeout,
			GracePeriod:    check.GracePeriod,
		})
	}
	return FlySummary{
		App:           plan.App,
		PrimaryRegion: plan.PrimaryRegion,
		Image:         redactFlyImage(plan.Image),
		Port:          plan.Port,
		Env:           env,
		Secrets:       secrets,
		Resources:     plan.Resources,
		Processes:     processes,
		HealthChecks:  checks,
	}
}

func normalizeFlyAppConfig(config FlyAppConfig) (FlyAppConfig, error) {
	config = normalizeFlyAppConfigFields(config)
	var errs []error
	if !validFlyAppName(config.App) {
		errs = append(errs, invalidFly("app", fmt.Sprintf("invalid app name %q", config.App)))
	}
	if !validFlyRegion(config.PrimaryRegion) {
		errs = append(errs, invalidFly("primary_region", fmt.Sprintf("invalid region %q", config.PrimaryRegion)))
	}
	if !validFlyImage(config.Image) {
		errs = append(errs, invalidFly("image", "must be a non-empty image reference without whitespace or control characters"))
	}
	if config.Port < 1 || config.Port > 65535 {
		errs = append(errs, invalidFly("port", "must be between 1 and 65535"))
	}
	errs = append(errs, validateFlyResources(config.Resources, "resources")...)
	if _, err := normalizeEnvSpecs(config.Env, "env"); err != nil {
		errs = append(errs, invalidFly("env", err.Error()))
	}
	if _, err := normalizeFlyProcesses(config.Processes, config.Resources); err != nil {
		errs = append(errs, err)
	}
	if _, err := normalizeFlyHealthChecks(config.HealthChecks, config.Port); err != nil {
		errs = append(errs, err)
	}
	if err := errors.Join(errs...); err != nil {
		return FlyAppConfig{}, err
	}
	return config, nil
}

func normalizeFlyAppConfigFields(config FlyAppConfig) FlyAppConfig {
	config.App = strings.ToLower(strings.TrimSpace(config.App))
	config.PrimaryRegion = strings.ToLower(strings.TrimSpace(config.PrimaryRegion))
	config.Image = strings.TrimSpace(config.Image)
	if config.PrimaryRegion == "" {
		config.PrimaryRegion = FlyDefaultRegion
	}
	if config.Port == 0 {
		config.Port = FlyDefaultPort
	}
	config.Resources = normalizeFlyResources(config.Resources)
	return config
}

func normalizeFlyResources(resources FlyResources) FlyResources {
	resources.CPUKind = strings.ToLower(strings.TrimSpace(resources.CPUKind))
	if resources.CPUKind == "" {
		resources.CPUKind = FlyDefaultCPUKind
	}
	if resources.CPUs == 0 {
		resources.CPUs = FlyDefaultCPUs
	}
	if resources.MemoryMB == 0 {
		resources.MemoryMB = FlyDefaultMemoryMB
	}
	return resources
}

func normalizeFlyEnvBindings(specs []EnvSpec) ([]FlyEnvBinding, []FlySecretBinding, error) {
	normalized, err := normalizeEnvSpecs(specs, "env")
	if err != nil {
		return nil, nil, err
	}
	env := make([]FlyEnvBinding, 0, len(normalized))
	secrets := make([]FlySecretBinding, 0, len(normalized))
	for _, spec := range normalized {
		if spec.isSecret {
			secrets = append(secrets, FlySecretBinding{Name: spec.Name, SecretRef: spec.SecretRef})
			continue
		}
		env = append(env, FlyEnvBinding{
			Name:          spec.Name,
			Value:         spec.Value,
			RedactedValue: RedactCloudRunEnvValue(spec.Name, spec.Value, false),
		})
	}
	return env, secrets, nil
}

func normalizeFlyProcesses(processes []FlyProcessGroup, defaultResources FlyResources) ([]FlyProcessGroupPlan, error) {
	if len(processes) == 0 {
		processes = []FlyProcessGroup{{Name: FlyDefaultProcessName, Command: []string{"/app/" + DefaultBinaryName}}}
	}
	out := make([]FlyProcessGroupPlan, 0, len(processes))
	seen := make(map[string]int, len(processes))
	var errs []error
	for i, process := range processes {
		field := fmt.Sprintf("processes[%d]", i)
		process.Name = strings.TrimSpace(process.Name)
		if process.Name == "" {
			process.Name = FlyDefaultProcessName
		}
		if !validFlyProcessName(process.Name) {
			errs = append(errs, invalidFly(field+".name", fmt.Sprintf("invalid process name %q", process.Name)))
			continue
		}
		if first, ok := seen[process.Name]; ok {
			errs = append(errs, invalidFly(field+".name", fmt.Sprintf("duplicates processes[%d].name %q", first, process.Name)))
			continue
		}
		seen[process.Name] = i
		command, commandErrs := normalizeProcessCommand(process.Command, field+".command")
		errs = append(errs, commandErrs...)
		resources := process.Resources
		if resources == (FlyResources{}) {
			resources = defaultResources
		}
		resources = normalizeFlyResources(resources)
		errs = append(errs, validateFlyResources(resources, field+".resources")...)
		count := process.Count
		if count == 0 {
			count = 1
		}
		if count < 0 {
			errs = append(errs, invalidFly(field+".count", "must be positive"))
		}
		out = append(out, FlyProcessGroupPlan{
			Name:      process.Name,
			Command:   command,
			Rendered:  renderShellCommand(command),
			Count:     count,
			Resources: resources,
		})
	}
	sort.SliceStable(out, func(i, j int) bool {
		return out[i].Name < out[j].Name
	})
	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return out, nil
}

func normalizeFlyHealthChecks(checks []FlyHealthCheck, port int) ([]FlyHealthCheck, error) {
	if len(checks) == 0 {
		checks = []FlyHealthCheck{{Name: "health", Path: FlyDefaultHealthPath}}
	}
	out := make([]FlyHealthCheck, 0, len(checks))
	seen := make(map[string]int, len(checks))
	var errs []error
	for i, check := range checks {
		field := fmt.Sprintf("health_checks[%d]", i)
		check.Name = strings.TrimSpace(check.Name)
		if check.Name == "" {
			check.Name = "health"
		}
		if !validFlyCheckName(check.Name) {
			errs = append(errs, invalidFly(field+".name", fmt.Sprintf("invalid check name %q", check.Name)))
			continue
		}
		if first, ok := seen[check.Name]; ok {
			errs = append(errs, invalidFly(field+".name", fmt.Sprintf("duplicates health_checks[%d].name %q", first, check.Name)))
			continue
		}
		seen[check.Name] = i
		check.Protocol = strings.ToLower(strings.TrimSpace(check.Protocol))
		if check.Protocol == "" {
			check.Protocol = FlyHealthProtocolHTTP
		}
		if check.Protocol != FlyHealthProtocolHTTP && check.Protocol != FlyHealthProtocolHTTPS {
			errs = append(errs, invalidFly(field+".protocol", "must be http or https"))
		}
		check.Method = strings.ToUpper(strings.TrimSpace(check.Method))
		if check.Method == "" {
			check.Method = FlyDefaultHealthMethod
		}
		if !validFlyHTTPMethod(check.Method) {
			errs = append(errs, invalidFly(field+".method", fmt.Sprintf("invalid method %q", check.Method)))
		}
		check.Path = strings.TrimSpace(check.Path)
		if check.Path == "" {
			check.Path = FlyDefaultHealthPath
		}
		if !validFlyHealthPath(check.Path) {
			errs = append(errs, invalidFly(field+".path", "must be an absolute path without control characters"))
		}
		if check.Port == 0 {
			check.Port = port
		}
		if check.Port < 1 || check.Port > 65535 {
			errs = append(errs, invalidFly(field+".port", "must be between 1 and 65535"))
		}
		if check.ExpectedStatus == 0 {
			check.ExpectedStatus = FlyDefaultHealthStatus
		}
		if check.ExpectedStatus < 100 || check.ExpectedStatus > 599 {
			errs = append(errs, invalidFly(field+".expected_status", "must be an HTTP status code"))
		}
		check.Interval = normalizeFlyDuration(check.Interval, FlyDefaultHealthEvery)
		check.Timeout = normalizeFlyDuration(check.Timeout, FlyDefaultHealthWait)
		check.GracePeriod = normalizeFlyDuration(check.GracePeriod, FlyDefaultHealthGrace)
		if check.Interval < 0 {
			errs = append(errs, invalidFly(field+".interval", "must be positive"))
		}
		if check.Timeout < 0 {
			errs = append(errs, invalidFly(field+".timeout", "must be positive"))
		}
		if check.GracePeriod < 0 {
			errs = append(errs, invalidFly(field+".grace_period", "must be positive"))
		}
		out = append(out, check)
	}
	sort.SliceStable(out, func(i, j int) bool {
		return out[i].Name < out[j].Name
	})
	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return out, nil
}

func validateFlyResources(resources FlyResources, field string) []error {
	var errs []error
	if resources.CPUKind != "shared" && resources.CPUKind != "performance" {
		errs = append(errs, invalidFly(field+".cpu_kind", "must be shared or performance"))
	}
	if resources.CPUs < FlyMinCPUs || resources.CPUs > FlyMaxCPUs {
		errs = append(errs, invalidFly(field+".cpus", fmt.Sprintf("must be between %d and %d", FlyMinCPUs, FlyMaxCPUs)))
	}
	if resources.MemoryMB < FlyMinMemoryMB || resources.MemoryMB > FlyMaxMemoryMB {
		errs = append(errs, invalidFly(field+".memory_mb", fmt.Sprintf("must be between %d and %d", FlyMinMemoryMB, FlyMaxMemoryMB)))
	}
	return errs
}

func flyEnvSpecs(env []FlyEnvBinding, secrets []FlySecretBinding) []EnvSpec {
	specs := make([]EnvSpec, 0, len(env)+len(secrets))
	for _, binding := range env {
		specs = append(specs, EnvValue(binding.Name, binding.Value))
	}
	for _, binding := range secrets {
		specs = append(specs, SecretRefEnv(binding.Name, binding.SecretRef))
	}
	return specs
}

func flyProcessConfigs(processes []FlyProcessGroupPlan) []FlyProcessGroup {
	out := make([]FlyProcessGroup, 0, len(processes))
	for _, process := range processes {
		out = append(out, FlyProcessGroup{
			Name:      process.Name,
			Command:   process.Command,
			Count:     process.Count,
			Resources: process.Resources,
		})
	}
	return out
}

func validFlyAppName(value string) bool {
	if value == "" || len(value) > 63 {
		return false
	}
	return validFlySlug(value)
}

func validFlyRegion(value string) bool {
	if len(value) < 3 || len(value) > 16 {
		return false
	}
	for _, r := range value {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') {
			continue
		}
		return false
	}
	return true
}

func validFlyImage(value string) bool {
	if value == "" || strings.TrimSpace(value) != value || hasControlRune(value) {
		return false
	}
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return false
		}
	}
	return strings.ContainsAny(value, "/:")
}

func validFlyProcessName(value string) bool {
	if value == "" || len(value) > 32 {
		return false
	}
	for _, r := range value {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') || r == '-' || r == '_' {
			continue
		}
		return false
	}
	return true
}

func validFlyCheckName(value string) bool {
	return validFlyProcessName(value)
}

func validFlySlug(value string) bool {
	if value[0] < 'a' || value[0] > 'z' {
		return false
	}
	last := value[len(value)-1]
	if !((last >= 'a' && last <= 'z') || (last >= '0' && last <= '9')) {
		return false
	}
	for _, r := range value {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') || r == '-' {
			continue
		}
		return false
	}
	return true
}

func validFlyHTTPMethod(value string) bool {
	switch value {
	case "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS":
		return true
	default:
		return false
	}
}

func validFlyHealthPath(value string) bool {
	if value == "" || value[0] != '/' || hasControlRune(value) || strings.ContainsAny(value, " \t\r\n") {
		return false
	}
	return true
}

func normalizeFlyDuration(value, fallback time.Duration) time.Duration {
	if value == 0 {
		return fallback
	}
	return value
}

func renderFlyDuration(value time.Duration) string {
	return value.String()
}

func writeFlyTOMLScalar(b *strings.Builder, key, value string) {
	b.WriteString(key)
	b.WriteString(" = ")
	b.WriteString(strconv.Quote(value))
	b.WriteByte('\n')
}

func writeFlyTOMLIndentedScalar(b *strings.Builder, indent int, key, value string) {
	b.WriteString(strings.Repeat(" ", indent))
	writeFlyTOMLScalar(b, key, value)
}

func writeFlyTOMLKeyValue(b *strings.Builder, key, value string) {
	b.WriteString(strconv.Quote(key))
	b.WriteString(" = ")
	b.WriteString(strconv.Quote(value))
	b.WriteByte('\n')
}

func writeFlyStringArray(b *strings.Builder, key string, values []string) {
	b.WriteString(key)
	b.WriteString(" = [")
	for i, value := range values {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString(strconv.Quote(value))
	}
	b.WriteString("]\n")
}

func redactFlyImage(value string) string {
	if strings.Contains(value, "@") {
		return DefaultEnvSecretMask
	}
	return value
}

func invalidFly(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidFlyConfig, field, detail)
}
