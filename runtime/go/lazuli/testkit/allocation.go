package testkit

import (
	"math"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"testing"
)

const (
	// DefaultAllocationSamples is the sample count used when AllocationSampling
	// leaves Samples unset.
	DefaultAllocationSamples = 5
	// DefaultAllocationRunsPerSample is the per-sample run count used when
	// AllocationSampling leaves RunsPerSample unset.
	DefaultAllocationRunsPerSample = 100
)

// AllocationSampling configures allocation sampling. Each sample runs the
// target function RunsPerSample times and records malloc count and allocated
// bytes per run.
type AllocationSampling struct {
	// Samples is the number of independent samples to capture. Non-positive
	// values use DefaultAllocationSamples.
	Samples int
	// RunsPerSample is the number of target function calls in each sample.
	// Non-positive values use DefaultAllocationRunsPerSample.
	RunsPerSample int
	// WarmupRuns calls the target function before sampling starts. Negative
	// values are treated as zero.
	WarmupRuns int
	// GCBeforeEachSample runs a full GC immediately before each sample's memory
	// counters are read. This can reduce noise from prior tests, but also makes
	// sampling slower.
	GCBeforeEachSample bool
}

// AllocationSample is one allocation measurement sample.
type AllocationSample struct {
	Runs         int
	Allocs       uint64
	Bytes        uint64
	AllocsPerRun float64
	BytesPerRun  float64
}

// AllocationSampleStats summarizes a set of sample values.
type AllocationSampleStats struct {
	Samples int
	Min     float64
	Max     float64
	Mean    float64
	Median  float64
}

// Value returns the sample statistic selected by policy. Unknown policies use
// AllocationBudgetMean.
func (s AllocationSampleStats) Value(policy AllocationBudgetPolicy) float64 {
	switch normalizeAllocationBudgetPolicy(policy) {
	case AllocationBudgetMin:
		return s.Min
	case AllocationBudgetMax:
		return s.Max
	case AllocationBudgetMedian:
		return s.Median
	default:
		return s.Mean
	}
}

// AllocationStats contains allocation samples and summary statistics.
type AllocationStats struct {
	SampleCount   int
	RunsPerSample int
	Samples       []AllocationSample
	AllocsPerRun  AllocationSampleStats
	BytesPerRun   AllocationSampleStats
}

// Value returns the metric statistic selected by policy. Unknown metrics use
// AllocationMetricAllocsPerRun.
func (s AllocationStats) Value(metric AllocationMetric, policy AllocationBudgetPolicy) float64 {
	switch metric {
	case AllocationMetricBytesPerRun:
		return s.BytesPerRun.Value(policy)
	default:
		return s.AllocsPerRun.Value(policy)
	}
}

// AllocationMetric identifies a measured allocation dimension.
type AllocationMetric string

const (
	AllocationMetricAllocsPerRun AllocationMetric = "allocs/run"
	AllocationMetricBytesPerRun  AllocationMetric = "bytes/run"
)

// AllocationBudgetPolicy selects which sample statistic is compared with a
// budget. The zero value uses AllocationBudgetMean.
type AllocationBudgetPolicy string

const (
	AllocationBudgetMean   AllocationBudgetPolicy = "mean"
	AllocationBudgetMedian AllocationBudgetPolicy = "median"
	AllocationBudgetMin    AllocationBudgetPolicy = "min"
	AllocationBudgetMax    AllocationBudgetPolicy = "max"
)

// AllocationLimit is an optional absolute budget for one metric. Use
// AllocationLimitMax to enable the limit, including a valid max of zero.
type AllocationLimit struct {
	Enabled bool
	Max     float64
}

// AllocationLimitMax returns an enabled absolute allocation limit. Negative
// and NaN values are treated as zero.
func AllocationLimitMax(max float64) AllocationLimit {
	return AllocationLimit{
		Enabled: true,
		Max:     nonNegativeAllocationFloat(max),
	}
}

// AllocationRegressionLimit is an optional regression budget for one metric.
// Actual values may exceed Baseline by the larger of MaxIncrease and
// Baseline*MaxIncreaseRatio. Set Enabled to true to check the metric.
type AllocationRegressionLimit struct {
	Enabled          bool
	Baseline         float64
	MaxIncrease      float64
	MaxIncreaseRatio float64
}

// AllocationRegressionLimitFor returns an enabled regression limit. Ratio is a
// fractional increase, so 0.10 allows a 10% increase over the baseline.
func AllocationRegressionLimitFor(baseline, maxIncrease, maxIncreaseRatio float64) AllocationRegressionLimit {
	return AllocationRegressionLimit{
		Enabled:          true,
		Baseline:         nonNegativeAllocationFloat(baseline),
		MaxIncrease:      nonNegativeAllocationFloat(maxIncrease),
		MaxIncreaseRatio: nonNegativeAllocationFloat(maxIncreaseRatio),
	}
}

