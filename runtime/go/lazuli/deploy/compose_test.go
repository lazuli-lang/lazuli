package deploy_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestRenderComposeBuildsAppPostgresRedisStack(t *testing.T) {
	app := deploy.AppService(deploy.AppServiceConfig{
		Build: ".",
		DependsOn: []string{
			"redis",
			"postgres",
		},
		Environment: deploy.EnvFromMap(map[string]string{
			"APP_ENV":      "production",
			"DATABASE_URL": "postgres://lazuli:lazuli@postgres:5432/lazuli?sslmode=disable",
			"REDIS_URL":    "redis://redis:6379/0",
		}),
		Ports: []deploy.Port{
			deploy.TCP(8080, 8080),
		},
		Volumes: []deploy.VolumeMount{
			deploy.NamedVolume("uploads", "/app/uploads"),
			deploy.ReadOnly(deploy.BindMount("./config", "/app/config")),
		},
	})
	postgres := deploy.PostgresService(deploy.PostgresServiceConfig{
		HostPort: 15432,
		Password: "secret",
	})
	redis := deploy.RedisService(deploy.RedisServiceConfig{
		HostPort: 16379,
	})

	got, err := deploy.RenderCompose(deploy.NewComposeFile(redis, postgres, app))
	if err != nil {
		t.Fatalf("RenderCompose() error = %v", err)
	}

	want := `services:
  app:
    build: "."
    restart: "unless-stopped"
    depends_on:
      - "postgres"
      - "redis"
    environment:
      APP_ENV: "production"
      DATABASE_URL: "postgres://lazuli:lazuli@postgres:5432/lazuli?sslmode=disable"
      REDIS_URL: "redis://redis:6379/0"
    ports:
      - "8080:8080"
    volumes:
      - "./config:/app/config:ro"
      - "uploads:/app/uploads"
  postgres:
    image: "postgres:16-alpine"
    restart: "unless-stopped"
    environment:
      POSTGRES_DB: "lazuli"
      POSTGRES_PASSWORD: "secret"
      POSTGRES_USER: "lazuli"
    ports:
      - "15432:5432"
    volumes:
      - "postgres-data:/var/lib/postgresql/data"
  redis:
    image: "redis:7-alpine"
    command:
      - "redis-server"
      - "--appendonly"
      - "yes"
    restart: "unless-stopped"
    ports:
      - "16379:6379"
    volumes:
      - "redis-data:/data"
volumes:
  postgres-data: {}
  redis-data: {}
  uploads: {}
`
	if got != want {
		t.Fatalf("RenderCompose() =\n%s\nwant\n%s", got, want)
	}
}

func TestRenderEnvSortsAndQuotesValues(t *testing.T) {
	got, err := deploy.RenderEnv(deploy.EnvFromMap(map[string]string{
		"C":     "line\nbreak",
		"A":     "1",
		"EMPTY": "",
		"B":     "two words",
	}))
	if err != nil {
		t.Fatalf("RenderEnv() error = %v", err)
	}

	want := "A=1\nB=\"two words\"\nC=\"line\\nbreak\"\nEMPTY=\n"
	if got != want {
		t.Fatalf("RenderEnv() = %q, want %q", got, want)
	}
}

func TestValidateEnvRejectsInvalidAndDuplicateNames(t *testing.T) {
	err := deploy.ValidateEnv([]deploy.EnvVar{
		deploy.Env("APP_ENV", "production"),
		deploy.Env("APP_ENV", "duplicate"),
		deploy.Env("1BAD", "bad"),
	})
	if !errors.Is(err, deploy.ErrInvalidEnv) {
		t.Fatalf("ValidateEnv() error = %v, want ErrInvalidEnv", err)
	}
	for _, fragment := range []string{"duplicate", "1BAD"} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateEnv() error = %v, want fragment %q", err, fragment)
		}
	}
}

func TestValidateComposeReportsInvalidServicesPortsVolumesAndDependencies(t *testing.T) {
	err := deploy.ValidateCompose(deploy.ComposeFile{
		Volumes: []string{"bad volume", "cache", "cache"},
		Services: []deploy.Service{
			{
				Name:      "app",
				Image:     "busybox",
				DependsOn: []string{"missing"},
				Environment: []deploy.EnvVar{
					deploy.Env("BAD-NAME", "x"),
				},
				Ports: []deploy.Port{
					{Host: 8080, Container: 70000},
				},
				Volumes: []deploy.VolumeMount{
					deploy.NamedVolume("bad volume", "/data"),
				},
			},
			{
				Name:  "app",
				Build: ".",
			},
			{
				Name: "worker",
			},
		},
	})
	if !errors.Is(err, deploy.ErrInvalidComposeConfig) {
		t.Fatalf("ValidateCompose() error = %v, want ErrInvalidComposeConfig", err)
	}
	for _, fragment := range []string{
		"invalid name",
		"duplicate volume",
		"duplicate service",
		"unknown service",
		"BAD-NAME",
		"container",
		"invalid named volume",
		"image or build is required",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateCompose() error = %v, want fragment %q", err, fragment)
		}
	}
}
