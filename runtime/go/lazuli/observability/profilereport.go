package observability

import (
	"sort"
	"strings"
	"time"
)

const (
	// ProfileLabelFeature is the pprof label key for a Lazuli feature.
	ProfileLabelFeature = "feature"
	// ProfileLabelKind is the pprof label key for a Lazuli operation kind.
	ProfileLabelKind = "kind"
	// ProfileLabelOp is the pprof label key for a Lazuli operation name.
	ProfileLabelOp = "op"
	// ProfileLabelPatternID is the optional decoded codegen pattern label key.
	ProfileLabelPatternID = "pattern_id"
	// ProfileLabelPatternVersion is the optional decoded codegen pattern version label key.
	ProfileLabelPatternVersion = "pattern_version"
)

// ProfileMetric names a profile duration axis used to rank operation rows.
type ProfileMetric string

const (
	// ProfileMetricCPU ranks operations by CPU duration.
	ProfileMetricCPU ProfileMetric = "cpu"
	// ProfileMetricAlloc ranks operations by allocation duration.
	ProfileMetricAlloc ProfileMetric = "alloc"
	// ProfileMetricBlock ranks operations by blocking duration.
	ProfileMetricBlock ProfileMetric = "block"
)

// ProfileSample is an already-decoded profile sample. Callers decode pprof
// protobufs elsewhere and pass the Lazuli label set plus duration buckets here.
type ProfileSample struct {
	// Labels carries decoded pprof labels. The feature, kind, and op keys are
	// required for an attributed operation row.
	Labels map[string]string `json:"labels,omitempty"`

	// Feature, Kind, and Op are accepted for callers that normalize labels
	// before building a report. Labels take precedence when both are present.
	Feature string `json:"feature,omitempty"`
	Kind    string `json:"kind,omitempty"`
	Op      string `json:"op,omitempty"`

	// PatternID and PatternVersion are optional codegen pattern metadata already
	// associated with this sample by the caller.
	PatternID      string `json:"pattern_id,omitempty"`
	PatternVersion string `json:"pattern_version,omitempty"`

	CPUDuration   time.Duration `json:"cpu_duration"`
	AllocDuration time.Duration `json:"alloc_duration"`
	BlockDuration time.Duration `json:"block_duration"`
}

// ProfileTotals contains aggregate profile duration counters.
type ProfileTotals struct {
	SampleCount   int           `json:"sample_count"`
	CPUDuration   time.Duration `json:"cpu_duration"`
	AllocDuration time.Duration `json:"alloc_duration"`
	BlockDuration time.Duration `json:"block_duration"`
}

// ProfileOpReport is the aggregate profile row for one Lazuli operation.
type ProfileOpReport struct {
	Feature string `json:"feature"`
	Kind    string `json:"kind"`
	Op      string `json:"op"`

	PatternID      string `json:"pattern_id,omitempty"`
	PatternVersion string `json:"pattern_version,omitempty"`

	SampleCount   int           `json:"sample_count"`
	CPUDuration   time.Duration `json:"cpu_duration"`
	AllocDuration time.Duration `json:"alloc_duration"`
	BlockDuration time.Duration `json:"block_duration"`
}

// Name returns the stable display key used by profile reports.
func (r ProfileOpReport) Name() string {
	switch {
	case r.Feature == "" && r.Kind == "":
		return r.Op
	case r.Feature == "":
		return r.Kind + "." + r.Op
	case r.Kind == "":
		return r.Feature + "." + r.Op
	default:
		return r.Feature + "." + r.Kind + "." + r.Op
	}
}

// ProfileReport is a deterministic aggregate view over decoded profile samples.
type ProfileReport struct {
	Total        ProfileTotals     `json:"total"`
	Unattributed ProfileTotals     `json:"unattributed"`
	Ops          []ProfileOpReport `json:"ops"`
	TopCPU       []ProfileOpReport `json:"top_cpu"`
	TopAlloc     []ProfileOpReport `json:"top_alloc"`
	TopBlock     []ProfileOpReport `json:"top_block"`
}

// BuildProfileReport aggregates decoded profile samples and ranks the top N
// operations for each duration axis.
func BuildProfileReport(samples []ProfileSample, topN int) ProfileReport {
	ops, total, unattributed := AggregateProfileSamples(samples)
	return ProfileReport{
		Total:        total,
		Unattributed: unattributed,
		Ops:          ops,
		TopCPU:       RankProfileOps(ops, ProfileMetricCPU, topN),
		TopAlloc:     RankProfileOps(ops, ProfileMetricAlloc, topN),
		TopBlock:     RankProfileOps(ops, ProfileMetricBlock, topN),
	}
}

