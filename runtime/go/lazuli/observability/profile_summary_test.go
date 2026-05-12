package observability

import (
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestSafeProfileLabelsKeepsOnlyBoundedLazuliLabels(t *testing.T) {
	labels := map[string]string{
		ProfileLabelFeature:        " customer\nadmin ",
		ProfileLabelKind:           " command ",
		opLabelNameKey:             " create\tcustomer ",
		ProfileLabelSource:         " features/customer.lzi:42\n:1 ",
		ProfileLabelPatternID:      " command_pgx_insert ",
		ProfileLabelPatternVersion: strings.Repeat("v", 140),
		"tenant":                   "acme",
		" ":                        "ignored",
	}

	got := SafeProfileLabels(labels)

	want := map[string]string{
		ProfileLabelFeature:        "customer admin",
		ProfileLabelKind:           "command",
		ProfileLabelOp:             "create customer",
		ProfileLabelSource:         "features/customer.lzi:42 :1",
		ProfileLabelPatternID:      "command_pgx_insert",
		ProfileLabelPatternVersion: strings.Repeat("v", 128),
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("SafeProfileLabels(...) = %#v, want %#v", got, want)
	}
	if _, ok := got["tenant"]; ok {
		t.Fatalf("SafeProfileLabels kept unknown label: %#v", got)
	}

	labels[ProfileLabelFeature] = "mutated"
	if got[ProfileLabelFeature] != "customer admin" {
		t.Fatalf("safe labels changed after input mutation: %#v", got)
	}
}

func TestSummarizeProfileCPUAppliesMetadataLabelsAndBudget(t *testing.T) {
	capturedAt := time.Date(2026, 5, 12, 18, 0, 0, 0, time.UTC)

	summary := SummarizeProfile(ProfileSummaryInput{
		Kind:         "profile",
		Name:         " /debug/pprof/profile ",
		CapturedAt:   capturedAt,
		Duration:     30 * time.Second,
		PayloadBytes: 900,
		Labels: map[string]string{
			ProfileLabelFeature: "checkout",
			ProfileLabelKind:    "command",
			ProfileLabelOp:      "pay",
			"request_id":        "req-1",
		},
		CPU: CPUProfileMetadata{
			SampleCount:  5,
			SamplePeriod: 10 * time.Millisecond,
		},
		Budget: ProfileBudget{
			MaxPayloadBytes: 1000,
			MaxSamples:      10,
			MaxDuration:     time.Minute,
			MaxCPUTime:      40 * time.Millisecond,
		},
	})

	if summary.Kind != ProfileKindCPU {
		t.Fatalf("Kind = %q, want %q", summary.Kind, ProfileKindCPU)
	}
	if summary.Name != "/debug/pprof/profile" {
		t.Fatalf("Name = %q, want trimmed pprof path", summary.Name)
	}
	if summary.CapturedAt != capturedAt {
		t.Fatalf("CapturedAt = %s, want %s", summary.CapturedAt, capturedAt)
	}
	if summary.SampleCount != 5 {
		t.Fatalf("SampleCount = %d, want CPU sample count", summary.SampleCount)
	}
	if summary.CPU == nil {
		t.Fatal("CPU metadata is nil")
	}
	if summary.CPU.TotalCPUTime != 50*time.Millisecond {
		t.Fatalf("TotalCPUTime = %s, want sample count * period", summary.CPU.TotalCPUTime)
	}
	wantLabels := map[string]string{
		ProfileLabelFeature: "checkout",
		ProfileLabelKind:    "command",
		ProfileLabelOp:      "pay",
	}
	if !reflect.DeepEqual(summary.Labels, wantLabels) {
		t.Fatalf("Labels = %#v, want %#v", summary.Labels, wantLabels)
	}
	wantBudget := ProfileBudgetClassification{
		Class:      ProfileBudgetExceeded,
		UsageRatio: 1.25,
		Reasons: []ProfileBudgetReason{
			ProfileBudgetReasonPayloadBytes,
			ProfileBudgetReasonCPUTime,
		},
	}
	if !reflect.DeepEqual(summary.Budget, wantBudget) {
		t.Fatalf("Budget = %#v, want %#v", summary.Budget, wantBudget)
	}
}

func TestSummarizeProfileMemoryClassifiesConfiguredBudgets(t *testing.T) {
	summary := SummarizeProfile(ProfileSummaryInput{
		Kind:         "heap",
		PayloadBytes: 200,
		Memory: MemoryProfileMetadata{
			SampleCount:  3,
			AllocBytes:   1500,
			AllocObjects: 12,
			InUseBytes:   800,
			InUseObjects: 6,
		},
		Budget: ProfileBudget{
			MaxPayloadBytes: 1000,
			MaxAllocBytes:   2000,
			MaxInUseBytes:   1000,
		},
	})

	if summary.Kind != ProfileKindMemory {
		t.Fatalf("Kind = %q, want %q", summary.Kind, ProfileKindMemory)
	}
	if summary.SampleCount != 3 {
		t.Fatalf("SampleCount = %d, want memory sample count", summary.SampleCount)
	}
	if summary.Memory == nil || summary.Memory.InUseBytes != 800 {
		t.Fatalf("Memory = %#v, want in-use metadata", summary.Memory)
	}
	wantBudget := ProfileBudgetClassification{
		Class:      ProfileBudgetWarning,
		UsageRatio: 0.8,
		Reasons:    []ProfileBudgetReason{ProfileBudgetReasonInUseBytes},
	}
	if !reflect.DeepEqual(summary.Budget, wantBudget) {
		t.Fatalf("Budget = %#v, want %#v", summary.Budget, wantBudget)
	}
}

func TestSummarizeProfileGoroutineNormalizesStatesAndBudget(t *testing.T) {
	summary := SummarizeProfile(ProfileSummaryInput{
		Kind: "goroutines",
		Goroutine: GoroutineProfileMetadata{
			States: map[string]int{
				" running\nnow ": 2,
				"running now":    1,
				"chan receive":   4,
				"bad":            -10,
				" ":              1,
			},
		},
		Budget: ProfileBudget{
			MaxGoroutines: 5,
			WarningRatio:  0.9,
		},
	})

	if summary.Kind != ProfileKindGoroutine {
		t.Fatalf("Kind = %q, want %q", summary.Kind, ProfileKindGoroutine)
	}
	if summary.Goroutine == nil {
		t.Fatal("Goroutine metadata is nil")
	}
	wantStates := map[string]int{
		"running now":  3,
		"chan receive": 4,
	}
	if !reflect.DeepEqual(summary.Goroutine.States, wantStates) {
		t.Fatalf("States = %#v, want %#v", summary.Goroutine.States, wantStates)
	}
	if summary.Goroutine.Goroutines != 7 || summary.SampleCount != 7 {
		t.Fatalf("goroutines/sample count = %d/%d, want 7/7", summary.Goroutine.Goroutines, summary.SampleCount)
	}
	wantBudget := ProfileBudgetClassification{
		Class:      ProfileBudgetExceeded,
		UsageRatio: 1.4,
		Reasons:    []ProfileBudgetReason{ProfileBudgetReasonGoroutines},
	}
	if !reflect.DeepEqual(summary.Budget, wantBudget) {
		t.Fatalf("Budget = %#v, want %#v", summary.Budget, wantBudget)
	}
}

func TestClassifyProfileBudgetIgnoresUnsetLimits(t *testing.T) {
	summary := SummarizeProfile(ProfileSummaryInput{
		Kind:         ProfileKindCPU,
		Duration:     -time.Second,
		PayloadBytes: -10,
		SampleCount:  -2,
		CPU: CPUProfileMetadata{
			SampleCount:  -1,
			SamplePeriod: -time.Millisecond,
			TotalCPUTime: -time.Millisecond,
		},
	})

	if summary.Duration != 0 || summary.PayloadBytes != 0 || summary.SampleCount != 0 {
		t.Fatalf("negative values were not clamped: %#v", summary)
	}
	if summary.CPU == nil {
		t.Fatal("CPU metadata is nil")
	}
	if *summary.CPU != (CPUProfileMetadata{}) {
		t.Fatalf("CPU metadata = %#v, want zero metadata", *summary.CPU)
	}
	if !reflect.DeepEqual(summary.Budget, ProfileBudgetClassification{Class: ProfileBudgetOK}) {
		t.Fatalf("Budget = %#v, want ok", summary.Budget)
	}
}
