package deploy

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strconv"
	"strings"
)

const (
	CloudRunIngressAll             = "all"
	CloudRunIngressInternal        = "internal"
	CloudRunIngressInternalAndLB   = "internal-and-cloud-load-balancing"
	CloudRunDefaultPort            = 8080
	CloudRunDefaultConcurrency     = 80
	CloudRunMinCPUMilli            = 80
	CloudRunMaxCPUMilli            = 8000
	CloudRunMinMemoryMiB           = 128
	CloudRunMaxMemoryMiB           = 32768
	CloudRunMinConcurrency         = 1
	CloudRunMaxConcurrency         = 1000
	cloudRunDefaultAPIVersion      = "serving.knative.dev/v1"
	cloudRunDefaultKind            = "Service"
	cloudRunUnauthenticatedIAMRole = "roles/run.invoker"
)

// ErrInvalidCloudRunConfig reports invalid Google Cloud Run descriptor input.
var ErrInvalidCloudRunConfig = errors.New("lazuli/deploy: invalid cloud run config")

// CloudRunServiceConfig describes a Cloud Run service deployment without
// executing provider APIs.
type CloudRunServiceConfig struct {
	Service              string
	Region               string
	Project              string
	Image                string
	Port                 int
	CPUMilli             int
	MemoryMiB            int
	Concurrency          int
	Env                  []EnvSpec
	Ingress              string
	AllowUnauthenticated bool
}

// CloudRunDeployPlan is a normalized dry-run deploy descriptor.
type CloudRunDeployPlan struct {
	Service              string
	Region               string
	Project              string
	Image                string
	Port                 int
	CPUMilli             int
	MemoryMiB            int
	Concurrency          int
	Env                  []CloudRunEnvBinding
	Ingress              string
	AllowUnauthenticated bool
	Manifest             CloudRunServiceManifest
	IAMBindings          []CloudRunIAMBinding
}

// CloudRunEnvBinding is a log-safe environment binding in a deploy plan.
type CloudRunEnvBinding struct {
	Name          string
	Value         string
	Secret        bool
	SecretRef     SecretRef
	RedactedValue string
}

// CloudRunServiceManifest is the deterministic manifest subset planned by
// these helpers.
type CloudRunServiceManifest struct {
	APIVersion  string
	Kind        string
	Name        string
	Region      string
	Project     string
	Image       string
	Port        int
	CPUMilli    int
	MemoryMiB   int
	Concurrency int
	Env         []CloudRunEnvBinding
	Annotations map[string]string
}

// CloudRunIAMBinding records IAM work an adapter may apply later.
type CloudRunIAMBinding struct {
	Role   string
	Member string
}

// Normalize returns a trimmed config with defaults applied.
func (c CloudRunServiceConfig) Normalize() CloudRunServiceConfig {
	return normalizeCloudRunServiceConfigFields(c)
}

// Validate checks whether the config can produce a deploy plan.
func (c CloudRunServiceConfig) Validate() error {
	return ValidateCloudRunServiceConfig(c)
}

// Plan returns a normalized dry-run Cloud Run deploy descriptor.
func (c CloudRunServiceConfig) Plan() (CloudRunDeployPlan, error) {
	return PlanCloudRunService(c)
}

// Validate checks the normalized deploy plan.
func (p CloudRunDeployPlan) Validate() error {
	return ValidateCloudRunDeployPlan(p)
}

// RenderManifest renders the plan's manifest as deterministic YAML.
func (p CloudRunDeployPlan) RenderManifest() (string, error) {
	return RenderCloudRunManifest(p.Manifest)
}

// NormalizeCloudRunServiceConfig trims input and applies stable defaults.
func NormalizeCloudRunServiceConfig(config CloudRunServiceConfig) CloudRunServiceConfig {
	return config.Normalize()
}

// ValidateCloudRunServiceConfig checks Cloud Run descriptor input.
func ValidateCloudRunServiceConfig(config CloudRunServiceConfig) error {
	_, err := PlanCloudRunService(config)
	return err
}