// AggregateProfileSamples groups decoded samples by Lazuli op labels.
//
// Samples missing feature, kind, or op labels are counted in the returned
// unattributed totals but are not emitted as operation rows.
func AggregateProfileSamples(samples []ProfileSample) ([]ProfileOpReport, ProfileTotals, ProfileTotals) {
	rowsByKey := make(map[profileReportOpKey]*ProfileOpReport)
	total := ProfileTotals{}
	unattributed := ProfileTotals{}

	for _, sample := range samples {
		profileReportAddTotals(&total, sample)

		key, ok := profileReportSampleKey(sample)
		if !ok {
			profileReportAddTotals(&unattributed, sample)
			continue
		}

		row := rowsByKey[key]
		if row == nil {
			row = &ProfileOpReport{
				Feature: key.feature,
				Kind:    key.kind,
				Op:      key.op,
			}
			rowsByKey[key] = row
		}

		row.SampleCount++
		row.CPUDuration += sample.CPUDuration
		row.AllocDuration += sample.AllocDuration
		row.BlockDuration += sample.BlockDuration
		profileReportMergePattern(row, sample)
	}

	rows := make([]ProfileOpReport, 0, len(rowsByKey))
	for _, row := range rowsByKey {
		rows = append(rows, *row)
	}
	sort.Slice(rows, func(i, j int) bool {
		return profileReportRowIdentityLess(rows[i], rows[j])
	})
	return rows, total, unattributed
}

// RankProfileOps returns the top N operation rows for the requested metric.
// The input slice is never mutated.
func RankProfileOps(ops []ProfileOpReport, by ProfileMetric, n int) []ProfileOpReport {
	if n <= 0 || len(ops) == 0 {
		return nil
	}
	if !profileReportValidMetric(by) {
		return nil
	}

	ranked := append([]ProfileOpReport(nil), ops...)
	sort.Slice(ranked, func(i, j int) bool {
		left := profileReportMetricValue(ranked[i], by)
		right := profileReportMetricValue(ranked[j], by)
		if left != right {
			return left > right
		}
		return profileReportRowIdentityLess(ranked[i], ranked[j])
	})

	if n > len(ranked) {
		n = len(ranked)
	}
	return ranked[:n]
}

type profileReportOpKey struct {
	feature string
	kind    string
	op      string
}

func profileReportSampleKey(sample ProfileSample) (profileReportOpKey, bool) {
	key := profileReportOpKey{
		feature: profileReportLabel(sample, ProfileLabelFeature, sample.Feature),
		kind:    profileReportLabel(sample, ProfileLabelKind, sample.Kind),
		op:      profileReportLabel(sample, ProfileLabelOp, sample.Op),
	}
	if key.feature == "" || key.kind == "" || key.op == "" {
		return profileReportOpKey{}, false
	}
	return key, true
}

func profileReportLabel(sample ProfileSample, key, fallback string) string {
	if sample.Labels != nil {
		if value := strings.TrimSpace(sample.Labels[key]); value != "" {
			return value
		}
	}
	return strings.TrimSpace(fallback)
}

func profileReportAddTotals(total *ProfileTotals, sample ProfileSample) {
	total.SampleCount++
	total.CPUDuration += sample.CPUDuration
	total.AllocDuration += sample.AllocDuration
	total.BlockDuration += sample.BlockDuration
}

func profileReportMergePattern(row *ProfileOpReport, sample ProfileSample) {
	patternID := profileReportLabel(sample, ProfileLabelPatternID, sample.PatternID)
	patternVersion := profileReportLabel(sample, ProfileLabelPatternVersion, sample.PatternVersion)
	if patternID == "" && patternVersion == "" {
		return
	}
	if row.PatternID == "" && row.PatternVersion == "" {
		row.PatternID = patternID
		row.PatternVersion = patternVersion
		return
	}
	if profileReportPatternLess(patternID, patternVersion, row.PatternID, row.PatternVersion) {
		row.PatternID = patternID
		row.PatternVersion = patternVersion
	}
}

func profileReportPatternLess(leftID, leftVersion, rightID, rightVersion string) bool {
	if leftID == "" || rightID == "" {
		return leftID != "" && rightID == ""
	}
	if leftID != rightID {
		return leftID < rightID
	}
	if leftVersion == "" || rightVersion == "" {
		return leftVersion != "" && rightVersion == ""
	}
	return leftVersion < rightVersion
}

func profileReportValidMetric(metric ProfileMetric) bool {
	switch metric {
	case ProfileMetricCPU, ProfileMetricAlloc, ProfileMetricBlock:
		return true
	default:
		return false
	}
}

func profileReportMetricValue(row ProfileOpReport, by ProfileMetric) time.Duration {
	switch by {
	case ProfileMetricCPU:
		return row.CPUDuration
	case ProfileMetricAlloc:
		return row.AllocDuration
	case ProfileMetricBlock:
		return row.BlockDuration
	default:
		return 0
	}
}

func profileReportRowIdentityLess(left, right ProfileOpReport) bool {
	if left.Feature != right.Feature {
		return left.Feature < right.Feature
	}
	if left.Kind != right.Kind {
		return left.Kind < right.Kind
	}
	if left.Op != right.Op {
		return left.Op < right.Op
	}
	if left.PatternID != right.PatternID {
		return left.PatternID < right.PatternID
	}
	return left.PatternVersion < right.PatternVersion
}
