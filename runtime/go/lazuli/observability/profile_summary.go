package observability

import (
	"strings"
	"time"
	"unicode/utf8"
)

const (
	profileSummaryDefaultWarningRatio = 0.8
	profileSummaryMaxLabelValueBytes  = 128
	profileSummaryMaxSourceValueBytes = 256
)

// ProfileKind names the pprof profile family represented by a summary.
type ProfileKind string

const (
	// ProfileKindCPU summarizes CPU profiles such as /debug/pprof/profile.
	ProfileKindCPU ProfileKind = "cpu"
	// ProfileKindMemory summarizes heap, allocs, or memory profiles.
	ProfileKindMemory ProfileKind = "memory"
	// ProfileKindGoroutine summarizes goroutine profiles.
	ProfileKindGoroutine ProfileKind = "goroutine"
)

// ProfileBudgetClass is the severity assigned after comparing a profile
// summary with caller-provided report budgets.
type ProfileBudgetClass string

const (
	// ProfileBudgetOK means all configured budgets are below their warning threshold.
	ProfileBudgetOK ProfileBudgetClass = "ok"
	// ProfileBudgetWarning means at least one configured budget is near its limit.
	ProfileBudgetWarning ProfileBudgetClass = "warning"
	// ProfileBudgetExceeded means at least one configured budget was exceeded.
	ProfileBudgetExceeded ProfileBudgetClass = "exceeded"
)

// ProfileBudgetReason identifies the profile dimension that triggered a budget
// warning or exceedance.
type ProfileBudgetReason string

const (
	ProfileBudgetReasonPayloadBytes ProfileBudgetReason = "payload_bytes"
	ProfileBudgetReasonSamples      ProfileBudgetReason = "samples"
	ProfileBudgetReasonDuration     ProfileBudgetReason = "duration"
	ProfileBudgetReasonCPUTime      ProfileBudgetReason = "cpu_time"
	ProfileBudgetReasonAllocBytes   ProfileBudgetReason = "alloc_bytes"
	ProfileBudgetReasonInUseBytes   ProfileBudgetReason = "inuse_bytes"
	ProfileBudgetReasonGoroutines   ProfileBudgetReason = "goroutines"
)

// CPUProfileMetadata is caller-provided CPU profile metadata. It deliberately
// avoids parsing pprof protobuf internals.
type CPUProfileMetadata struct {
	SampleCount  int           `json:"sample_count,omitempty"`
	SamplePeriod time.Duration `json:"sample_period,omitempty"`
	TotalCPUTime time.Duration `json:"total_cpu_time,omitempty"`
}

// MemoryProfileMetadata is caller-provided heap or allocation profile metadata.
type MemoryProfileMetadata struct {
	SampleCount  int    `json:"sample_count,omitempty"`
	AllocBytes   uint64 `json:"alloc_bytes,omitempty"`
	AllocObjects uint64 `json:"alloc_objects,omitempty"`
	InUseBytes   uint64 `json:"inuse_bytes,omitempty"`
	InUseObjects uint64 `json:"inuse_objects,omitempty"`
}

// GoroutineProfileMetadata is caller-provided goroutine profile metadata.
type GoroutineProfileMetadata struct {
	Goroutines int            `json:"goroutines,omitempty"`
	States     map[string]int `json:"states,omitempty"`
}

// ProfileBudget contains optional report limits. Zero values leave a dimension
// unbounded. WarningRatio defaults to 0.8 when unset or outside (0, 1].
type ProfileBudget struct {
	MaxPayloadBytes int64         `json:"max_payload_bytes,omitempty"`
	MaxSamples      int           `json:"max_samples,omitempty"`
	MaxDuration     time.Duration `json:"max_duration,omitempty"`
	MaxCPUTime      time.Duration `json:"max_cpu_time,omitempty"`
	MaxAllocBytes   uint64        `json:"max_alloc_bytes,omitempty"`
	MaxInUseBytes   uint64        `json:"max_inuse_bytes,omitempty"`
	MaxGoroutines   int           `json:"max_goroutines,omitempty"`
	WarningRatio    float64       `json:"warning_ratio,omitempty"`
}

// ProfileBudgetClassification is the budget result attached to a summary.
type ProfileBudgetClassification struct {
	Class      ProfileBudgetClass    `json:"class"`
	UsageRatio float64               `json:"usage_ratio,omitempty"`
	Reasons    []ProfileBudgetReason `json:"reasons,omitempty"`
}