// AllocationRegressionBudget contains regression limits for allocation metrics.
type AllocationRegressionBudget struct {
	AllocsPerRun AllocationRegressionLimit
	BytesPerRun  AllocationRegressionLimit
}

// AllocationBudget describes absolute and regression budgets for sampled
// allocations. Zero-value limits are disabled unless their Enabled field is set.
type AllocationBudget struct {
	Name         string
	Policy       AllocationBudgetPolicy
	AllocsPerRun AllocationLimit
	BytesPerRun  AllocationLimit
	Regression   *AllocationRegressionBudget
}

// AllocationBudgetFailureKind describes why a budget check failed.
type AllocationBudgetFailureKind string

const (
	AllocationBudgetFailureLimit      AllocationBudgetFailureKind = "limit"
	AllocationBudgetFailureRegression AllocationBudgetFailureKind = "regression"
)

// AllocationBudgetFailure is one failed budget comparison.
type AllocationBudgetFailure struct {
	Metric          AllocationMetric
	Kind            AllocationBudgetFailureKind
	Policy          AllocationBudgetPolicy
	Actual          float64
	Limit           float64
	Baseline        float64
	AllowedIncrease float64
}

// AllocationBudgetReport is the structured result of evaluating allocation
// stats against a budget. It implements error with a human-readable failure
// message.
type AllocationBudgetReport struct {
	Stats    AllocationStats
	Budget   AllocationBudget
	Policy   AllocationBudgetPolicy
	Failures []AllocationBudgetFailure
}

// OK reports whether all configured budget checks passed.
func (r AllocationBudgetReport) OK() bool {
	return len(r.Failures) == 0
}

// Error formats the budget result for assertion failures.
func (r AllocationBudgetReport) Error() string {
	var b strings.Builder
	if name := strings.TrimSpace(r.Budget.Name); name != "" {
		b.WriteString("allocation budget ")
		b.WriteString(strconv.Quote(name))
	} else {
		b.WriteString("allocation budget")
	}

	if r.OK() {
		b.WriteString(" ok")
		b.WriteString(r.formatContext())
		return b.String()
	}

	b.WriteString(" failed")
	b.WriteString(r.formatContext())
	b.WriteString(": ")
	for i, failure := range r.Failures {
		if i > 0 {
			b.WriteString("; ")
		}
		b.WriteString(r.formatFailure(failure))
	}
	return b.String()
}

// SampleAllocations measures malloc count and allocated bytes for fn.
func SampleAllocations(sampling AllocationSampling, fn func()) AllocationStats {
	if fn == nil {
		panic("testkit: nil allocation sample function")
	}

	sampling = normalizeAllocationSampling(sampling)
	for i := 0; i < sampling.WarmupRuns; i++ {
		fn()
	}

	samples := make([]AllocationSample, sampling.Samples)
	allocValues := make([]float64, sampling.Samples)
	byteValues := make([]float64, sampling.Samples)
	for i := range samples {
		if sampling.GCBeforeEachSample {
			runtime.GC()
		}
		sample := sampleAllocationRun(sampling.RunsPerSample, fn)
		samples[i] = sample
		allocValues[i] = sample.AllocsPerRun
		byteValues[i] = sample.BytesPerRun
	}

	return AllocationStats{
		SampleCount:   sampling.Samples,
		RunsPerSample: sampling.RunsPerSample,
		Samples:       samples,
		AllocsPerRun:  summarizeAllocationValues(allocValues),
		BytesPerRun:   summarizeAllocationValues(byteValues),
	}
}

// EvaluateAllocationBudget compares stats against budget and returns a
// structured report.
func EvaluateAllocationBudget(stats AllocationStats, budget AllocationBudget) AllocationBudgetReport {
	policy := normalizeAllocationBudgetPolicy(budget.Policy)
	report := AllocationBudgetReport{
		Stats:  stats,
		Budget: budget,
		Policy: policy,
	}

	report.checkLimit(AllocationMetricAllocsPerRun, budget.AllocsPerRun)
	report.checkLimit(AllocationMetricBytesPerRun, budget.BytesPerRun)
	if budget.Regression != nil {
		report.checkRegression(AllocationMetricAllocsPerRun, budget.Regression.AllocsPerRun)
		report.checkRegression(AllocationMetricBytesPerRun, budget.Regression.BytesPerRun)
	}

	return report
}

