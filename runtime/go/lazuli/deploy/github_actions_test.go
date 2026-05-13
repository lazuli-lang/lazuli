package deploy_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestRenderGitHubActionsCIWorkflowRendersGoAndRustMatrix(t *testing.T) {
	config := deploy.GitHubActionsCIConfig{
		Name:     "Lazuli CI",
		Branches: []string{"main", "release"},
		Env: []deploy.EnvSpec{
			deploy.EnvValue("APP_ENV", "test"),
			deploy.SecretEnv("API_TOKEN", "CI_API_TOKEN"),
		},
		Go: deploy.GitHubActionsGoJob{
			Enabled:    true,
			Versions:   []string{"1.25", "1.24", "1.25"},
			WorkingDir: "runtime/go",
			Env: []deploy.EnvSpec{
				deploy.EnvValue("GOFLAGS", "-mod=readonly"),
			},
			TestCommand:          []string{"go test ./lazuli/deploy"},
			BuildCommand:         []string{"go build ./lazuli/deploy"},
			CacheDependencyFiles: []string{"go.sum", "go.mod"},
		},
		Rust: deploy.GitHubActionsRustJob{
			Enabled:    true,
			Toolchains: []string{"stable", "1.88.0"},
			Targets:    []string{"x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"},
			WorkingDir: "crates",
			Env: []deploy.EnvSpec{
				deploy.EnvValue("RUSTFLAGS", "-Dwarnings"),
			},
			TestCommand:          []string{"cargo test --locked --workspace"},
			BuildCommand:         []string{"cargo build --locked --workspace"},
			CacheDependencyFiles: []string{"Cargo.lock", "Cargo.toml"},
		},
	}

	got, err := deploy.RenderGitHubActionsCIWorkflow(config)
	if err != nil {
		t.Fatalf("RenderGitHubActionsCIWorkflow() error = %v", err)
	}

	want := "name: \"Lazuli CI\"\n" +
		"on:\n" +
		"  push:\n" +
		"    branches:\n" +
		"      - \"main\"\n" +
		"      - \"release\"\n" +
		"  pull_request:\n" +
		"    branches:\n" +
		"      - \"main\"\n" +
		"      - \"release\"\n" +
		"env:\n" +
		"  API_TOKEN: \"${{ secrets.CI_API_TOKEN }}\"\n" +
		"  APP_ENV: \"test\"\n" +
		"jobs:\n" +
		"  go:\n" +
		"    name: \"Go\"\n" +
		"    runs-on: \"ubuntu-latest\"\n" +
		"    strategy:\n" +
		"      fail-fast: false\n" +
		"      matrix:\n" +
		"        go-version:\n" +
		"          - \"1.24\"\n" +
		"          - \"1.25\"\n" +
		"    env:\n" +
		"      GOFLAGS: \"-mod=readonly\"\n" +
		"    defaults:\n" +
		"      run:\n" +
		"        working-directory: \"runtime/go\"\n" +
		"    steps:\n" +
		"      - name: \"Checkout\"\n" +
		"        uses: \"actions/checkout@v4\"\n" +
		"      - name: \"Setup Go\"\n" +
		"        uses: \"actions/setup-go@v5\"\n" +
		"        with:\n" +
		"          go-version: \"${{ matrix.go-version }}\"\n" +
		"      - name: \"Cache dependencies\"\n" +
		"        uses: \"actions/cache@v4\"\n" +
		"        with:\n" +
		"          path: |\n" +
		"            ~/.cache/go-build\n" +
		"            ~/go/pkg/mod\n" +
		"          key: \"${{ runner.os }}-go-${{ matrix.go-version }}-${{ hashFiles('go.mod', 'go.sum') }}\"\n" +
		"          restore-keys: \"${{ runner.os }}-go-${{ matrix.go-version }}-\"\n" +
		"      - name: \"Test\"\n" +
		"        run: |\n" +
		"          go test ./lazuli/deploy\n" +
		"      - name: \"Build\"\n" +
		"        run: |\n" +
		"          go build ./lazuli/deploy\n" +
		"  rust:\n" +
		"    name: \"Rust\"\n" +
		"    runs-on: \"ubuntu-latest\"\n" +
		"    strategy:\n" +
		"      fail-fast: false\n" +
		"      matrix:\n" +
		"        rust-target:\n" +
		"          - \"aarch64-unknown-linux-gnu\"\n" +
		"          - \"x86_64-unknown-linux-gnu\"\n" +
		"        rust-toolchain:\n" +
		"          - \"1.88.0\"\n" +
		"          - \"stable\"\n" +
		"    env:\n" +
		"      RUSTFLAGS: \"-Dwarnings\"\n" +
		"    defaults:\n" +
		"      run:\n" +
		"        working-directory: \"crates\"\n" +
		"    steps:\n" +
		"      - name: \"Checkout\"\n" +
		"        uses: \"actions/checkout@v4\"\n" +
		"      - name: \"Setup Rust\"\n" +
		"        uses: \"dtolnay/rust-toolchain@stable\"\n" +
		"        with:\n" +
		"          target: \"${{ matrix.rust-target }}\"\n" +
		"          toolchain: \"${{ matrix.rust-toolchain }}\"\n" +
		"      - name: \"Cache dependencies\"\n" +
		"        uses: \"actions/cache@v4\"\n" +
		"        with:\n" +
		"          path: |\n" +
		"            ~/.cargo/bin\n" +
		"            ~/.cargo/git\n" +
		"            ~/.cargo/registry\n" +
		"            target\n" +
		"          key: \"${{ runner.os }}-rust-${{ matrix.rust-toolchain }}-${{ matrix.rust-target }}-${{ hashFiles('Cargo.lock', 'Cargo.toml') }}\"\n" +
		"          restore-keys: \"${{ runner.os }}-rust-${{ matrix.rust-toolchain }}-${{ matrix.rust-target }}-\"\n" +
		"      - name: \"Test\"\n" +
		"        run: |\n" +
		"          cargo test --locked --workspace\n" +
		"      - name: \"Build\"\n" +
		"        run: |\n" +
		"          cargo build --locked --workspace\n"
	if got != want {
		t.Fatalf("RenderGitHubActionsCIWorkflow() =\n%s\nwant:\n%s", got, want)
	}
}