// PlanCloudRunService returns a normalized deploy descriptor. It does not call
// gcloud, Cloud Run APIs, or IAM APIs.
func PlanCloudRunService(config CloudRunServiceConfig) (CloudRunDeployPlan, error) {
	normalized, err := normalizeCloudRunServiceConfig(config)
	if err != nil {
		return CloudRunDeployPlan{}, err
	}

	env, err := normalizeCloudRunEnvBindings(normalized.Env)
	if err != nil {
		return CloudRunDeployPlan{}, err
	}

	annotations := map[string]string{
		"run.googleapis.com/ingress": normalized.Ingress,
	}
	manifest := CloudRunServiceManifest{
		APIVersion:  cloudRunDefaultAPIVersion,
		Kind:        cloudRunDefaultKind,
		Name:        normalized.Service,
		Region:      normalized.Region,
		Project:     normalized.Project,
		Image:       normalized.Image,
		Port:        normalized.Port,
		CPUMilli:    normalized.CPUMilli,
		MemoryMiB:   normalized.MemoryMiB,
		Concurrency: normalized.Concurrency,
		Env:         append([]CloudRunEnvBinding(nil), env...),
		Annotations: annotations,
	}

	var bindings []CloudRunIAMBinding
	if normalized.AllowUnauthenticated {
		bindings = append(bindings, CloudRunIAMBinding{
			Role:   cloudRunUnauthenticatedIAMRole,
			Member: "allUsers",
		})
	}

	plan := CloudRunDeployPlan{
		Service:              normalized.Service,
		Region:               normalized.Region,
		Project:              normalized.Project,
		Image:                normalized.Image,
		Port:                 normalized.Port,
		CPUMilli:             normalized.CPUMilli,
		MemoryMiB:            normalized.MemoryMiB,
		Concurrency:          normalized.Concurrency,
		Env:                  env,
		Ingress:              normalized.Ingress,
		AllowUnauthenticated: normalized.AllowUnauthenticated,
		Manifest:             manifest,
		IAMBindings:          bindings,
	}
	return plan, ValidateCloudRunDeployPlan(plan)
}

// ValidateCloudRunDeployPlan validates a normalized Cloud Run deploy plan.
func ValidateCloudRunDeployPlan(plan CloudRunDeployPlan) error {
	var errs []error
	if plan.Service == "" {
		errs = append(errs, invalidCloudRun("service", "value is required"))
	} else if !validCloudRunServiceName(plan.Service) {
		errs = append(errs, invalidCloudRun("service", fmt.Sprintf("invalid name %q", plan.Service)))
	}
	if !validCloudRunRegion(plan.Region) {
		errs = append(errs, invalidCloudRun("region", fmt.Sprintf("invalid region %q", plan.Region)))
	}
	if !validCloudRunProject(plan.Project) {
		errs = append(errs, invalidCloudRun("project", fmt.Sprintf("invalid project %q", plan.Project)))
	}
	if !validCloudRunImage(plan.Image) {
		errs = append(errs, invalidCloudRun("image", "must be a non-empty image reference without whitespace or control characters"))
	}
	errs = append(errs, validateCloudRunBounds(plan.Port, plan.CPUMilli, plan.MemoryMiB, plan.Concurrency)...)
	if !validCloudRunIngress(plan.Ingress) {
		errs = append(errs, invalidCloudRun("ingress", fmt.Sprintf("invalid value %q", plan.Ingress)))
	}

	if _, err := normalizeCloudRunEnvBindings(cloudRunEnvSpecs(plan.Env)); err != nil {
		errs = append(errs, invalidCloudRun("env", err.Error()))
	}
	if err := ValidateCloudRunManifest(plan.Manifest); err != nil {
		errs = append(errs, err)
	}
	for i, binding := range plan.IAMBindings {
		field := fmt.Sprintf("iam_bindings[%d]", i)
		if strings.TrimSpace(binding.Role) == "" {
			errs = append(errs, invalidCloudRun(field+".role", "value is required"))
		}
		if strings.TrimSpace(binding.Member) == "" {
			errs = append(errs, invalidCloudRun(field+".member", "value is required"))
		}
	}
	if err := errors.Join(errs...); err != nil {
		return err
	}
	return nil
}