// ProfileSummaryInput describes one already-captured profile using simple
// metadata supplied by the caller.
type ProfileSummaryInput struct {
	Kind         ProfileKind       `json:"kind,omitempty"`
	Name         string            `json:"name,omitempty"`
	CapturedAt   time.Time         `json:"captured_at,omitempty"`
	Duration     time.Duration     `json:"duration,omitempty"`
	PayloadBytes int64             `json:"payload_bytes,omitempty"`
	SampleCount  int               `json:"sample_count,omitempty"`
	Labels       map[string]string `json:"labels,omitempty"`

	CPU       CPUProfileMetadata       `json:"cpu,omitempty"`
	Memory    MemoryProfileMetadata    `json:"memory,omitempty"`
	Goroutine GoroutineProfileMetadata `json:"goroutine,omitempty"`

	Budget ProfileBudget `json:"budget,omitempty"`
}

// ProfileSummary is a JSON-safe profile report header for CPU, memory, and
// goroutine profiles.
type ProfileSummary struct {
	Kind         ProfileKind       `json:"kind"`
	Name         string            `json:"name,omitempty"`
	CapturedAt   time.Time         `json:"captured_at,omitempty"`
	Duration     time.Duration     `json:"duration,omitempty"`
	PayloadBytes int64             `json:"payload_bytes,omitempty"`
	SampleCount  int               `json:"sample_count,omitempty"`
	Labels       map[string]string `json:"labels,omitempty"`

	CPU       *CPUProfileMetadata       `json:"cpu,omitempty"`
	Memory    *MemoryProfileMetadata    `json:"memory,omitempty"`
	Goroutine *GoroutineProfileMetadata `json:"goroutine,omitempty"`

	Budget ProfileBudgetClassification `json:"budget"`
}

// SummarizeProfile builds a compact, deterministic profile summary from
// caller-provided metadata. It does not read or decode pprof protobuf payloads.
func SummarizeProfile(input ProfileSummaryInput) ProfileSummary {
	kind := NormalizeProfileKind(input.Kind)
	if kind == "" {
		kind = profileSummaryInferKind(input)
	}
	summary := ProfileSummary{
		Kind:         kind,
		Name:         strings.TrimSpace(input.Name),
		CapturedAt:   input.CapturedAt,
		Duration:     profileSummaryNonNegativeDuration(input.Duration),
		PayloadBytes: profileSummaryNonNegativeInt64(input.PayloadBytes),
		SampleCount:  profileSummaryNonNegativeInt(input.SampleCount),
		Labels:       SafeProfileLabels(input.Labels),
	}

	cpu := profileSummaryCPU(input.CPU)
	if kind == ProfileKindCPU || profileSummaryHasCPU(cpu) {
		summary.CPU = &cpu
	}

	memory := profileSummaryMemory(input.Memory)
	if kind == ProfileKindMemory || profileSummaryHasMemory(memory) {
		summary.Memory = &memory
	}

	goroutine := profileSummaryGoroutine(input.Goroutine)
	if kind == ProfileKindGoroutine || profileSummaryHasGoroutine(goroutine) {
		summary.Goroutine = &goroutine
	}

	if summary.SampleCount == 0 {
		summary.SampleCount = profileSummarySampleCount(summary)
	}
	summary.Budget = ClassifyProfileBudget(summary, input.Budget)
	return summary
}

// NormalizeProfileKind canonicalizes common pprof profile names used by report
// callers.
func NormalizeProfileKind(kind ProfileKind) ProfileKind {
	value := strings.ToLower(strings.TrimSpace(string(kind)))
	switch value {
	case "":
		return ""
	case "profile", "cpu":
		return ProfileKindCPU
	case "heap", "alloc", "allocs", "memory", "mem":
		return ProfileKindMemory
	case "goroutine", "goroutines":
		return ProfileKindGoroutine
	default:
		return ProfileKind(value)
	}
}

func profileSummaryInferKind(input ProfileSummaryInput) ProfileKind {
	if profileSummaryHasCPU(profileSummaryCPU(input.CPU)) {
		return ProfileKindCPU
	}
	if profileSummaryHasMemory(profileSummaryMemory(input.Memory)) {
		return ProfileKindMemory
	}
	if profileSummaryHasGoroutine(profileSummaryGoroutine(input.Goroutine)) {
		return ProfileKindGoroutine
	}
	return ""
}

