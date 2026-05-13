package testkit_test

import (
	"errors"
	"reflect"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/testkit"
)

func TestPlanRaceTestsBuildsDeterministicCommand(t *testing.T) {
	plan, err := testkit.PlanRaceTests(testkit.RaceTestPlanOptions{
		Packages:     []string{" ./z ", "./a", "./a", "./internal/slow", ""},
		SkipPackages: []string{" ./internal/... ", "./z", "./z"},
		SkipTests:    []string{"TestSlow", " TestFlaky$ ", "TestSlow"},
		Timeout:      2 * time.Minute,
		Env: map[string]string{
			"GORACE":      "halt_on_error=1",
			"CGO_ENABLED": "1",
		},
	})
	if err != nil {
		t.Fatalf("PlanRaceTests() error = %v", err)
	}

	if want := []string{"./a"}; !reflect.DeepEqual(plan.Packages, want) {
		t.Fatalf("plan packages = %#v, want %#v", plan.Packages, want)
	}
	if want := []string{"./internal/...", "./z"}; !reflect.DeepEqual(plan.SkipPackages, want) {
		t.Fatalf("plan skip packages = %#v, want %#v", plan.SkipPackages, want)
	}
	if want := []string{"TestFlaky$", "TestSlow"}; !reflect.DeepEqual(plan.SkipTests, want) {
		t.Fatalf("plan skip tests = %#v, want %#v", plan.SkipTests, want)
	}

	wantArgv := []string{
		"go",
		"test",
		"-race",
		"-timeout=2m0s",
		"-skip=(?:TestFlaky$)|(?:TestSlow)",
		"./a",
	}
	if got := plan.CommandArgv(); !reflect.DeepEqual(got, wantArgv) {
		t.Fatalf("CommandArgv() = %#v, want %#v", got, wantArgv)
	}

	wantEnv := []string{"CGO_ENABLED=1", "GORACE=halt_on_error=1"}
	if got := plan.CommandEnv(); !reflect.DeepEqual(got, wantEnv) {
		t.Fatalf("CommandEnv() = %#v, want %#v", got, wantEnv)
	}
}

func TestPlanRaceTestsDefaultsToAllPackages(t *testing.T) {
	plan, err := testkit.PlanRaceTests(testkit.RaceTestPlanOptions{})
	if err != nil {
		t.Fatalf("PlanRaceTests(default) error = %v", err)
	}

	want := []string{"go", "test", "-race", "./..."}
	if got := plan.CommandArgv(); !reflect.DeepEqual(got, want) {
		t.Fatalf("CommandArgv(default) = %#v, want %#v", got, want)
	}
}

func TestSelectRaceTestPackagesFiltersExactAndSubtreeSkips(t *testing.T) {
	got := testkit.SelectRaceTestPackages(
		[]string{"./b", "./internal/db", "./a", "./internal", "./b"},
		[]string{"./internal/...", "./b"},
	)
	want := []string{"./a"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("SelectRaceTestPackages() = %#v, want %#v", got, want)
	}
}

func TestPlanRaceTestsValidatesInputs(t *testing.T) {
	tests := []struct {
		name string
		opts testkit.RaceTestPlanOptions
	}{
		{
			name: "all packages skipped",
			opts: testkit.RaceTestPlanOptions{
				Packages:     []string{"./a"},
				SkipPackages: []string{"./a"},
			},
		},
		{
			name: "negative timeout",
			opts: testkit.RaceTestPlanOptions{
				Timeout: -time.Second,
			},
		},
		{
			name: "invalid env key",
			opts: testkit.RaceTestPlanOptions{
				Env: map[string]string{"BAD=KEY": "1"},
			},
		},
		{
			name: "invalid skip regex",
			opts: testkit.RaceTestPlanOptions{
				SkipTests: []string{"Test("},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := testkit.PlanRaceTests(tt.opts)
			if !errors.Is(err, testkit.ErrRaceTestPlanInvalid) {
				t.Fatalf("PlanRaceTests() error = %v, want ErrRaceTestPlanInvalid", err)
			}
		})
	}
}

func TestRaceTestPlanReturnsCommandCopies(t *testing.T) {
	plan, err := testkit.PlanRaceTests(testkit.RaceTestPlanOptions{
		Packages: []string{"./a"},
		Env:      map[string]string{"GORACE": "halt_on_error=1"},
	})
	if err != nil {
		t.Fatalf("PlanRaceTests() error = %v", err)
	}

	argv := plan.CommandArgv()
	argv[0] = "changed"
	if got := plan.CommandArgv()[0]; got != "go" {
		t.Fatalf("CommandArgv() was mutated through returned slice: got first arg %q", got)
	}

	env := plan.CommandEnv()
	env[0] = "GORACE=changed"
	if got := plan.CommandEnv()[0]; got != "GORACE=halt_on_error=1" {
		t.Fatalf("CommandEnv() was mutated through returned slice: got %q", got)
	}
}