// CheckAllocationBudget returns nil when stats satisfy budget, or a
// human-readable AllocationBudgetReport error when they do not.
func CheckAllocationBudget(stats AllocationStats, budget AllocationBudget) error {
	report := EvaluateAllocationBudget(stats, budget)
	if report.OK() {
		return nil
	}
	return report
}

// AssertAllocationBudget fails t when stats do not satisfy budget.
func AssertAllocationBudget(t testing.TB, stats AllocationStats, budget AllocationBudget) {
	t.Helper()
	if err := CheckAllocationBudget(stats, budget); err != nil {
		t.Fatal(err)
	}
}

// AssertAllocations samples fn and fails t when the sampled stats do not
// satisfy budget. The sampled stats are returned for additional test-specific
// checks or logging.
func AssertAllocations(t testing.TB, sampling AllocationSampling, budget AllocationBudget, fn func()) AllocationStats {
	t.Helper()
	if fn == nil {
		t.Fatal("testkit: nil allocation sample function")
	}
	stats := SampleAllocations(sampling, fn)
	AssertAllocationBudget(t, stats, budget)
	return stats
}

func normalizeAllocationSampling(sampling AllocationSampling) AllocationSampling {
	if sampling.Samples <= 0 {
		sampling.Samples = DefaultAllocationSamples
	}
	if sampling.RunsPerSample <= 0 {
		sampling.RunsPerSample = DefaultAllocationRunsPerSample
	}
	if sampling.WarmupRuns < 0 {
		sampling.WarmupRuns = 0
	}
	return sampling
}

func sampleAllocationRun(runs int, fn func()) AllocationSample {
	var before, after runtime.MemStats
	runtime.ReadMemStats(&before)
	for i := 0; i < runs; i++ {
		fn()
	}
	runtime.ReadMemStats(&after)

	allocs := allocationCounterDelta(after.Mallocs, before.Mallocs)
	bytes := allocationCounterDelta(after.TotalAlloc, before.TotalAlloc)
	return AllocationSample{
		Runs:         runs,
		Allocs:       allocs,
		Bytes:        bytes,
		AllocsPerRun: float64(allocs) / float64(runs),
		BytesPerRun:  float64(bytes) / float64(runs),
	}
}

func allocationCounterDelta(after, before uint64) uint64 {
	if after < before {
		return 0
	}
	return after - before
}

func summarizeAllocationValues(values []float64) AllocationSampleStats {
	if len(values) == 0 {
		return AllocationSampleStats{}
	}

	sorted := append([]float64(nil), values...)
	sort.Float64s(sorted)
	sum := 0.0
	for _, value := range sorted {
		sum += value
	}

	median := sorted[len(sorted)/2]
	if len(sorted)%2 == 0 {
		median = (sorted[len(sorted)/2-1] + sorted[len(sorted)/2]) / 2
	}

	return AllocationSampleStats{
		Samples: len(sorted),
		Min:     sorted[0],
		Max:     sorted[len(sorted)-1],
		Mean:    sum / float64(len(sorted)),
		Median:  median,
	}
}

func normalizeAllocationBudgetPolicy(policy AllocationBudgetPolicy) AllocationBudgetPolicy {
	switch policy {
	case AllocationBudgetMin, AllocationBudgetMax, AllocationBudgetMedian:
		return policy
	default:
		return AllocationBudgetMean
	}
}

func (r *AllocationBudgetReport) checkLimit(metric AllocationMetric, limit AllocationLimit) {
	if !limit.Enabled {
		return
	}

	actual := nonNegativeAllocationFloat(r.Stats.Value(metric, r.Policy))
	max := nonNegativeAllocationFloat(limit.Max)
	if actual <= max {
		return
	}
	r.Failures = append(r.Failures, AllocationBudgetFailure{
		Metric: metric,
		Kind:   AllocationBudgetFailureLimit,
		Policy: r.Policy,
		Actual: actual,
		Limit:  max,
	})
}

func (r *AllocationBudgetReport) checkRegression(metric AllocationMetric, limit AllocationRegressionLimit) {
	if !limit.Enabled {
		return
	}

	actual := nonNegativeAllocationFloat(r.Stats.Value(metric, r.Policy))
	baseline := nonNegativeAllocationFloat(limit.Baseline)
	allowedIncrease := allocationRegressionAllowedIncrease(baseline, limit)
	max := baseline + allowedIncrease
	if actual <= max {
		return
	}
	r.Failures = append(r.Failures, AllocationBudgetFailure{
		Metric:          metric,
		Kind:            AllocationBudgetFailureRegression,
		Policy:          r.Policy,
		Actual:          actual,
		Limit:           max,
		Baseline:        baseline,
		AllowedIncrease: allowedIncrease,
	})
}