// ValidateCloudRunManifest validates the manifest subset planned by this
// package.
func ValidateCloudRunManifest(manifest CloudRunServiceManifest) error {
	var errs []error
	if manifest.APIVersion != cloudRunDefaultAPIVersion {
		errs = append(errs, invalidCloudRun("manifest.api_version", fmt.Sprintf("must be %q", cloudRunDefaultAPIVersion)))
	}
	if manifest.Kind != cloudRunDefaultKind {
		errs = append(errs, invalidCloudRun("manifest.kind", fmt.Sprintf("must be %q", cloudRunDefaultKind)))
	}
	if !validCloudRunServiceName(manifest.Name) {
		errs = append(errs, invalidCloudRun("manifest.name", fmt.Sprintf("invalid name %q", manifest.Name)))
	}
	if !validCloudRunRegion(manifest.Region) {
		errs = append(errs, invalidCloudRun("manifest.region", fmt.Sprintf("invalid region %q", manifest.Region)))
	}
	if !validCloudRunProject(manifest.Project) {
		errs = append(errs, invalidCloudRun("manifest.project", fmt.Sprintf("invalid project %q", manifest.Project)))
	}
	if !validCloudRunImage(manifest.Image) {
		errs = append(errs, invalidCloudRun("manifest.image", "must be a non-empty image reference without whitespace or control characters"))
	}
	errs = append(errs, validateCloudRunBounds(manifest.Port, manifest.CPUMilli, manifest.MemoryMiB, manifest.Concurrency)...)
	if ingress := manifest.Annotations["run.googleapis.com/ingress"]; !validCloudRunIngress(ingress) {
		errs = append(errs, invalidCloudRun("manifest.annotations.run.googleapis.com/ingress", fmt.Sprintf("invalid value %q", ingress)))
	}
	if _, err := normalizeCloudRunEnvBindings(cloudRunEnvSpecs(manifest.Env)); err != nil {
		errs = append(errs, invalidCloudRun("manifest.env", err.Error()))
	}
	return errors.Join(errs...)
}

// RenderCloudRunManifest renders a deterministic Knative Service YAML subset.
func RenderCloudRunManifest(manifest CloudRunServiceManifest) (string, error) {
	if err := ValidateCloudRunManifest(manifest); err != nil {
		return "", err
	}

	var b strings.Builder
	writeScalar(&b, 0, "apiVersion", manifest.APIVersion)
	writeScalar(&b, 0, "kind", manifest.Kind)
	b.WriteString("metadata:\n")
	writeScalar(&b, 2, "name", manifest.Name)
	b.WriteString("  annotations:\n")
	for _, key := range sortedCloudRunAnnotationKeys(manifest.Annotations) {
		writeScalar(&b, 4, key, manifest.Annotations[key])
	}
	b.WriteString("spec:\n")
	b.WriteString("  template:\n")
	b.WriteString("    metadata:\n")
	b.WriteString("      annotations:\n")
	writeScalar(&b, 8, "autoscaling.knative.dev/maxScale", "100")
	writeScalar(&b, 8, "run.googleapis.com/cpu-throttling", "true")
	b.WriteString("    spec:\n")
	b.WriteString("      containerConcurrency: ")
	b.WriteString(strconv.Itoa(manifest.Concurrency))
	b.WriteByte('\n')
	b.WriteString("      containers:\n")
	b.WriteString("      - image: ")
	b.WriteString(quoteYAML(manifest.Image))
	b.WriteByte('\n')
	b.WriteString("        ports:\n")
	b.WriteString("        - containerPort: ")
	b.WriteString(strconv.Itoa(manifest.Port))
	b.WriteByte('\n')
	b.WriteString("        resources:\n")
	b.WriteString("          limits:\n")
	writeScalar(&b, 12, "cpu", renderCloudRunCPU(manifest.CPUMilli))
	writeScalar(&b, 12, "memory", strconv.Itoa(manifest.MemoryMiB)+"Mi")
	if len(manifest.Env) > 0 {
		b.WriteString("        env:\n")
		for _, env := range manifest.Env {
			b.WriteString("        - name: ")
			b.WriteString(quoteYAML(env.Name))
			b.WriteByte('\n')
			if env.Secret {
				b.WriteString("          valueFrom:\n")
				b.WriteString("            secretKeyRef:\n")
				writeScalar(&b, 14, "name", env.SecretRef.Name)
				version := string(env.SecretRef.Version)
				if version == "" {
					version = "latest"
				}
				writeScalar(&b, 14, "key", version)
			} else {
				writeScalar(&b, 10, "value", env.Value)
			}
		}
	}
	return b.String(), nil
}

