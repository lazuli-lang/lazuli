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
	// DefaultSystemdDescription is used when no unit description is provided.
	DefaultSystemdDescription = "Lazuli process"
	// DefaultSystemdRestartPolicy is used when no systemd restart policy is set.
	DefaultSystemdRestartPolicy = SystemdRestartOnFailure

	// Systemd restart policies supported by generated unit files.
	SystemdRestartNo         = "no"
	SystemdRestartAlways     = "always"
	SystemdRestartOnSuccess  = "on-success"
	SystemdRestartOnFailure  = "on-failure"
	SystemdRestartOnAbnormal = "on-abnormal"
	SystemdRestartOnAbort    = "on-abort"
	SystemdRestartOnWatchdog = "on-watchdog"
)

// ErrInvalidProcessConfig reports an invalid systemd unit or Procfile config.
var ErrInvalidProcessConfig = errors.New("lazuli/deploy: invalid process config")

// SystemdUnitConfig configures a generated systemd service unit.
type SystemdUnitConfig struct {
	// Description is emitted in the [Unit] section. Empty uses a Lazuli default.
	Description string
	// Command is the executable and arguments emitted as ExecStart.
	Command []string
	// Environment emits sorted Environment= entries.
	Environment []EnvVar
	// User optionally emits User= in the [Service] section.
	User string
	// WorkingDir optionally emits WorkingDirectory= in the [Service] section.
	WorkingDir string
	// Restart emits Restart=. Empty uses DefaultSystemdRestartPolicy.
	Restart string
}

// Procfile contains process entries for a deterministic Procfile.
type Procfile struct {
	Processes []ProcfileProcess
}

// ProcfileProcess configures one Procfile process type.
type ProcfileProcess struct {
	Name string
	// Command is rendered as a shell-quoted command line.
	Command []string
	// Environment emits sorted inline KEY=value assignments.
	Environment []EnvVar
	// WorkingDir optionally prefixes the process with `cd <dir> &&`.
	WorkingDir string
	// User is rejected because Procfile has no portable user field.
	User string
	// Restart is rejected because Procfile has no portable restart field.
	Restart string
}

// NewProcfile returns a Procfile with the provided processes.
func NewProcfile(processes ...ProcfileProcess) Procfile {
	return Procfile{Processes: append([]ProcfileProcess(nil), processes...)}
}

// Validate checks the Procfile.
func (f Procfile) Validate() error {
	return ValidateProcfile(f)
}

// Render renders the Procfile as deterministic text.
func (f Procfile) Render() (string, error) {
	return RenderProcfile(f)
}

// ValidateSystemdUnitConfig reports whether config can be rendered as a
// systemd service unit after defaults are applied.
func ValidateSystemdUnitConfig(config SystemdUnitConfig) error {
	_, err := normalizeSystemdUnitConfig(config)
	return err
}

// GenerateSystemdUnit returns deterministic systemd service unit text.
func GenerateSystemdUnit(config SystemdUnitConfig) (string, error) {
	normalized, err := normalizeSystemdUnitConfig(config)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString("[Unit]\n")
	b.WriteString("Description=")
	b.WriteString(normalized.Description)
	b.WriteString("\n\n")

	b.WriteString("[Service]\n")
	if normalized.WorkingDir != "" {
		b.WriteString("WorkingDirectory=")
		b.WriteString(normalized.WorkingDir)
		b.WriteByte('\n')
	}
	for _, env := range normalized.Environment {
		b.WriteString("Environment=")
		b.WriteString(strconv.Quote(env.Name + "=" + env.Value))
		b.WriteByte('\n')
	}
	if normalized.User != "" {
		b.WriteString("User=")
		b.WriteString(normalized.User)
		b.WriteByte('\n')
	}
	b.WriteString("ExecStart=")
	b.WriteString(renderSystemdCommand(normalized.Command))
	b.WriteByte('\n')
	b.WriteString("Restart=")
	b.WriteString(normalized.Restart)
	b.WriteString("\n\n")

	b.WriteString("[Install]\n")
	b.WriteString("WantedBy=multi-user.target\n")

	return b.String(), nil
}

