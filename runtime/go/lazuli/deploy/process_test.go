package deploy_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestGenerateSystemdUnitRendersDeterministically(t *testing.T) {
	config := deploy.SystemdUnitConfig{
		Description: "Lazuli API",
		Command:     []string{"/srv/lazuli/server", "--http", ":8080", "two words"},
		Environment: []deploy.EnvVar{
			deploy.Env("PORT", "8080"),
			deploy.Env("APP_ENV", "production"),
		},
		User:       "lazuli",
		WorkingDir: "/srv/lazuli",
		Restart:    deploy.SystemdRestartAlways,
	}

	got, err := deploy.GenerateSystemdUnit(config)
	if err != nil {
		t.Fatalf("GenerateSystemdUnit() error = %v", err)
	}

	want := `[Unit]
Description=Lazuli API

[Service]
WorkingDirectory=/srv/lazuli
Environment="APP_ENV=production"
Environment="PORT=8080"
User=lazuli
ExecStart=/srv/lazuli/server --http :8080 "two words"
Restart=always

[Install]
WantedBy=multi-user.target
`
	if got != want {
		t.Fatalf("GenerateSystemdUnit() =\n%s\nwant\n%s", got, want)
	}

	gotAgain, err := deploy.GenerateSystemdUnit(deploy.SystemdUnitConfig{
		Description: config.Description,
		Command:     config.Command,
		Environment: []deploy.EnvVar{
			deploy.Env("APP_ENV", "production"),
			deploy.Env("PORT", "8080"),
		},
		User:       config.User,
		WorkingDir: config.WorkingDir,
		Restart:    config.Restart,
	})
	if err != nil {
		t.Fatalf("GenerateSystemdUnit(second) error = %v", err)
	}
	if gotAgain != got {
		t.Fatalf("GenerateSystemdUnit(second) =\n%s\nwant deterministic output\n%s", gotAgain, got)
	}
}

func TestGenerateSystemdUnitAppliesDefaults(t *testing.T) {
	got, err := deploy.GenerateSystemdUnit(deploy.SystemdUnitConfig{
		Command: []string{"/app/server"},
	})
	if err != nil {
		t.Fatalf("GenerateSystemdUnit() error = %v", err)
	}

	want := `[Unit]
Description=Lazuli process

[Service]
ExecStart=/app/server
Restart=on-failure

[Install]
WantedBy=multi-user.target
`
	if got != want {
		t.Fatalf("GenerateSystemdUnit() =\n%s\nwant\n%s", got, want)
	}
}

func TestRenderProcfileSortsProcessesAndQuotesValues(t *testing.T) {
	got, err := deploy.RenderProcfile(deploy.NewProcfile(
		deploy.ProcfileProcess{
			Name:    "worker",
			Command: []string{"./worker", "--queue", "critical jobs"},
		},
		deploy.ProcfileProcess{
			Name: "web",
			Command: []string{
				"/srv/lazuli/server",
				"--http",
				":8080",
			},
			Environment: []deploy.EnvVar{
				deploy.Env("SPACE", "two words"),
				deploy.Env("APP_ENV", "production"),
			},
			WorkingDir: "/srv/lazuli",
		},
	))
	if err != nil {
		t.Fatalf("RenderProcfile() error = %v", err)
	}

	want := "web: cd /srv/lazuli && APP_ENV=production SPACE='two words' /srv/lazuli/server --http :8080\n" +
		"worker: ./worker --queue 'critical jobs'\n"
	if got != want {
		t.Fatalf("RenderProcfile() = %q, want %q", got, want)
	}
}

func TestValidateSystemdUnitConfigRejectsInvalidValues(t *testing.T) {
	tests := []struct {
		name   string
		config deploy.SystemdUnitConfig
		field  string
	}{
		{
			name:   "missing command",
			config: deploy.SystemdUnitConfig{},
			field:  "command",
		},
		{
			name: "invalid env",
			config: deploy.SystemdUnitConfig{
				Command:     []string{"/app/server"},
				Environment: []deploy.EnvVar{deploy.Env("BAD-NAME", "value")},
			},
			field: "environment",
		},
		{
			name: "unsafe user",
			config: deploy.SystemdUnitConfig{
				Command: []string{"/app/server"},
				User:    "root;id",
			},
			field: "user",
		},
		{
			name: "unsafe working dir",
			config: deploy.SystemdUnitConfig{
				Command:    []string{"/app/server"},
				WorkingDir: "/srv/../app",
			},
			field: "working_dir",
		},
		{
			name: "invalid restart",
			config: deploy.SystemdUnitConfig{
				Command: []string{"/app/server"},
				Restart: "sometimes",
			},
			field: "restart",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := deploy.ValidateSystemdUnitConfig(tt.config)
			if !errors.Is(err, deploy.ErrInvalidProcessConfig) {
				t.Fatalf("ValidateSystemdUnitConfig() error = %v, want ErrInvalidProcessConfig", err)
			}
			if !strings.Contains(err.Error(), tt.field) {
				t.Fatalf("ValidateSystemdUnitConfig() error = %v, want field %q", err, tt.field)
			}
		})
	}
}

func TestValidateProcfileRejectsInvalidValues(t *testing.T) {
	err := deploy.ValidateProcfile(deploy.NewProcfile(
		deploy.ProcfileProcess{
			Name:       "web",
			Command:    []string{"/app/server"},
			WorkingDir: "relative",
			User:       "app",
			Restart:    deploy.SystemdRestartAlways,
		},
		deploy.ProcfileProcess{
			Name:    "web",
			Command: []string{"/app/worker"},
		},
		deploy.ProcfileProcess{
			Name: "bad name",
			Environment: []deploy.EnvVar{
				deploy.Env("1BAD", "value"),
			},
		},
	))
	if !errors.Is(err, deploy.ErrInvalidProcessConfig) {
		t.Fatalf("ValidateProcfile() error = %v, want ErrInvalidProcessConfig", err)
	}
	for _, fragment := range []string{
		"working_dir",
		"user",
		"restart",
		"duplicate process",
		"invalid process name",
		"command",
		"1BAD",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateProcfile() error = %v, want fragment %q", err, fragment)
		}
	}
}