// RedactCloudRunEnvValue returns log-safe metadata for environment values.
func RedactCloudRunEnvValue(name, value string, secret bool) string {
	if secret {
		return DefaultEnvSecretMask
	}
	name = strings.ToUpper(strings.TrimSpace(name))
	if strings.Contains(name, "URL") || strings.Contains(name, "URI") || looksLikeURL(value) {
		return redactCloudRunURL(value)
	}
	return value
}

func normalizeCloudRunServiceConfig(config CloudRunServiceConfig) (CloudRunServiceConfig, error) {
	config = normalizeCloudRunServiceConfigFields(config)

	var errs []error
	if config.Service == "" {
		errs = append(errs, invalidCloudRun("service", "value is required"))
	} else if !validCloudRunServiceName(config.Service) {
		errs = append(errs, invalidCloudRun("service", fmt.Sprintf("invalid name %q", config.Service)))
	}
	if !validCloudRunRegion(config.Region) {
		errs = append(errs, invalidCloudRun("region", fmt.Sprintf("invalid region %q", config.Region)))
	}
	if !validCloudRunProject(config.Project) {
		errs = append(errs, invalidCloudRun("project", fmt.Sprintf("invalid project %q", config.Project)))
	}
	if !validCloudRunImage(config.Image) {
		errs = append(errs, invalidCloudRun("image", "must be a non-empty image reference without whitespace or control characters"))
	}
	errs = append(errs, validateCloudRunBounds(config.Port, config.CPUMilli, config.MemoryMiB, config.Concurrency)...)
	if !validCloudRunIngress(config.Ingress) {
		errs = append(errs, invalidCloudRun("ingress", fmt.Sprintf("invalid value %q", config.Ingress)))
	}
	if _, err := normalizeEnvSpecs(config.Env, "env"); err != nil {
		errs = append(errs, invalidCloudRun("env", err.Error()))
	}
	if err := errors.Join(errs...); err != nil {
		return CloudRunServiceConfig{}, err
	}
	return config, nil
}

func normalizeCloudRunServiceConfigFields(config CloudRunServiceConfig) CloudRunServiceConfig {
	config.Service = strings.TrimSpace(config.Service)
	config.Region = strings.ToLower(strings.TrimSpace(config.Region))
	config.Project = strings.ToLower(strings.TrimSpace(config.Project))
	config.Image = strings.TrimSpace(config.Image)
	config.Ingress = strings.ToLower(strings.TrimSpace(config.Ingress))
	if config.Port == 0 {
		config.Port = CloudRunDefaultPort
	}
	if config.Concurrency == 0 {
		config.Concurrency = CloudRunDefaultConcurrency
	}
	if config.Ingress == "" {
		config.Ingress = CloudRunIngressAll
	}
	return config
}

func normalizeCloudRunEnvBindings(specs []EnvSpec) ([]CloudRunEnvBinding, error) {
	normalized, err := normalizeEnvSpecs(specs, "env")
	if err != nil {
		return nil, err
	}
	out := make([]CloudRunEnvBinding, 0, len(normalized))
	for _, spec := range normalized {
		binding := CloudRunEnvBinding{
			Name:      spec.Name,
			Value:     spec.Value,
			Secret:    spec.isSecret,
			SecretRef: spec.SecretRef,
		}
		binding.RedactedValue = RedactCloudRunEnvValue(binding.Name, binding.Value, binding.Secret)
		out = append(out, binding)
	}
	return out, nil
}

