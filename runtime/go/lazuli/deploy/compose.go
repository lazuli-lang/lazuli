// Package deploy provides small deployment manifest helpers for Lazuli
// applications.
package deploy

import (
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
)

const (
	defaultAppName         = "app"
	defaultPostgresName    = "postgres"
	defaultPostgresImage   = "postgres:16-alpine"
	defaultPostgresDB      = "lazuli"
	defaultPostgresUser    = "lazuli"
	defaultPostgresPass    = "lazuli"
	defaultPostgresDataDir = "/var/lib/postgresql/data"
	defaultPostgresPort    = 5432
	defaultRedisName       = "redis"
	defaultRedisImage      = "redis:7-alpine"
	defaultRedisDataDir    = "/data"
	defaultRedisPort       = 6379
	defaultRestartPolicy   = "unless-stopped"
)

var (
	// ErrInvalidComposeConfig reports an invalid Docker Compose document or
	// helper input.
	ErrInvalidComposeConfig = errors.New("lazuli/deploy: invalid compose config")
	// ErrInvalidEnv reports invalid environment variable names or duplicate
	// entries while rendering env files.
	ErrInvalidEnv = errors.New("lazuli/deploy: invalid env")
)

// ComposeFile is the subset of Docker Compose Lazuli can generate without a
// YAML dependency.
type ComposeFile struct {
	Services []Service
	Volumes  []string
}

// Service describes a Docker Compose service.
type Service struct {
	Name        string
	Image       string
	Build       string
	Command     []string
	Restart     string
	DependsOn   []string
	Environment []EnvVar
	Ports       []Port
	Volumes     []VolumeMount
}

// EnvVar is one environment variable assignment.
type EnvVar struct {
	Name  string
	Value string
}

// Port maps a container port to an optional host port.
type Port struct {
	Host      int
	Container int
	Protocol  string
}

// VolumeMount mounts a named volume or bind source at a container target.
type VolumeMount struct {
	Source   string
	Target   string
	ReadOnly bool
	Named    bool
}

// AppServiceConfig configures the app service helper.
type AppServiceConfig struct {
	Name        string
	Image       string
	Build       string
	Command     []string
	Restart     string
	DependsOn   []string
	Environment []EnvVar
	Ports       []Port
	Volumes     []VolumeMount
}

// PostgresServiceConfig configures a Postgres service with development-oriented
// defaults.
type PostgresServiceConfig struct {
	Name        string
	Image       string
	Database    string
	User        string
	Password    string
	Restart     string
	HostPort    int
	DataVolume  string
	Environment []EnvVar
	Volumes     []VolumeMount
}

// RedisServiceConfig configures a Redis service with development-oriented
// defaults.
type RedisServiceConfig struct {
	Name        string
	Image       string
	Restart     string
	HostPort    int
	DataVolume  string
	Environment []EnvVar
	Volumes     []VolumeMount
}

// NewComposeFile returns a Compose file with the provided services.
func NewComposeFile(services ...Service) ComposeFile {
	return ComposeFile{Services: append([]Service(nil), services...)}
}

// AppService builds a generic app service from config.
func AppService(config AppServiceConfig) Service {
	name := strings.TrimSpace(config.Name)
	if name == "" {
		name = defaultAppName
	}
	restart := strings.TrimSpace(config.Restart)
	if restart == "" {
		restart = defaultRestartPolicy
	}
	return Service{
		Name:        name,
		Image:       strings.TrimSpace(config.Image),
		Build:       strings.TrimSpace(config.Build),
		Command:     append([]string(nil), config.Command...),
		Restart:     restart,
		DependsOn:   append([]string(nil), config.DependsOn...),
		Environment: append([]EnvVar(nil), config.Environment...),
		Ports:       append([]Port(nil), config.Ports...),
		Volumes:     append([]VolumeMount(nil), config.Volumes...),
	}
}

