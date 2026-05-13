package deploy_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestBuildBlueGreenDeployPlanRendersText(t *testing.T) {
	plan, err := deploy.BuildBlueGreenDeployPlan(deploy.BlueGreenDeployConfig{
		Name:          "Checkout API",
		Environment:   "production",
		ActiveSlot:    "blue",
		CandidateSlot: "green",
		TrafficShifts: []deploy.BlueGreenTrafficShift{
			deploy.TrafficShift(20, "candidate readiness"),
			deploy.TrafficShift(100, "candidate readiness", "error budget"),
		},
		HealthGates: []deploy.BlueGreenHealthGate{
			deploy.HealthCheck("candidate readiness", "GET https://green.example.test/healthz returns 200."),
			deploy.HealthCheck("error budget", "Error rate remains below 1% for five minutes."),
		},
		RollbackHints: []deploy.BlueGreenRollbackHint{
			deploy.RollbackHint("pause migrations", "Keep backward-compatible migrations enabled until the rollback window closes."),
		},
	})
	if err != nil {
		t.Fatalf("BuildBlueGreenDeployPlan() error = %v", err)
	}

	got, err := plan.RenderText()
	if err != nil {
		t.Fatalf("RenderText() error = %v", err)
	}

	want := `Blue-green deploy: Checkout API
Environment: production
Dry run: true

Slots:
1. active: blue (100% traffic)
2. candidate: green (0% traffic)

Traffic phases:
1. warm candidate: blue 100%, green 0%; gates: candidate readiness, error budget
2. shift 20% traffic: blue 80%, green 20%; gates: candidate readiness
3. promote candidate: blue 0%, green 100%; gates: candidate readiness, error budget

Health gates:
1. candidate readiness: GET https://green.example.test/healthz returns 200.
2. error budget: Error rate remains below 1% for five minutes.

Rollback hints:
1. restore active slot: Route 100% traffic to blue and 0% to green.
2. hold candidate slot: Keep green deployed for logs and investigation while blue serves traffic.
3. pause migrations: Keep backward-compatible migrations enabled until the rollback window closes.
`
	if got != want {
		t.Fatalf("RenderText() mismatch\nwant:\n%s\ngot:\n%s", want, got)
	}
}

func TestBuildBlueGreenDeployPlanDefaultsAndCopiesInput(t *testing.T) {
	config := deploy.BlueGreenDeployConfig{
		ActiveSlot: "green",
		TrafficShifts: []deploy.BlueGreenTrafficShift{
			deploy.TrafficShift(25, "ready"),
		},
		HealthGates: []deploy.BlueGreenHealthGate{
			deploy.HealthCheck("ready", "Candidate answers readiness checks."),
		},
	}

	plan, err := deploy.BuildBlueGreenDeployPlan(config)
	if err != nil {
		t.Fatalf("BuildBlueGreenDeployPlan() error = %v", err)
	}

	if !plan.DryRun {
		t.Fatal("BuildBlueGreenDeployPlan() DryRun = false, want true")
	}
	if plan.Name != "release" {
		t.Fatalf("Name = %q, want release", plan.Name)
	}
	if plan.Environment != "production" {
		t.Fatalf("Environment = %q, want production", plan.Environment)
	}
	if plan.ActiveSlot.Name != "green" || plan.ActiveSlot.TrafficPercent != 100 {
		t.Fatalf("ActiveSlot = %#v, want green at 100%%", plan.ActiveSlot)
	}
	if plan.CandidateSlot.Name != "blue" || plan.CandidateSlot.TrafficPercent != 0 {
		t.Fatalf("CandidateSlot = %#v, want blue at 0%%", plan.CandidateSlot)
	}

	wantPercents := []int{0, 25, 100}
	if len(plan.TrafficPhases) != len(wantPercents) {
		t.Fatalf("TrafficPhases len = %d, want %d", len(plan.TrafficPhases), len(wantPercents))
	}
	for i, want := range wantPercents {
		if plan.TrafficPhases[i].CandidatePercent != want {
			t.Fatalf("TrafficPhases[%d].CandidatePercent = %d, want %d", i, plan.TrafficPhases[i].CandidatePercent, want)
		}
	}

	config.TrafficShifts[0].HealthGateNames[0] = "changed"
	config.HealthGates[0].Name = "changed"
	if plan.TrafficPhases[1].HealthGateNames[0] != "ready" {
		t.Fatalf("phase health gate changed after input mutation: %q", plan.TrafficPhases[1].HealthGateNames[0])
	}
	if plan.HealthGates[0].Name != "ready" {
		t.Fatalf("health gate changed after input mutation: %q", plan.HealthGates[0].Name)
	}
}

