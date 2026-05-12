package deploy_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestBuildBlueGreenReleasePlanRendersMarkdown(t *testing.T) {
	plan, err := deploy.BuildBlueGreenReleasePlan(deploy.BlueGreenReleaseConfig{
		Name:           "Checkout API",
		Version:        "2026.05.12",
		Environment:    "production",
		CurrentColor:   "blue",
		CandidateColor: "green",
		MigrationGates: []deploy.MigrationGate{
			deploy.Gate("schema compatibility", "Confirm migrations are backward compatible with blue."),
		},
		Validations: []deploy.ReleaseValidation{
			deploy.Validation("health | smoke", "GET /healthz returns 200."),
		},
		Rollback: []deploy.RollbackAction{
			deploy.Rollback("revert migration flag", "Disable forward-only migration feature flags."),
		},
	})
	if err != nil {
		t.Fatalf("BuildBlueGreenReleasePlan() error = %v", err)
	}

	got, err := plan.RenderMarkdown()
	if err != nil {
		t.Fatalf("RenderMarkdown() error = %v", err)
	}

	want := `# Release: Checkout API

| Field | Value |
| --- | --- |
| Version | 2026.05.12 |
| Environment | production |
| Strategy | blue-green |

## Steps

| # | Name | Action |
| --- | --- | --- |
| 1 | prepare green | Deploy version 2026.05.12 to green in production with traffic disabled. |
| 2 | run migration gates | Confirm migration gates pass before routing traffic. |
| 3 | warm green | Warm green and run readiness checks before promotion. |
| 4 | shift traffic | Route traffic from blue to green. |
| 5 | validate green | Run release validation checks against green. |
| 6 | hold blue | Keep blue available until the rollback window closes. |

## Migration Gates

| # | Name | Check |
| --- | --- | --- |
| 1 | schema compatibility | Confirm migrations are backward compatible with blue. |

## Validation Checks

| # | Name | Check |
| --- | --- | --- |
| 1 | health \| smoke | GET /healthz returns 200. |

## Rollback Actions

| # | Name | Action |
| --- | --- | --- |
| 1 | route traffic to blue | Route all traffic back to blue. |
| 2 | hold green | Keep green deployed for logs and investigation while serving traffic from blue. |
| 3 | revert migration flag | Disable forward-only migration feature flags. |
`
	if got != want {
		t.Fatalf("RenderMarkdown() mismatch\nwant:\n%s\ngot:\n%s", want, got)
	}
}

func TestBuildCanaryReleasePlanRendersTextWithSortedPercentages(t *testing.T) {
	plan, err := deploy.BuildCanaryReleasePlan(deploy.CanaryReleaseConfig{
		Name:        "Checkout API",
		Version:     "2026.05.12",
		Environment: "production",
		Percentages: []int{50, 10, 10},
		MigrationGates: []deploy.MigrationGate{
			deploy.Gate("schema lock", "Ensure no blocking schema locks are active."),
		},
		Validations: []deploy.ReleaseValidation{
			deploy.Validation("error budget", "Error rate remains below threshold."),
		},
		Rollback: []deploy.RollbackAction{
			deploy.Rollback("pause workers", "Pause background workers before retrying."),
		},
	})
	if err != nil {
		t.Fatalf("BuildCanaryReleasePlan() error = %v", err)
	}

	got, err := deploy.RenderReleasePlanText(plan)
	if err != nil {
		t.Fatalf("RenderReleasePlanText() error = %v", err)
	}

	want := `Release: Checkout API
Version: 2026.05.12
Environment: production
Strategy: canary

Steps:
1. prepare canary: Deploy version 2026.05.12 to canary targets in production with 0% traffic.
2. run migration gates: Confirm migration gates pass before increasing canary traffic.
3. shift 10% traffic: Route 10% traffic to version 2026.05.12.
4. validate 10% canary: Run release validation checks before increasing canary traffic above 10%.
5. shift 50% traffic: Route 50% traffic to version 2026.05.12.
6. validate 50% canary: Run release validation checks before increasing canary traffic above 50%.
7. promote 100% traffic: Route 100% traffic to version 2026.05.12.

Migration gates:
1. schema lock: Ensure no blocking schema locks are active.

Validation checks:
1. error budget: Error rate remains below threshold.

Rollback actions:
1. set canary to 0%: Route 0% traffic to version 2026.05.12.
2. restore stable version: Keep the previous stable version serving all traffic.
3. pause workers: Pause background workers before retrying.
`
	if got != want {
		t.Fatalf("RenderReleasePlanText() mismatch\nwant:\n%s\ngot:\n%s", want, got)
	}
}

func TestValidateReleasePlanRejectsInvalidMetadataAndSections(t *testing.T) {
	err := deploy.ValidateReleasePlan(deploy.ReleasePlan{
		Name:        "Checkout API",
		Environment: "production",
		Strategy:    "rolling",
		Steps: []deploy.ReleaseStep{
			deploy.Step("deploy", "Deploy the candidate."),
			deploy.Step("deploy", "Duplicate step name."),
		},
		MigrationGates: []deploy.MigrationGate{
			deploy.Gate("schema", ""),
		},
		Validations: []deploy.ReleaseValidation{
			deploy.Validation("smoke", "GET /healthz returns 200."),
			deploy.Validation("SMOKE", "Duplicate validation name."),
		},
	})
	if !errors.Is(err, deploy.ErrInvalidReleasePlan) {
		t.Fatalf("ValidateReleasePlan() error = %v, want ErrInvalidReleasePlan", err)
	}

	for _, fragment := range []string{
		"version",
		"strategy",
		"steps[1].name",
		"migration_gates[0].check",
		"validations[1].name",
		"rollback",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateReleasePlan() error = %v, want fragment %q", err, fragment)
		}
	}
}

func TestBuildCanaryReleasePlanRejectsInvalidPercentages(t *testing.T) {
	_, err := deploy.BuildCanaryReleasePlan(deploy.CanaryReleaseConfig{
		Version:     "2026.05.12",
		Percentages: []int{0, 101},
	})
	if !errors.Is(err, deploy.ErrInvalidReleasePlan) {
		t.Fatalf("BuildCanaryReleasePlan() error = %v, want ErrInvalidReleasePlan", err)
	}
	for _, fragment := range []string{"canary.percentages[0]", "canary.percentages[1]"} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("BuildCanaryReleasePlan() error = %v, want fragment %q", err, fragment)
		}
	}
}