// ValidateProcfile reports whether file can be rendered as a deterministic
// Procfile.
func ValidateProcfile(file Procfile) error {
	_, err := normalizeProcfile(file)
	return err
}

// RenderProcfile renders deterministic Procfile text.
func RenderProcfile(file Procfile) (string, error) {
	normalized, err := normalizeProcfile(file)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	for _, process := range normalized {
		b.WriteString(process.Name)
		b.WriteString(": ")
		if process.WorkingDir != "" {
			b.WriteString("cd ")
			b.WriteString(shellQuote(process.WorkingDir))
			b.WriteString(" && ")
		}
		for _, env := range process.Environment {
			b.WriteString(renderShellEnvAssignment(env))
			b.WriteByte(' ')
		}
		b.WriteString(renderShellCommand(process.Command))
		b.WriteByte('\n')
	}
	return b.String(), nil
}

func normalizeSystemdUnitConfig(config SystemdUnitConfig) (SystemdUnitConfig, error) {
	var errs []error

	config.Description = strings.TrimSpace(config.Description)
	if config.Description == "" {
		config.Description = DefaultSystemdDescription
	}
	if hasControlRune(config.Description) {
		errs = append(errs, invalidProcessConfig("description", "cannot contain control characters"))
	}

	command, commandErrs := normalizeProcessCommand(config.Command, "command")
	errs = append(errs, commandErrs...)
	config.Command = command

	env, envErrs := normalizeProcessEnv(config.Environment, "environment")
	errs = append(errs, envErrs...)
	config.Environment = env

	config.User = strings.TrimSpace(config.User)
	if config.User != "" && !validSystemdUser(config.User) {
		errs = append(errs, invalidProcessConfig("user", "must be a safe systemd user name"))
	}

	config.WorkingDir = strings.TrimSpace(config.WorkingDir)
	if config.WorkingDir != "" && !safeContainerPath(config.WorkingDir) {
		errs = append(errs, invalidProcessConfig("working_dir", "must be a safe absolute path"))
	}
	if config.WorkingDir != "" {
		config.WorkingDir = path.Clean(config.WorkingDir)
	}

	config.Restart = strings.TrimSpace(config.Restart)
	if config.Restart == "" {
		config.Restart = DefaultSystemdRestartPolicy
	}
	if !validSystemdRestartPolicy(config.Restart) {
		errs = append(errs, invalidProcessConfig("restart", "unsupported systemd restart policy"))
	}

	if err := errors.Join(errs...); err != nil {
		return SystemdUnitConfig{}, err
	}
	return config, nil
}

