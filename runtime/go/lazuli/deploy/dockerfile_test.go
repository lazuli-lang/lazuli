package deploy

import (
	"errors"
	"strings"
	"testing"
)

func TestGenerateGoDockerfileDefaultsToDistrolessStatic(t *testing.T) {
	got, err := GenerateGoDockerfile(GoDockerfileConfig{})
	if err != nil {
		t.Fatalf("GenerateGoDockerfile() error = %v", err)
	}

	want := "FROM golang:1.25 AS build\n" +
		"WORKDIR /src\n" +
		"COPY go.mod go.sum ./\n" +
		"RUN go mod download\n" +
		"COPY . .\n" +
		"RUN CGO_ENABLED=0 GOOS=linux go build -trimpath -ldflags=\"-s -w\" -o /out/app .\n" +
		"\n" +
		"FROM gcr.io/distroless/static-debian12:nonroot\n" +
		"COPY --from=build /out/app /app\n" +
		"USER nonroot:nonroot\n" +
		"ENTRYPOINT [\"/app\"]\n"
	if got != want {
		t.Fatalf("GenerateGoDockerfile() = %q, want %q", got, want)
	}
}

func TestGenerateGoDockerfileRendersDeterministically(t *testing.T) {
	config := GoDockerfileConfig{
		Build: GoBuildStage{
			GoVersion:   "1.25.0",
			WorkDir:     "/workspace",
			MainPackage: "./cmd/api",
			BinaryName:  "server",
			GOARCH:      "amd64",
		},
		Runtime: ScratchStaticRuntime(),
		Labels: map[string]string{
			"org.opencontainers.image.source":   "https://example.test/repo",
			"org.opencontainers.image.revision": "abc123",
		},
		Env: map[string]string{
			"PORT":    "8080",
			"APP_ENV": "production",
		},
		Expose: []int{9090, 8080, 8080},
	}

	got, err := GenerateGoDockerfile(config)
	if err != nil {
		t.Fatalf("GenerateGoDockerfile() error = %v", err)
	}

	want := "FROM golang:1.25.0 AS build\n" +
		"WORKDIR /workspace\n" +
		"COPY go.mod go.sum ./\n" +
		"RUN go mod download\n" +
		"COPY . .\n" +
		"RUN CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -trimpath -ldflags=\"-s -w\" -o /out/server ./cmd/api\n" +
		"\n" +
		"FROM scratch\n" +
		"LABEL org.opencontainers.image.revision=\"abc123\"\n" +
		"LABEL org.opencontainers.image.source=\"https://example.test/repo\"\n" +
		"ENV APP_ENV=\"production\"\n" +
		"ENV PORT=\"8080\"\n" +
		"EXPOSE 8080 9090\n" +
		"COPY --from=build /out/server /server\n" +
		"USER 65532:65532\n" +
		"ENTRYPOINT [\"/server\"]\n"
	if got != want {
		t.Fatalf("GenerateGoDockerfile() = %q, want %q", got, want)
	}

	gotAgain, err := GenerateGoDockerfile(GoDockerfileConfig{
		Build:   config.Build,
		Runtime: config.Runtime,
		Labels: map[string]string{
			"org.opencontainers.image.revision": "abc123",
			"org.opencontainers.image.source":   "https://example.test/repo",
		},
		Env: map[string]string{
			"APP_ENV": "production",
			"PORT":    "8080",
		},
		Expose: []int{8080, 9090},
	})
	if err != nil {
		t.Fatalf("GenerateGoDockerfile(second) error = %v", err)
	}
	if gotAgain != got {
		t.Fatalf("GenerateGoDockerfile(second) = %q, want deterministic output %q", gotAgain, got)
	}
}

func TestGenerateGoDockerfileCustomRuntimeCanAvoidStaticBuild(t *testing.T) {
	got, err := GenerateGoDockerfile(GoDockerfileConfig{
		Build: GoBuildStage{
			MainPackage: "./cmd/api",
		},
		Runtime: CustomRuntime("debian:bookworm-slim"),
	})
	if err != nil {
		t.Fatalf("GenerateGoDockerfile() error = %v", err)
	}

	if strings.Contains(got, "CGO_ENABLED=0") {
		t.Fatalf("GenerateGoDockerfile() included static build env for custom runtime:\n%s", got)
	}
	if !strings.Contains(got, "RUN GOOS=linux go build") {
		t.Fatalf("GenerateGoDockerfile() missing GOOS build env:\n%s", got)
	}
	if strings.Contains(got, "\nUSER ") {
		t.Fatalf("GenerateGoDockerfile() included user for custom runtime:\n%s", got)
	}
}

func TestValidateGoDockerfileConfigRejectsInvalidValues(t *testing.T) {
	tests := []struct {
		name   string
		config GoDockerfileConfig
		field  string
	}{
		{
			name: "unsafe main package",
			config: GoDockerfileConfig{
				Build: GoBuildStage{MainPackage: "./cmd/api; rm -rf /"},
			},
			field: "build.main_package",
		},
		{
			name: "unsafe binary name",
			config: GoDockerfileConfig{
				Build: GoBuildStage{BinaryName: "../api"},
			},
			field: "build.binary_name",
		},
		{
			name: "unsafe workdir",
			config: GoDockerfileConfig{
				Build: GoBuildStage{WorkDir: "/src/../app"},
			},
			field: "build.work_dir",
		},
		{
			name: "invalid env key",
			config: GoDockerfileConfig{
				Env: map[string]string{"APP-ENV": "production"},
			},
			field: "env.APP-ENV",
		},
		{
			name: "invalid label key",
			config: GoDockerfileConfig{
				Labels: map[string]string{"bad key": "value"},
			},
			field: "labels.bad key",
		},
		{
			name: "invalid expose port",
			config: GoDockerfileConfig{
				Expose: []int{0},
			},
			field: "expose",
		},
		{
			name: "unsafe runtime user",
			config: GoDockerfileConfig{
				Runtime: GoRuntimeStage{Image: "alpine:3.20", User: "root&&id"},
			},
			field: "runtime.user",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateGoDockerfileConfig(tt.config)
			if !errors.Is(err, ErrInvalidDockerfileConfig) {
				t.Fatalf("ValidateGoDockerfileConfig() error = %v, want ErrInvalidDockerfileConfig", err)
			}
			if !strings.Contains(err.Error(), tt.field) {
				t.Fatalf("ValidateGoDockerfileConfig() error = %v, want field %q", err, tt.field)
			}
		})
	}
}
