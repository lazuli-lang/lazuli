package deploy_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestBuildGoTargetMatrixSortsAndDeduplicates(t *testing.T) {
	got, err := deploy.BuildGoTargetMatrix(
		[]string{"linux", "darwin", "linux"},
		[]string{"arm64", "amd64", "amd64"},
	)
	if err != nil {
		t.Fatalf("BuildGoTargetMatrix() error = %v", err)
	}

	want := []deploy.GoBuildTarget{
		deploy.GoTarget("darwin", "amd64"),
		deploy.GoTarget("darwin", "arm64"),
		deploy.GoTarget("linux", "amd64"),
		deploy.GoTarget("linux", "arm64"),
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("BuildGoTargetMatrix() = %#v, want %#v", got, want)
	}
}

func TestBuildCrossCompilePlanBuildsDeterministicArgvAndMetadata(t *testing.T) {
	targets, err := deploy.BuildGoTargetMatrix(
		[]string{"windows", "linux"},
		[]string{"arm64", "amd64"},
	)
	if err != nil {
		t.Fatalf("BuildGoTargetMatrix() error = %v", err)
	}

	plan, err := deploy.BuildCrossCompilePlan(deploy.CrossCompileConfig{
		Targets:     append([]deploy.GoBuildTarget{deploy.GoTarget("linux", "amd64")}, targets...),
		MainPackage: " ./cmd/api ",
		BinaryName:  "server",
		OutputDir:   "build/release",
		BuildTags:   []string{"prod", "netgo", "prod"},
		LDFlags: []string{
			"-s",
			"-w",
			"-X lazuli.dev/runtime/lazuli/version.Commit=abc123",
		},
	})
	if err != nil {
		t.Fatalf("BuildCrossCompilePlan() error = %v", err)
	}

	if plan.CGO != deploy.CGOModeDisabled {
		t.Fatalf("plan.CGO = %q, want %q", plan.CGO, deploy.CGOModeDisabled)
	}
	if !reflect.DeepEqual(plan.BuildTags, []string{"netgo", "prod"}) {
		t.Fatalf("plan.BuildTags = %#v, want sorted unique tags", plan.BuildTags)
	}
	if len(plan.Builds) != 4 {
		t.Fatalf("len(plan.Builds) = %d, want 4", len(plan.Builds))
	}

	gotFirst := plan.Builds[0]
	wantFirst := deploy.CrossCompileBuild{
		Target: deploy.GoBuildTarget{GOOS: "linux", GOARCH: "amd64"},
		Environment: []deploy.EnvVar{
			deploy.Env("CGO_ENABLED", "0"),
			deploy.Env("GOOS", "linux"),
			deploy.Env("GOARCH", "amd64"),
		},
		Argv: []string{
			"go",
			"build",
			"-trimpath",
			"-tags",
			"netgo,prod",
			"-ldflags",
			"-s -w -X lazuli.dev/runtime/lazuli/version.Commit=abc123",
			"-o",
			"build/release/server_linux_amd64",
			"./cmd/api",
		},
		Output:       "build/release/server_linux_amd64",
		MainPackage:  "./cmd/api",
		CGO:          deploy.CGOModeDisabled,
		BuildTags:    []string{"netgo", "prod"},
		LDFlags:      []string{"-s", "-w", "-X lazuli.dev/runtime/lazuli/version.Commit=abc123"},
		StaticBinary: true,
	}
	if !reflect.DeepEqual(gotFirst, wantFirst) {
		t.Fatalf("first build = %#v, want %#v", gotFirst, wantFirst)
	}

	gotLast := plan.Builds[len(plan.Builds)-1]
	if gotLast.Output != "build/release/server_windows_arm64.exe" {
		t.Fatalf("last output = %q, want windows exe output", gotLast.Output)
	}
	if gotLast.StaticBinary {
		t.Fatalf("windows build StaticBinary = true, want false")
	}
}

func TestBuildCrossCompilePlanCanUseDefaultCGOMode(t *testing.T) {
	plan, err := deploy.BuildCrossCompilePlan(deploy.CrossCompileConfig{
		Targets: []deploy.GoBuildTarget{deploy.GoTarget("darwin", "arm64")},
		CGO:     deploy.CGOModeDefault,
	})
	if err != nil {
		t.Fatalf("BuildCrossCompilePlan() error = %v", err)
	}

	build := plan.Builds[0]
	wantEnv := []deploy.EnvVar{
		deploy.Env("GOOS", "darwin"),
		deploy.Env("GOARCH", "arm64"),
	}
	if !reflect.DeepEqual(build.Environment, wantEnv) {
		t.Fatalf("Environment = %#v, want %#v", build.Environment, wantEnv)
	}
	if build.StaticBinary {
		t.Fatalf("StaticBinary = true, want false when CGO mode is default")
	}
}

func TestValidateCrossCompileConfigRejectsInvalidValues(t *testing.T) {
	err := deploy.ValidateCrossCompileConfig(deploy.CrossCompileConfig{
		Targets: []deploy.GoBuildTarget{
			deploy.GoTarget("linux", "amd64"),
			deploy.GoTarget("bad os", "arm64"),
			deploy.GoTarget("linux", "arm-64"),
		},
		MainPackage: "./cmd/api; rm -rf /",
		BinaryName:  "../server",
		OutputDir:   "../dist",
		CGO:         "sometimes",
		BuildTags:   []string{"prod", "bad tag"},
		LDFlags:     []string{"-s", "line\nbreak"},
	})
	if !errors.Is(err, deploy.ErrInvalidCrossCompileConfig) {
		t.Fatalf("ValidateCrossCompileConfig() error = %v, want ErrInvalidCrossCompileConfig", err)
	}

	for _, fragment := range []string{
		"targets[1].goos",
		"targets[2].goarch",
		"main_package",
		"binary_name",
		"output_dir",
		"cgo",
		"build_tags[1]",
		"ldflags[1]",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateCrossCompileConfig() error = %v, want fragment %q", err, fragment)
		}
	}
}