// PostgresService builds a Postgres service using the official image.
func PostgresService(config PostgresServiceConfig) Service {
	name := strings.TrimSpace(config.Name)
	if name == "" {
		name = defaultPostgresName
	}
	image := strings.TrimSpace(config.Image)
	if image == "" {
		image = defaultPostgresImage
	}
	database := strings.TrimSpace(config.Database)
	if database == "" {
		database = defaultPostgresDB
	}
	user := strings.TrimSpace(config.User)
	if user == "" {
		user = defaultPostgresUser
	}
	password := config.Password
	if password == "" {
		password = defaultPostgresPass
	}
	restart := strings.TrimSpace(config.Restart)
	if restart == "" {
		restart = defaultRestartPolicy
	}
	dataVolume := strings.TrimSpace(config.DataVolume)
	if dataVolume == "" {
		dataVolume = name + "-data"
	}

	env := []EnvVar{
		{Name: "POSTGRES_DB", Value: database},
		{Name: "POSTGRES_USER", Value: user},
		{Name: "POSTGRES_PASSWORD", Value: password},
	}
	env = append(env, config.Environment...)

	var ports []Port
	if config.HostPort > 0 {
		ports = append(ports, TCP(config.HostPort, defaultPostgresPort))
	}

	volumes := []VolumeMount{NamedVolume(dataVolume, defaultPostgresDataDir)}
	volumes = append(volumes, config.Volumes...)

	return Service{
		Name:        name,
		Image:       image,
		Restart:     restart,
		Environment: env,
		Ports:       ports,
		Volumes:     volumes,
	}
}

// RedisService builds a Redis service using the official image and append-only
// persistence.
func RedisService(config RedisServiceConfig) Service {
	name := strings.TrimSpace(config.Name)
	if name == "" {
		name = defaultRedisName
	}
	image := strings.TrimSpace(config.Image)
	if image == "" {
		image = defaultRedisImage
	}
	restart := strings.TrimSpace(config.Restart)
	if restart == "" {
		restart = defaultRestartPolicy
	}
	dataVolume := strings.TrimSpace(config.DataVolume)
	if dataVolume == "" {
		dataVolume = name + "-data"
	}

	var ports []Port
	if config.HostPort > 0 {
		ports = append(ports, TCP(config.HostPort, defaultRedisPort))
	}

	volumes := []VolumeMount{NamedVolume(dataVolume, defaultRedisDataDir)}
	volumes = append(volumes, config.Volumes...)

	return Service{
		Name:        name,
		Image:       image,
		Command:     []string{"redis-server", "--appendonly", "yes"},
		Restart:     restart,
		Environment: append([]EnvVar(nil), config.Environment...),
		Ports:       ports,
		Volumes:     volumes,
	}
}

// Env returns an environment variable assignment.
func Env(name, value string) EnvVar {
	return EnvVar{Name: name, Value: value}
}

// EnvFromMap converts a map into sorted environment entries.
func EnvFromMap(values map[string]string) []EnvVar {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	vars := make([]EnvVar, 0, len(keys))
	for _, key := range keys {
		vars = append(vars, Env(key, values[key]))
	}
	return vars
}

// TCP maps a TCP host port to a container port. A host value of zero renders
// only the container port.
func TCP(host, container int) Port {
	return Port{Host: host, Container: container, Protocol: "tcp"}
}

// UDP maps a UDP host port to a container port. A host value of zero renders
// only the container port.
func UDP(host, container int) Port {
	return Port{Host: host, Container: container, Protocol: "udp"}
}

// ContainerPort exposes only a container TCP port.
func ContainerPort(container int) Port {
	return TCP(0, container)
}

// NamedVolume returns a Docker-managed named volume mount.
func NamedVolume(name, target string) VolumeMount {
	return VolumeMount{Source: name, Target: target, Named: true}
}

// BindMount returns a host bind mount.
func BindMount(source, target string) VolumeMount {
	return VolumeMount{Source: source, Target: target}
}

// ReadOnly marks a volume mount read-only.
func ReadOnly(mount VolumeMount) VolumeMount {
	mount.ReadOnly = true
	return mount
}

// Validate checks the compose file.
func (f ComposeFile) Validate() error {
	return ValidateCompose(f)
}

// Render renders the compose file as deterministic YAML.
func (f ComposeFile) Render() (string, error) {
	return RenderCompose(f)
}

// ValidateCompose validates a ComposeFile.
func ValidateCompose(file ComposeFile) error {
	_, err := normalizeCompose(file)
	return err
}