func cloudRunEnvSpecs(bindings []CloudRunEnvBinding) []EnvSpec {
	specs := make([]EnvSpec, 0, len(bindings))
	for _, binding := range bindings {
		specs = append(specs, EnvSpec{
			Name:      binding.Name,
			Value:     binding.Value,
			Secret:    binding.Secret,
			SecretRef: binding.SecretRef,
		})
	}
	return specs
}

func validateCloudRunBounds(port, cpuMilli, memoryMiB, concurrency int) []error {
	var errs []error
	if port < 1 || port > 65535 {
		errs = append(errs, invalidCloudRun("port", "must be between 1 and 65535"))
	}
	if cpuMilli < CloudRunMinCPUMilli || cpuMilli > CloudRunMaxCPUMilli {
		errs = append(errs, invalidCloudRun("cpu_milli", fmt.Sprintf("must be between %d and %d", CloudRunMinCPUMilli, CloudRunMaxCPUMilli)))
	}
	if memoryMiB < CloudRunMinMemoryMiB || memoryMiB > CloudRunMaxMemoryMiB {
		errs = append(errs, invalidCloudRun("memory_mib", fmt.Sprintf("must be between %d and %d", CloudRunMinMemoryMiB, CloudRunMaxMemoryMiB)))
	}
	if concurrency < CloudRunMinConcurrency || concurrency > CloudRunMaxConcurrency {
		errs = append(errs, invalidCloudRun("concurrency", fmt.Sprintf("must be between %d and %d", CloudRunMinConcurrency, CloudRunMaxConcurrency)))
	}
	return errs
}

func validCloudRunServiceName(value string) bool {
	if value == "" || len(value) > 63 {
		return false
	}
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

func validCloudRunRegion(value string) bool {
	if value == "" || len(value) > 64 {
		return false
	}
	for i, r := range value {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= '0' && r <= '9' && i > 0:
		case r == '-' && i > 0 && i < len(value)-1:
		default:
			return false
		}
	}
	return strings.Contains(value, "-")
}

func validCloudRunProject(value string) bool {
	if len(value) < 6 || len(value) > 30 {
		return false
	}
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

func validCloudRunImage(value string) bool {
	if value == "" || strings.TrimSpace(value) != value || hasControlRune(value) {
		return false
	}
	for _, r := range value {
		if r <= ' ' {
			return false
		}
	}
	return strings.ContainsAny(value, "/:")
}

func validCloudRunIngress(value string) bool {
	switch value {
	case CloudRunIngressAll, CloudRunIngressInternal, CloudRunIngressInternalAndLB:
		return true
	default:
		return false
	}
}

func renderCloudRunCPU(cpuMilli int) string {
	if cpuMilli%1000 == 0 {
		return strconv.Itoa(cpuMilli / 1000)
	}
	return strconv.FormatFloat(float64(cpuMilli)/1000, 'f', -1, 64)
}

func sortedCloudRunAnnotationKeys(values map[string]string) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func looksLikeURL(value string) bool {
	u, err := url.Parse(strings.TrimSpace(value))
	return err == nil && u.Scheme != "" && u.Host != ""
}

func redactCloudRunURL(value string) string {
	u, err := url.Parse(strings.TrimSpace(value))
	if err != nil || u.Scheme == "" || u.Host == "" {
		return DefaultEnvSecretMask
	}
	u.User = nil
	u.RawQuery = ""
	u.Fragment = ""
	if u.Path != "" && u.Path != "/" {
		parts := strings.Split(strings.TrimPrefix(u.Path, "/"), "/")
		u.Path = "/" + parts[0] + "/..."
	}
	return u.String()
}

func invalidCloudRun(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidCloudRunConfig, field, detail)
}
