package testkit_test

import (
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/testkit"
)

var allocationTestSink any

func TestSampleAllocationsReportsStats(t *testing.T) {
	stats := testkit.SampleAllocations(testkit.AllocationSampling{
		Samples:            3,
		RunsPerSample:      5,
		WarmupRuns:         1,
		GCBeforeEachSample: true,
	}, func() {
		allocationTestSink = make([]byte, 32)
	})

	if stats.SampleCount != 3 || stats.RunsPerSample != 5 || len(stats.Samples) != 3 {
		t.Fatalf("stats sample shape = %#v, want 3 samples of 5 runs", stats)
	}
	if stats.AllocsPerRun.Samples != 3 || stats.BytesPerRun.Samples != 3 {
		t.Fatalf("sample stats counts = allocs %d bytes %d, want 3", stats.AllocsPerRun.Samples, stats.BytesPerRun.Samples)
	}
	if stats.AllocsPerRun.Min <= 0 || stats.AllocsPerRun.Mean <= 0 {
		t.Fatalf("allocs/run stats = %#v, want positive allocations", stats.AllocsPerRun)
	}
	if stats.BytesPerRun.Min <= 0 || stats.BytesPerRun.Mean <= 0 {
		t.Fatalf("bytes/run stats = %#v, want positive bytes", stats.BytesPerRun)
	}
	if stats.AllocsPerRun.Max < stats.AllocsPerRun.Min {
		t.Fatalf("allocs/run max %f before min %f", stats.AllocsPerRun.Max, stats.AllocsPerRun.Min)
	}
}

func TestEvaluateAllocationBudgetChecksAbsoluteLimits(t *testing.T) {
	stats := allocationStatsForTest(3, 128, 4, 160)
	budget := testkit.AllocationBudget{
		Name:         "widgets.create",
		Policy:       testkit.AllocationBudgetMean,
		AllocsPerRun: testkit.AllocationLimitMax(2),
		BytesPerRun:  testkit.AllocationLimitMax(100),
	}

	report := testkit.EvaluateAllocationBudget(stats, budget)

	if report.OK() {
		t.Fatal("EvaluateAllocationBudget() passed, want absolute limit failures")
	}
	if len(report.Failures) != 2 {
		t.Fatalf("failures = %d, want 2: %#v", len(report.Failures), report.Failures)
	}
	message := report.Error()
	assertContains(t, message, `allocation budget "widgets.create" failed`)
	assertContains(t, message, "policy mean, samples 2 x 10 runs")
	assertContains(t, message, "allocs/run mean 3 > max 2")
	assertContains(t, message, "bytes/run mean 128 B > max 100 B")
	assertContains(t, message, "min 2")
	assertContains(t, message, "max 4")
}

func TestAllocationBudgetPolicySelectsMaxSample(t *testing.T) {
	stats := allocationStatsForTest(1, 64, 5, 96)
	budget := testkit.AllocationBudget{
		Policy:       testkit.AllocationBudgetMax,
		AllocsPerRun: testkit.AllocationLimitMax(2),
	}

	err := testkit.CheckAllocationBudget(stats, budget)

	if err == nil {
		t.Fatal("CheckAllocationBudget() passed, want max policy failure")
	}
	assertContains(t, err.Error(), "allocs/run max 5 > max 2")
}

func TestEvaluateAllocationBudgetDetectsRegression(t *testing.T) {
	stats := allocationStatsForTest(7, 200, 7, 200)
	budget := testkit.AllocationBudget{
		Name:   "checkout.render",
		Policy: testkit.AllocationBudgetMean,
		Regression: &testkit.AllocationRegressionBudget{
			AllocsPerRun: testkit.AllocationRegressionLimitFor(5, 1, 0.10),
			BytesPerRun:  testkit.AllocationRegressionLimitFor(100, 0, 0.50),
		},
	}

	report := testkit.EvaluateAllocationBudget(stats, budget)

	if report.OK() {
		t.Fatal("EvaluateAllocationBudget() passed, want regression failures")
	}
	if len(report.Failures) != 2 {
		t.Fatalf("failures = %d, want 2: %#v", len(report.Failures), report.Failures)
	}
	message := report.Error()
	assertContains(t, message, `allocation budget "checkout.render" failed`)
	assertContains(t, message, "allocs/run mean 7 regressed from baseline 5 by 2")
	assertContains(t, message, "allowed 1, limit 6")
	assertContains(t, message, "bytes/run mean 200 B regressed from baseline 100 B by 100 B")
	assertContains(t, message, "allowed 50 B, limit 150 B")
}

func TestAssertAllocationsReturnsStats(t *testing.T) {
	stats := testkit.AssertAllocations(t,
		testkit.AllocationSampling{Samples: 2, RunsPerSample: 3},
		testkit.AllocationBudget{
			Policy:       testkit.AllocationBudgetMax,
			AllocsPerRun: testkit.AllocationLimitMax(100),
			BytesPerRun:  testkit.AllocationLimitMax(64 * 1024),
		},
		func() {
			allocationTestSink = []byte("ok")
		},
	)

	if stats.SampleCount != 2 || stats.RunsPerSample != 3 {
		t.Fatalf("AssertAllocations() stats = %#v, want configured sampling", stats)
	}
}

func allocationStatsForTest(meanAllocs, meanBytes, maxAllocs, maxBytes float64) testkit.AllocationStats {
	return testkit.AllocationStats{
		SampleCount:   2,
		RunsPerSample: 10,
		AllocsPerRun: testkit.AllocationSampleStats{
			Samples: 2,
			Min:     meanAllocs - 1,
			Median:  meanAllocs,
			Mean:    meanAllocs,
			Max:     maxAllocs,
		},
		BytesPerRun: testkit.AllocationSampleStats{
			Samples: 2,
			Min:     meanBytes - 16,
			Median:  meanBytes,
			Mean:    meanBytes,
			Max:     maxBytes,
		},
	}
}

func assertContains(t *testing.T, got, want string) {
	t.Helper()
	if !strings.Contains(got, want) {
		t.Fatalf("message %q does not contain %q", got, want)
	}
}