func TestBuildGitHubActionsCIPlanDefaultsGoJob(t *testing.T) {
	plan, err := deploy.BuildGitHubActionsCIPlan(deploy.GitHubActionsCIConfig{
		Go: deploy.GitHubActionsGoJob{Enabled: true},
	})
	if err != nil {
		t.Fatalf("BuildGitHubActionsCIPlan() error = %v", err)
	}
	if plan.Name != deploy.DefaultGitHubActionsWorkflowName {
		t.Fatalf("Name = %q, want default", plan.Name)
	}
	if len(plan.Jobs) != 1 || plan.Jobs[0].ID != "go" {
		t.Fatalf("Jobs = %#v, want one go job", plan.Jobs)
	}
	job := plan.Jobs[0]
	if got := strings.Join(job.Matrix["go-version"], ","); got != deploy.DefaultGitHubActionsGoVersion {
		t.Fatalf("go matrix = %q, want default", got)
	}
	if got := strings.Join(job.Commands[0].Run, "\n"); got != "go test ./..." {
		t.Fatalf("test command = %q, want default", got)
	}
	if got := strings.Join(job.Commands[1].Run, "\n"); got != "go build ./..." {
		t.Fatalf("build command = %q, want default", got)
	}
}

func TestRenderGitHubActionsCISummaryRedactsSecrets(t *testing.T) {
	config := deploy.GitHubActionsCIConfig{
		Env: []deploy.EnvSpec{
			deploy.EnvValue("APP_ENV", "test"),
			deploy.SecretEnv("API_TOKEN", "CI_API_TOKEN"),
		},
		Go: deploy.GitHubActionsGoJob{
			Enabled: true,
			Env: []deploy.EnvSpec{
				deploy.SecretEnv("DATABASE_URL", "DATABASE_URL"),
			},
		},
	}

	got, err := deploy.RenderGitHubActionsCISummary(config)
	if err != nil {
		t.Fatalf("RenderGitHubActionsCISummary() error = %v", err)
	}
	if strings.Contains(got, "${{ secrets.") {
		t.Fatalf("summary leaked GitHub secret expression:\n%s", got)
	}
	if strings.Contains(got, "CI_API_TOKEN") || strings.Contains(got, "DATABASE_URL | DATABASE_URL") {
		t.Fatalf("summary leaked secret ref:\n%s", got)
	}
	if count := strings.Count(got, deploy.DefaultEnvSecretMask); count != 2 {
		t.Fatalf("summary redaction count = %d, want 2:\n%s", count, got)
	}
	if !strings.Contains(got, "| APP_ENV | test |") {
		t.Fatalf("summary missing literal env:\n%s", got)
	}
}

func TestValidateGitHubActionsCIConfigRejectsInvalidValues(t *testing.T) {
	tests := []struct {
		name     string
		config   deploy.GitHubActionsCIConfig
		fragment string
	}{
		{
			name:     "no jobs",
			config:   deploy.GitHubActionsCIConfig{},
			fragment: "jobs",
		},
		{
			name: "unsafe command",
			config: deploy.GitHubActionsCIConfig{
				Go: deploy.GitHubActionsGoJob{
					Enabled:     true,
					TestCommand: []string{"go test\n./..."},
				},
			},
			fragment: "go.commands[0].run[0]",
		},
		{
			name: "unsafe cache dependency",
			config: deploy.GitHubActionsCIConfig{
				Go: deploy.GitHubActionsGoJob{
					Enabled:              true,
					CacheDependencyFiles: []string{"../go.sum"},
				},
			},
			fragment: "go.cache_dependency_files[0]",
		},
		{
			name: "invalid secret name",
			config: deploy.GitHubActionsCIConfig{
				Go: deploy.GitHubActionsGoJob{
					Enabled: true,
					Env: []deploy.EnvSpec{
						deploy.SecretEnv("API_TOKEN", "api-token"),
					},
				},
			},
			fragment: "go.env.API_TOKEN",
		},
		{
			name: "unsafe working directory",
			config: deploy.GitHubActionsCIConfig{
				Rust: deploy.GitHubActionsRustJob{
					Enabled:    true,
					WorkingDir: "/tmp/app",
				},
			},
			fragment: "rust.working_dir",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := deploy.ValidateGitHubActionsCIConfig(tt.config)
			if !errors.Is(err, deploy.ErrInvalidGitHubActionsConfig) {
				t.Fatalf("ValidateGitHubActionsCIConfig() error = %v, want ErrInvalidGitHubActionsConfig", err)
			}
			if !strings.Contains(err.Error(), tt.fragment) {
				t.Fatalf("ValidateGitHubActionsCIConfig() error = %v, want fragment %q", err, tt.fragment)
			}
		})
	}
}