// RenderCompose renders a ComposeFile as deterministic YAML.
func RenderCompose(file ComposeFile) (string, error) {
	normalized, err := normalizeCompose(file)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString("services:\n")
	for _, service := range normalized.Services {
		b.WriteString("  ")
		b.WriteString(service.Name)
		b.WriteString(":\n")
		if service.Image != "" {
			writeScalar(&b, 4, "image", service.Image)
		}
		if service.Build != "" {
			writeScalar(&b, 4, "build", service.Build)
		}
		if len(service.Command) > 0 {
			writeList(&b, 4, "command", service.Command)
		}
		if service.Restart != "" {
			writeScalar(&b, 4, "restart", service.Restart)
		}
		if len(service.DependsOn) > 0 {
			writeList(&b, 4, "depends_on", service.DependsOn)
		}
		if len(service.Environment) > 0 {
			b.WriteString("    environment:\n")
			for _, env := range service.Environment {
				b.WriteString("      ")
				b.WriteString(env.Name)
				b.WriteString(": ")
				b.WriteString(quoteYAML(env.Value))
				b.WriteByte('\n')
			}
		}
		if len(service.Ports) > 0 {
			b.WriteString("    ports:\n")
			for _, port := range service.Ports {
				b.WriteString("      - ")
				b.WriteString(quoteYAML(renderPort(port)))
				b.WriteByte('\n')
			}
		}
		if len(service.Volumes) > 0 {
			b.WriteString("    volumes:\n")
			for _, volume := range service.Volumes {
				b.WriteString("      - ")
				b.WriteString(quoteYAML(renderVolumeMount(volume)))
				b.WriteByte('\n')
			}
		}
	}
	if len(normalized.Volumes) > 0 {
		b.WriteString("volumes:\n")
		for _, volume := range normalized.Volumes {
			b.WriteString("  ")
			b.WriteString(volume)
			b.WriteString(": {}\n")
		}
	}
	return b.String(), nil
}

// ValidateEnv validates environment entries for env-file rendering.
func ValidateEnv(vars []EnvVar) error {
	_, err := normalizeEnv(vars, "env")
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInvalidEnv, err)
	}
	return nil
}

// RenderEnv renders deterministic dotenv-style KEY=value lines.
func RenderEnv(vars []EnvVar) (string, error) {
	normalized, err := normalizeEnv(vars, "env")
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrInvalidEnv, err)
	}

	var b strings.Builder
	for _, env := range normalized {
		b.WriteString(env.Name)
		b.WriteByte('=')
		b.WriteString(quoteEnvValue(env.Value))
		b.WriteByte('\n')
	}
	return b.String(), nil
}

type normalizedCompose struct {
	Services []Service
	Volumes  []string
}

func normalizeCompose(file ComposeFile) (normalizedCompose, error) {
	var errs []error
	if len(file.Services) == 0 {
		errs = append(errs, invalidCompose("services", "at least one service is required"))
	}

	services := make([]Service, 0, len(file.Services))
	serviceNames := make(map[string]struct{}, len(file.Services))
	namedVolumes := map[string]struct{}{}
	explicitVolumes := map[string]struct{}{}

	for i, volume := range file.Volumes {
		name := strings.TrimSpace(volume)
		field := fmt.Sprintf("volumes[%d]", i)
		if name == "" {
			errs = append(errs, invalidCompose(field, "name is required"))
			continue
		}
		if !validComposeName(name) {
			errs = append(errs, invalidCompose(field, fmt.Sprintf("invalid name %q", name)))
			continue
		}
		if _, ok := explicitVolumes[name]; ok {
			errs = append(errs, invalidCompose(field, fmt.Sprintf("duplicate volume %q", name)))
			continue
		}
		explicitVolumes[name] = struct{}{}
		namedVolumes[name] = struct{}{}
	}

	for i, service := range file.Services {
		normalized, serviceErrs := normalizeService(service, i)
		errs = append(errs, serviceErrs...)
		if normalized.Name == "" {
			continue
		}
		if _, ok := serviceNames[normalized.Name]; ok {
			errs = append(errs, invalidCompose(fmt.Sprintf("services[%d].name", i), fmt.Sprintf("duplicate service %q", normalized.Name)))
			continue
		}
		serviceNames[normalized.Name] = struct{}{}
		for _, volume := range normalized.Volumes {
			if volume.Named {
				namedVolumes[volume.Source] = struct{}{}
			}
		}
		services = append(services, normalized)
	}

	for i := range services {
		for _, dependency := range services[i].DependsOn {
			if _, ok := serviceNames[dependency]; !ok {
				errs = append(errs, invalidCompose("services."+services[i].Name+".depends_on", fmt.Sprintf("unknown service %q", dependency)))
			}
		}
	}

	if err := errors.Join(errs...); err != nil {
		return normalizedCompose{}, err
	}

	sort.SliceStable(services, func(i, j int) bool {
		return services[i].Name < services[j].Name
	})

	volumes := make([]string, 0, len(namedVolumes))
	for name := range namedVolumes {
		volumes = append(volumes, name)
	}
	sort.Strings(volumes)

	return normalizedCompose{Services: services, Volumes: volumes}, nil
}