func allocationRegressionAllowedIncrease(baseline float64, limit AllocationRegressionLimit) float64 {
	maxIncrease := nonNegativeAllocationFloat(limit.MaxIncrease)
	ratioIncrease := baseline * nonNegativeAllocationFloat(limit.MaxIncreaseRatio)
	if ratioIncrease > maxIncrease {
		return ratioIncrease
	}
	return maxIncrease
}

func (r AllocationBudgetReport) formatContext() string {
	var b strings.Builder
	b.WriteString(" (policy ")
	b.WriteString(string(r.Policy))
	if r.Stats.SampleCount > 0 || r.Stats.RunsPerSample > 0 {
		b.WriteString(", samples ")
		b.WriteString(strconv.Itoa(r.Stats.SampleCount))
		b.WriteString(" x ")
		b.WriteString(strconv.Itoa(r.Stats.RunsPerSample))
		b.WriteString(" runs")
	}
	b.WriteByte(')')
	return b.String()
}

func (r AllocationBudgetReport) formatFailure(failure AllocationBudgetFailure) string {
	var b strings.Builder
	b.WriteString(string(failure.Metric))
	b.WriteByte(' ')
	b.WriteString(string(failure.Policy))
	b.WriteByte(' ')
	b.WriteString(formatAllocationMetricValue(failure.Metric, failure.Actual))

	switch failure.Kind {
	case AllocationBudgetFailureRegression:
		increase := failure.Actual - failure.Baseline
		b.WriteString(" regressed from baseline ")
		b.WriteString(formatAllocationMetricValue(failure.Metric, failure.Baseline))
		b.WriteString(" by ")
		b.WriteString(formatAllocationMetricValue(failure.Metric, increase))
		if failure.Baseline > 0 {
			b.WriteString(" (")
			b.WriteString(formatAllocationPercent(increase / failure.Baseline))
			b.WriteString(" increase, allowed ")
		} else {
			b.WriteString(" (allowed ")
		}
		b.WriteString(formatAllocationMetricValue(failure.Metric, failure.AllowedIncrease))
		b.WriteString(", limit ")
		b.WriteString(formatAllocationMetricValue(failure.Metric, failure.Limit))
		b.WriteByte(')')
	default:
		b.WriteString(" > max ")
		b.WriteString(formatAllocationMetricValue(failure.Metric, failure.Limit))
	}

	if stats := r.metricStats(failure.Metric); stats.Samples > 0 {
		b.WriteString(" [")
		b.WriteString(formatAllocationSampleStats(failure.Metric, stats))
		b.WriteByte(']')
	}
	return b.String()
}

func (r AllocationBudgetReport) metricStats(metric AllocationMetric) AllocationSampleStats {
	switch metric {
	case AllocationMetricBytesPerRun:
		return r.Stats.BytesPerRun
	default:
		return r.Stats.AllocsPerRun
	}
}

func formatAllocationSampleStats(metric AllocationMetric, stats AllocationSampleStats) string {
	parts := []string{
		"min " + formatAllocationMetricValue(metric, stats.Min),
		"median " + formatAllocationMetricValue(metric, stats.Median),
		"mean " + formatAllocationMetricValue(metric, stats.Mean),
		"max " + formatAllocationMetricValue(metric, stats.Max),
	}
	return strings.Join(parts, ", ")
}

func formatAllocationMetricValue(metric AllocationMetric, value float64) string {
	value = nonNegativeAllocationFloat(value)
	if metric == AllocationMetricBytesPerRun {
		return formatAllocationBytes(value)
	}
	return formatAllocationFloat(value)
}

func formatAllocationBytes(value float64) string {
	const unit = 1024
	switch {
	case value < unit:
		return formatAllocationFloat(value) + " B"
	case value < unit*unit:
		return formatAllocationFloat(value/unit) + " KiB"
	case value < unit*unit*unit:
		return formatAllocationFloat(value/(unit*unit)) + " MiB"
	default:
		return formatAllocationFloat(value/(unit*unit*unit)) + " GiB"
	}
}

func formatAllocationFloat(value float64) string {
	value = nonNegativeAllocationFloat(value)
	if math.IsInf(value, 1) {
		return "+Inf"
	}
	rounded := math.Round(value)
	if math.Abs(value-rounded) < 0.005 {
		return strconv.FormatInt(int64(rounded), 10)
	}
	return strconv.FormatFloat(value, 'f', 2, 64)
}

func formatAllocationPercent(ratio float64) string {
	ratio = nonNegativeAllocationFloat(ratio)
	return formatAllocationFloat(ratio*100) + "%"
}

func nonNegativeAllocationFloat(value float64) float64 {
	if math.IsNaN(value) || value < 0 {
		return 0
	}
	return value
}