// SafeProfileLabels returns a bounded, low-cardinality copy of Lazuli profile
// labels suitable for profile reports. Unknown labels are omitted so tenant,
// user, or request identifiers do not leak into report metadata.
func SafeProfileLabels(labels map[string]string) map[string]string {
	normalized := NormalizeProfileLabels(labels)
	if len(normalized) == 0 {
		return nil
	}

	safe := make(map[string]string, 6)
	profileSummaryAddSafeLabel(safe, normalized, ProfileLabelFeature, profileSummaryMaxLabelValueBytes)
	profileSummaryAddSafeLabel(safe, normalized, ProfileLabelKind, profileSummaryMaxLabelValueBytes)
	profileSummaryAddSafeLabel(safe, normalized, ProfileLabelOp, profileSummaryMaxLabelValueBytes)
	profileSummaryAddSafeLabel(safe, normalized, ProfileLabelSource, profileSummaryMaxSourceValueBytes)
	profileSummaryAddSafeLabel(safe, normalized, ProfileLabelPatternID, profileSummaryMaxLabelValueBytes)
	profileSummaryAddSafeLabel(safe, normalized, ProfileLabelPatternVersion, profileSummaryMaxLabelValueBytes)
	if len(safe) == 0 {
		return nil
	}
	return safe
}

// ClassifyProfileBudget compares summary metadata with optional limits.
func ClassifyProfileBudget(summary ProfileSummary, budget ProfileBudget) ProfileBudgetClassification {
	classification := ProfileBudgetClassification{Class: ProfileBudgetOK}
	warningRatio := profileSummaryBudgetWarningRatio(budget)

	profileSummaryClassifyFloat(&classification, ProfileBudgetReasonPayloadBytes, float64(summary.PayloadBytes), float64(budget.MaxPayloadBytes), warningRatio)
	profileSummaryClassifyFloat(&classification, ProfileBudgetReasonSamples, float64(summary.SampleCount), float64(budget.MaxSamples), warningRatio)
	profileSummaryClassifyFloat(&classification, ProfileBudgetReasonDuration, float64(summary.Duration), float64(budget.MaxDuration), warningRatio)
	if summary.CPU != nil {
		profileSummaryClassifyFloat(&classification, ProfileBudgetReasonCPUTime, float64(summary.CPU.TotalCPUTime), float64(budget.MaxCPUTime), warningRatio)
	}
	if summary.Memory != nil {
		profileSummaryClassifyFloat(&classification, ProfileBudgetReasonAllocBytes, float64(summary.Memory.AllocBytes), float64(budget.MaxAllocBytes), warningRatio)
		profileSummaryClassifyFloat(&classification, ProfileBudgetReasonInUseBytes, float64(summary.Memory.InUseBytes), float64(budget.MaxInUseBytes), warningRatio)
	}
	if summary.Goroutine != nil {
		profileSummaryClassifyFloat(&classification, ProfileBudgetReasonGoroutines, float64(summary.Goroutine.Goroutines), float64(budget.MaxGoroutines), warningRatio)
	}

	return classification
}

func profileSummaryAddSafeLabel(out map[string]string, labels map[string]string, key string, maxBytes int) {
	value := profileSummarySafeLabelValue(labels[key], maxBytes)
	if value != "" {
		out[key] = value
	}
}

func profileSummarySafeLabelValue(value string, maxBytes int) string {
	value = strings.TrimSpace(value)
	if value == "" || maxBytes <= 0 {
		return ""
	}

	var b strings.Builder
	b.Grow(len(value))
	lastSpace := false
	for _, r := range value {
		if r < 0x20 || r == 0x7f {
			if !lastSpace && b.Len() > 0 {
				b.WriteByte(' ')
				lastSpace = true
			}
			continue
		}
		b.WriteRune(r)
		lastSpace = false
	}

	value = strings.TrimSpace(b.String())
	if len(value) <= maxBytes {
		return value
	}
	value = value[:maxBytes]
	for len(value) > 0 && !utf8.ValidString(value) {
		value = value[:len(value)-1]
	}
	return strings.TrimSpace(value)
}