func normalizeService(service Service, index int) (Service, []error) {
	var errs []error
	field := fmt.Sprintf("services[%d]", index)
	service.Name = strings.TrimSpace(service.Name)
	service.Image = strings.TrimSpace(service.Image)
	service.Build = strings.TrimSpace(service.Build)
	service.Restart = strings.TrimSpace(service.Restart)

	if service.Name == "" {
		errs = append(errs, invalidCompose(field+".name", "value is required"))
	} else if !validComposeName(service.Name) {
		errs = append(errs, invalidCompose(field+".name", fmt.Sprintf("invalid name %q", service.Name)))
	}
	if service.Image == "" && service.Build == "" {
		errs = append(errs, invalidCompose(field, "image or build is required"))
	}

	command := make([]string, 0, len(service.Command))
	for i, part := range service.Command {
		part = strings.TrimSpace(part)
		if part == "" {
			errs = append(errs, invalidCompose(fmt.Sprintf("%s.command[%d]", field, i), "value is required"))
			continue
		}
		command = append(command, part)
	}
	service.Command = command

	dependsOn, dependencyErrs := normalizeNameList(service.DependsOn, field+".depends_on")
	errs = append(errs, dependencyErrs...)
	service.DependsOn = dependsOn

	env, err := normalizeEnv(service.Environment, field+".environment")
	if err != nil {
		errs = append(errs, invalidCompose(field+".environment", err.Error()))
	}
	service.Environment = env

	ports, portErrs := normalizePorts(service.Ports, field+".ports")
	errs = append(errs, portErrs...)
	service.Ports = ports

	volumes, volumeErrs := normalizeVolumeMounts(service.Volumes, field+".volumes")
	errs = append(errs, volumeErrs...)
	service.Volumes = volumes

	return service, errs
}