func normalizeProcfile(file Procfile) ([]ProcfileProcess, error) {
	var errs []error
	if len(file.Processes) == 0 {
		errs = append(errs, invalidProcessConfig("processes", "at least one process is required"))
	}

	processes := make([]ProcfileProcess, 0, len(file.Processes))
	seen := make(map[string]struct{}, len(file.Processes))
	for i, process := range file.Processes {
		field := fmt.Sprintf("processes[%d]", i)
		process.Name = strings.TrimSpace(process.Name)
		if process.Name == "" {
			errs = append(errs, invalidProcessConfig(field+".name", "value is required"))
		} else if !validProcfileProcessName(process.Name) {
			errs = append(errs, invalidProcessConfig(field+".name", fmt.Sprintf("invalid process name %q", process.Name)))
		} else if _, ok := seen[process.Name]; ok {
			errs = append(errs, invalidProcessConfig(field+".name", fmt.Sprintf("duplicate process %q", process.Name)))
		} else {
			seen[process.Name] = struct{}{}
		}

		command, commandErrs := normalizeProcessCommand(process.Command, field+".command")
		errs = append(errs, commandErrs...)
		process.Command = command

		env, envErrs := normalizeProcessEnv(process.Environment, field+".environment")
		errs = append(errs, envErrs...)
		process.Environment = env

		process.WorkingDir = strings.TrimSpace(process.WorkingDir)
		if process.WorkingDir != "" && !safeContainerPath(process.WorkingDir) {
			errs = append(errs, invalidProcessConfig(field+".working_dir", "must be a safe absolute path"))
		}
		if process.WorkingDir != "" {
			process.WorkingDir = path.Clean(process.WorkingDir)
		}

		process.User = strings.TrimSpace(process.User)
		if process.User != "" {
			if !validSystemdUser(process.User) {
				errs = append(errs, invalidProcessConfig(field+".user", "must be a safe user name"))
			} else {
				errs = append(errs, invalidProcessConfig(field+".user", "Procfile does not support a portable user field"))
			}
		}

		process.Restart = strings.TrimSpace(process.Restart)
		if process.Restart != "" {
			if !validSystemdRestartPolicy(process.Restart) {
				errs = append(errs, invalidProcessConfig(field+".restart", "unsupported restart policy"))
			} else {
				errs = append(errs, invalidProcessConfig(field+".restart", "Procfile does not support a portable restart policy"))
			}
		}

		if process.Name != "" {
			processes = append(processes, process)
		}
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	sort.SliceStable(processes, func(i, j int) bool {
		return processes[i].Name < processes[j].Name
	})
	return processes, nil
}

func normalizeProcessCommand(command []string, field string) ([]string, []error) {
	var errs []error
	if len(command) == 0 {
		return nil, []error{invalidProcessConfig(field, "at least one command token is required")}
	}

	normalized := make([]string, 0, len(command))
	for i, part := range command {
		part = strings.TrimSpace(part)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if part == "" {
			errs = append(errs, invalidProcessConfig(itemField, "value is required"))
			continue
		}
		if hasControlRune(part) {
			errs = append(errs, invalidProcessConfig(itemField, "cannot contain control characters"))
			continue
		}
		normalized = append(normalized, part)
	}
	if len(normalized) == 0 {
		errs = append(errs, invalidProcessConfig(field, "at least one command token is required"))
	}
	return normalized, errs
}

func normalizeProcessEnv(vars []EnvVar, field string) ([]EnvVar, []error) {
	var errs []error
	env, err := normalizeEnv(vars, field)
	if err != nil {
		errs = append(errs, invalidProcessConfig(field, err.Error()))
	}
	for _, item := range env {
		if hasControlRune(item.Value) {
			errs = append(errs, invalidProcessConfig(field+"."+item.Name, "value cannot contain control characters"))
		}
	}
	return env, errs
}

func renderSystemdCommand(command []string) string {
	parts := make([]string, 0, len(command))
	for _, part := range command {
		parts = append(parts, systemdQuote(part))
	}
	return strings.Join(parts, " ")
}

func systemdQuote(value string) string {
	if isBareProcessArg(value) {
		return value
	}
	return strconv.Quote(value)
}

func renderShellCommand(command []string) string {
	parts := make([]string, 0, len(command))
	for _, part := range command {
		parts = append(parts, shellQuote(part))
	}
	return strings.Join(parts, " ")
}

func renderShellEnvAssignment(env EnvVar) string {
	if env.Value == "" {
		return env.Name + "="
	}
	return env.Name + "=" + shellQuote(env.Value)
}

func shellQuote(value string) string {
	if isBareProcessArg(value) {
		return value
	}
	return "'" + strings.ReplaceAll(value, "'", `'"'"'`) + "'"
}

func isBareProcessArg(value string) bool {
	if value == "" {
		return false
	}
	for _, r := range value {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r >= '0' && r <= '9':
		case strings.ContainsRune("@%_+=:,./-", r):
		default:
			return false
		}
	}
	return true
}

func validSystemdUser(value string) bool {
	if value == "" || strings.TrimSpace(value) != value || strings.HasPrefix(value, "-") {
		return false
	}
	for _, r := range value {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r >= '0' && r <= '9':
		case r == '_', r == '-', r == '.', r == '@':
		default:
			return false
		}
	}
	return true
}

func validSystemdRestartPolicy(value string) bool {
	switch value {
	case SystemdRestartNo,
		SystemdRestartAlways,
		SystemdRestartOnSuccess,
		SystemdRestartOnFailure,
		SystemdRestartOnAbnormal,
		SystemdRestartOnAbort,
		SystemdRestartOnWatchdog:
		return true
	default:
		return false
	}
}

func validProcfileProcessName(value string) bool {
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
		if i == 0 && !unicode.IsLetter(r) {
			return false
		}
	}
	return true
}

func invalidProcessConfig(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidProcessConfig, field, detail)
}