func TestValidateBlueGreenDeployConfigRejectsInvalidValues(t *testing.T) {
	tests := []struct {
		name     string
		config   deploy.BlueGreenDeployConfig
		fragment string
	}{
		{
			name: "same slots",
			config: deploy.BlueGreenDeployConfig{
				ActiveSlot:    "blue",
				CandidateSlot: "BLUE",
			},
			fragment: "candidate_slot.name",
		},
		{
			name: "invalid active slot",
			config: deploy.BlueGreenDeployConfig{
				ActiveSlot: "bad slot",
			},
			fragment: "active_slot.name",
		},
		{
			name: "invalid traffic percent",
			config: deploy.BlueGreenDeployConfig{
				TrafficShifts: []deploy.BlueGreenTrafficShift{
					deploy.TrafficShift(101),
				},
			},
			fragment: "candidate_percent",
		},
		{
			name: "decreasing traffic phases",
			config: deploy.BlueGreenDeployConfig{
				TrafficShifts: []deploy.BlueGreenTrafficShift{
					deploy.TrafficShift(50),
					deploy.TrafficShift(25),
				},
			},
			fragment: "must increase",
		},
		{
			name: "unknown health gate",
			config: deploy.BlueGreenDeployConfig{
				TrafficShifts: []deploy.BlueGreenTrafficShift{
					deploy.TrafficShift(10, "missing"),
				},
			},
			fragment: "unknown health gate",
		},
		{
			name: "duplicate health gate",
			config: deploy.BlueGreenDeployConfig{
				HealthGates: []deploy.BlueGreenHealthGate{
					deploy.HealthCheck("ready", "GET /ready returns 200."),
					deploy.HealthCheck("READY", "Duplicate."),
				},
			},
			fragment: "health_gates[1].name",
		},
		{
			name: "empty rollback hint",
			config: deploy.BlueGreenDeployConfig{
				RollbackHints: []deploy.BlueGreenRollbackHint{
					deploy.RollbackHint("pause workers", ""),
				},
			},
			fragment: "rollback_hints[2].hint",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := deploy.ValidateBlueGreenDeployConfig(tt.config)
			if !errors.Is(err, deploy.ErrInvalidBlueGreenPlan) {
				t.Fatalf("ValidateBlueGreenDeployConfig() error = %v, want ErrInvalidBlueGreenPlan", err)
			}
			if !strings.Contains(err.Error(), tt.fragment) {
				t.Fatalf("ValidateBlueGreenDeployConfig() error = %v, want fragment %q", err, tt.fragment)
			}
		})
	}
}

func TestValidateBlueGreenDeployPlanRejectsNonDryRunPlan(t *testing.T) {
	err := deploy.ValidateBlueGreenDeployPlan(deploy.BlueGreenDeployPlan{
		Name:        "Checkout API",
		Environment: "production",
		ActiveSlot: deploy.BlueGreenSlot{
			Name:           "blue",
			TrafficPercent: 100,
		},
		CandidateSlot: deploy.BlueGreenSlot{
			Name:           "green",
			TrafficPercent: 0,
		},
		TrafficPhases: []deploy.BlueGreenTrafficPhase{
			{Name: "warm candidate", ActivePercent: 100, CandidatePercent: 0},
			{Name: "promote candidate", ActivePercent: 0, CandidatePercent: 100},
		},
		RollbackHints: []deploy.BlueGreenRollbackHint{
			deploy.RollbackHint("restore active slot", "Route traffic back to blue."),
		},
	})
	if !errors.Is(err, deploy.ErrInvalidBlueGreenPlan) {
		t.Fatalf("ValidateBlueGreenDeployPlan() error = %v, want ErrInvalidBlueGreenPlan", err)
	}
	if !strings.Contains(err.Error(), "dry_run") {
		t.Fatalf("ValidateBlueGreenDeployPlan() error = %v, want dry_run fragment", err)
	}
}