func normalizeNameList(values []string, field string) ([]string, []error) {
	var errs []error
	out := make([]string, 0, len(values))
	seen := map[string]struct{}{}
	for i, value := range values {
		value = strings.TrimSpace(value)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if value == "" {
			errs = append(errs, invalidCompose(itemField, "value is required"))
			continue
		}
		if !validComposeName(value) {
			errs = append(errs, invalidCompose(itemField, fmt.Sprintf("invalid name %q", value)))
			continue
		}
		if _, ok := seen[value]; ok {
			errs = append(errs, invalidCompose(itemField, fmt.Sprintf("duplicate value %q", value)))
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out, errs
}

func normalizeEnv(vars []EnvVar, field string) ([]EnvVar, error) {
	normalized := make([]EnvVar, 0, len(vars))
	seen := make(map[string]struct{}, len(vars))
	var errs []error
	for i, env := range vars {
		env.Name = strings.TrimSpace(env.Name)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if env.Name == "" {
			errs = append(errs, fmt.Errorf("%s.name is required", itemField))
			continue
		}
		if !validEnvName(env.Name) {
			errs = append(errs, fmt.Errorf("%s.name %q is invalid", itemField, env.Name))
			continue
		}
		if _, ok := seen[env.Name]; ok {
			errs = append(errs, fmt.Errorf("%s.name duplicate %q", itemField, env.Name))
			continue
		}
		seen[env.Name] = struct{}{}
		normalized = append(normalized, env)
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, errors.Join(errs...)
}

func normalizePorts(ports []Port, field string) ([]Port, []error) {
	var errs []error
	out := make([]Port, 0, len(ports))
	seen := map[string]struct{}{}
	for i, port := range ports {
		port.Protocol = strings.ToLower(strings.TrimSpace(port.Protocol))
		if port.Protocol == "" {
			port.Protocol = "tcp"
		}
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if port.Container < 1 || port.Container > 65535 {
			errs = append(errs, invalidCompose(itemField+".container", "must be between 1 and 65535"))
			continue
		}
		if port.Host < 0 || port.Host > 65535 {
			errs = append(errs, invalidCompose(itemField+".host", "must be between 0 and 65535"))
			continue
		}
		if port.Protocol != "tcp" && port.Protocol != "udp" {
			errs = append(errs, invalidCompose(itemField+".protocol", "must be tcp or udp"))
			continue
		}
		key := renderPort(port)
		if _, ok := seen[key]; ok {
			errs = append(errs, invalidCompose(itemField, fmt.Sprintf("duplicate port %q", key)))
			continue
		}
		seen[key] = struct{}{}
		out = append(out, port)
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Container != out[j].Container {
			return out[i].Container < out[j].Container
		}
		if out[i].Host != out[j].Host {
			return out[i].Host < out[j].Host
		}
		return out[i].Protocol < out[j].Protocol
	})
	return out, errs
}

func normalizeVolumeMounts(volumes []VolumeMount, field string) ([]VolumeMount, []error) {
	var errs []error
	out := make([]VolumeMount, 0, len(volumes))
	seenTargets := map[string]struct{}{}
	for i, volume := range volumes {
		volume.Source = strings.TrimSpace(volume.Source)
		volume.Target = strings.TrimSpace(volume.Target)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if volume.Source == "" {
			errs = append(errs, invalidCompose(itemField+".source", "value is required"))
			continue
		}
		if volume.Target == "" {
			errs = append(errs, invalidCompose(itemField+".target", "value is required"))
			continue
		}
		if !strings.HasPrefix(volume.Target, "/") {
			errs = append(errs, invalidCompose(itemField+".target", "must be an absolute container path"))
			continue
		}
		if volume.Named && !validComposeName(volume.Source) {
			errs = append(errs, invalidCompose(itemField+".source", fmt.Sprintf("invalid named volume %q", volume.Source)))
			continue
		}
		if _, ok := seenTargets[volume.Target]; ok {
			errs = append(errs, invalidCompose(itemField+".target", fmt.Sprintf("duplicate target %q", volume.Target)))
			continue
		}
		seenTargets[volume.Target] = struct{}{}
		out = append(out, volume)
	}
	sort.SliceStable(out, func(i, j int) bool {
		left := renderVolumeMount(out[i])
		right := renderVolumeMount(out[j])
		return left < right
	})
	return out, errs
}

func renderPort(port Port) string {
	var value string
	if port.Host > 0 {
		value = strconv.Itoa(port.Host) + ":" + strconv.Itoa(port.Container)
	} else {
		value = strconv.Itoa(port.Container)
	}
	if port.Protocol == "udp" {
		value += "/udp"
	}
	return value
}

func renderVolumeMount(volume VolumeMount) string {
	value := volume.Source + ":" + volume.Target
	if volume.ReadOnly {
		value += ":ro"
	}
	return value
}

func writeScalar(b *strings.Builder, indent int, key, value string) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(": ")
	b.WriteString(quoteYAML(value))
	b.WriteByte('\n')
}

func writeList(b *strings.Builder, indent int, key string, values []string) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(":\n")
	for _, value := range values {
		b.WriteString(strings.Repeat(" ", indent+2))
		b.WriteString("- ")
		b.WriteString(quoteYAML(value))
		b.WriteByte('\n')
	}
}

func quoteYAML(value string) string {
	return strconv.Quote(value)
}

func quoteEnvValue(value string) string {
	if value == "" {
		return ""
	}
	if isBareEnvValue(value) {
		return value
	}
	return strconv.Quote(value)
}

func isBareEnvValue(value string) bool {
	for _, r := range value {
		switch r {
		case ' ', '\t', '\n', '\r', '#', '"', '\'', '=':
			return false
		}
	}
	return true
}

func validComposeName(value string) bool {
	if value == "" {
		return false
	}
	for i, r := range value {
		ok := (r >= 'a' && r <= 'z') ||
			(r >= 'A' && r <= 'Z') ||
			(r >= '0' && r <= '9') ||
			r == '_' || r == '-' || r == '.'
		if !ok {
			return false
		}
		if i == 0 && !((r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9')) {
			return false
		}
	}
	return true
}

func validEnvName(value string) bool {
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

func invalidCompose(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidComposeConfig, field, detail)
}