func profileSummaryCPU(cpu CPUProfileMetadata) CPUProfileMetadata {
	cpu.SampleCount = profileSummaryNonNegativeInt(cpu.SampleCount)
	cpu.SamplePeriod = profileSummaryNonNegativeDuration(cpu.SamplePeriod)
	cpu.TotalCPUTime = profileSummaryNonNegativeDuration(cpu.TotalCPUTime)
	if cpu.TotalCPUTime == 0 && cpu.SampleCount > 0 && cpu.SamplePeriod > 0 {
		cpu.TotalCPUTime = profileSummaryMulDuration(cpu.SamplePeriod, cpu.SampleCount)
	}
	return cpu
}

func profileSummaryMemory(memory MemoryProfileMetadata) MemoryProfileMetadata {
	memory.SampleCount = profileSummaryNonNegativeInt(memory.SampleCount)
	return memory
}

func profileSummaryGoroutine(goroutine GoroutineProfileMetadata) GoroutineProfileMetadata {
	goroutine.Goroutines = profileSummaryNonNegativeInt(goroutine.Goroutines)
	if len(goroutine.States) == 0 {
		return goroutine
	}

	states := make(map[string]int, len(goroutine.States))
	total := 0
	for state, count := range goroutine.States {
		state = profileSummarySafeLabelValue(state, profileSummaryMaxLabelValueBytes)
		count = profileSummaryNonNegativeInt(count)
		if state == "" || count == 0 {
			continue
		}
		states[state] += count
		total += count
	}
	if len(states) > 0 {
		goroutine.States = states
		if goroutine.Goroutines == 0 {
			goroutine.Goroutines = total
		}
		return goroutine
	}
	goroutine.States = nil
	return goroutine
}

func profileSummarySampleCount(summary ProfileSummary) int {
	if summary.CPU != nil && summary.CPU.SampleCount > 0 {
		return summary.CPU.SampleCount
	}
	if summary.Memory != nil && summary.Memory.SampleCount > 0 {
		return summary.Memory.SampleCount
	}
	if summary.Goroutine != nil && summary.Goroutine.Goroutines > 0 {
		return summary.Goroutine.Goroutines
	}
	return 0
}

func profileSummaryHasCPU(cpu CPUProfileMetadata) bool {
	return cpu.SampleCount > 0 || cpu.SamplePeriod > 0 || cpu.TotalCPUTime > 0
}

func profileSummaryHasMemory(memory MemoryProfileMetadata) bool {
	return memory.SampleCount > 0 ||
		memory.AllocBytes > 0 ||
		memory.AllocObjects > 0 ||
		memory.InUseBytes > 0 ||
		memory.InUseObjects > 0
}

func profileSummaryHasGoroutine(goroutine GoroutineProfileMetadata) bool {
	return goroutine.Goroutines > 0 || len(goroutine.States) > 0
}

func profileSummaryBudgetWarningRatio(budget ProfileBudget) float64 {
	if budget.WarningRatio <= 0 || budget.WarningRatio > 1 {
		return profileSummaryDefaultWarningRatio
	}
	return budget.WarningRatio
}

func profileSummaryClassifyFloat(classification *ProfileBudgetClassification, reason ProfileBudgetReason, actual, limit float64, warningRatio float64) {
	if actual <= 0 || limit <= 0 {
		return
	}

	usageRatio := actual / limit
	if usageRatio > classification.UsageRatio {
		classification.UsageRatio = usageRatio
	}

	switch {
	case actual > limit:
		classification.Class = ProfileBudgetExceeded
		classification.Reasons = append(classification.Reasons, reason)
	case usageRatio >= warningRatio && classification.Class != ProfileBudgetExceeded:
		classification.Class = ProfileBudgetWarning
		classification.Reasons = append(classification.Reasons, reason)
	case usageRatio >= warningRatio:
		classification.Reasons = append(classification.Reasons, reason)
	}
}

func profileSummaryNonNegativeInt(value int) int {
	if value < 0 {
		return 0
	}
	return value
}

func profileSummaryNonNegativeInt64(value int64) int64 {
	if value < 0 {
		return 0
	}
	return value
}

func profileSummaryNonNegativeDuration(value time.Duration) time.Duration {
	if value < 0 {
		return 0
	}
	return value
}

func profileSummaryMulDuration(value time.Duration, multiplier int) time.Duration {
	if multiplier <= 0 || value <= 0 {
		return 0
	}
	if int64(value) > int64(1<<63-1)/int64(multiplier) {
		return time.Duration(1<<63 - 1)
	}
	return value * time.Duration(multiplier)
}
