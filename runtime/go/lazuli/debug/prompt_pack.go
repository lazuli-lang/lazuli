package debug

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"
	"unicode/utf8"

	"lazuli.dev/runtime/lazuli/observability"
)

const (
	// PromptPackVersion is the current prompt-pack payload schema.
	PromptPackVersion = "lazuli.debug.prompt_pack.v1"
	// DefaultPromptPackMaxBytes is the default final JSON payload budget.
	DefaultPromptPackMaxBytes = 16 * 1024
)

var (
	// ErrPromptPackBudget is returned when the payload cannot fit within the
	// configured byte budget after deterministic truncation.
	ErrPromptPackBudget = errors.New("lazuli/debug: prompt pack byte budget too small")
	// ErrPromptPackGeneratedSource is returned when a source snippet points at
	// generated Go, which prompt packs intentionally do not expose to IA tools.
	ErrPromptPackGeneratedSource = errors.New("lazuli/debug: prompt pack cannot include generated Go source")
)

// PromptPackConfig controls prompt-pack redaction and final payload size.
type PromptPackConfig struct {
	// MaxBytes caps the final JSON payload. Values less than or equal to zero
	// use DefaultPromptPackMaxBytes.
	MaxBytes int
	// Redaction controls sensitive-field redaction. Nil uses the debug package
	// defaults while allowing strings up to MaxBytes before prompt-pack
	// truncation decides what to remove.
	Redaction *RedactionConfig
}

// PromptPackInput is the value form accepted by BuildPromptPack.
type PromptPackInput struct {
	Config PromptPackConfig

	Error       error
	ErrorReport *ErrorReport

	SourceSnippets  []PromptPackSourceSnippet
	ProfileRows     []PromptPackProfileRow
	Recommendations []PromptPackRecommendation
}

// PromptPackSourceSnippet is one source or IR excerpt included in a prompt
// pack. Use Language values such as "lzi" or "json" to distinguish snippet
// kinds.
type PromptPackSourceSnippet struct {
	Name      string `json:"name,omitempty"`
	Path      string `json:"path,omitempty"`
	StartLine int    `json:"start_line,omitempty"`
	EndLine   int    `json:"end_line,omitempty"`
	Language  string `json:"language,omitempty"`
	Content   string `json:"content,omitempty"`
	Truncated bool   `json:"truncated,omitempty"`
}

// PromptPackProfileRow is a compact profile row for one Lazuli operation.
type PromptPackProfileRow struct {
	Feature string `json:"feature,omitempty"`
	Kind    string `json:"kind,omitempty"`
	Op      string `json:"op,omitempty"`

	PatternID      string `json:"pattern_id,omitempty"`
	PatternVersion string `json:"pattern_version,omitempty"`

	SampleCount int   `json:"sample_count,omitempty"`
	CPUNanos    int64 `json:"cpu_ns,omitempty"`
	AllocNanos  int64 `json:"alloc_ns,omitempty"`
	BlockNanos  int64 `json:"block_ns,omitempty"`
}

// PromptPackRecommendation is one suggested next action for the IA debugger.
type PromptPackRecommendation struct {
	Route    string `json:"route,omitempty"`
	Message  string `json:"message,omitempty"`
	Target   string `json:"target,omitempty"`
	Priority int    `json:"priority,omitempty"`
}

// PromptPackSummary mirrors the metadata embedded in the emitted payload.
type PromptPackSummary struct {
	MaxBytes               int  `json:"max_bytes"`
	TotalBytes             int  `json:"total_bytes"`
	Truncated              bool `json:"truncated"`
	OmittedSourceSnippets  int  `json:"omitted_source_snippets"`
	OmittedProfileRows     int  `json:"omitted_profile_rows"`
	OmittedRecommendations int  `json:"omitted_recommendations"`
}

// PromptPackBuilder accumulates debug context before producing a deterministic
// bounded JSON prompt pack.
type PromptPackBuilder struct {
	config PromptPackConfig

	errorReport     *ErrorReport
	sourceSnippets  []PromptPackSourceSnippet
	profileRows     []PromptPackProfileRow
	recommendations []PromptPackRecommendation
}

// NewPromptPackBuilder returns an empty prompt-pack builder.
func NewPromptPackBuilder(config *PromptPackConfig) *PromptPackBuilder {
	builder := &PromptPackBuilder{}
	if config != nil {
		builder.config = *config
	}
	return builder
}

// SetError stores BuildErrorReport(err) as the pack error report.
func (b *PromptPackBuilder) SetError(err error) {
	if b == nil {
		return
	}
	if err == nil {
		b.errorReport = nil
		return
	}
	report := BuildErrorReport(err)
	b.errorReport = &report
}

// SetErrorReport stores report as the pack error report.
func (b *PromptPackBuilder) SetErrorReport(report ErrorReport) {
	if b == nil {
		return
	}
	b.errorReport = &report
}

// AddSourceSnippet appends a source or IR snippet to the pack.
func (b *PromptPackBuilder) AddSourceSnippet(snippet PromptPackSourceSnippet) {
	if b == nil {
		return
	}
	b.sourceSnippets = append(b.sourceSnippets, snippet)
}

// AddProfileRow appends a profile row to the pack.
func (b *PromptPackBuilder) AddProfileRow(row PromptPackProfileRow) {
	if b == nil {
		return
	}
	b.profileRows = append(b.profileRows, row)
}

// AddProfileOpReport appends an observability profile row to the pack.
func (b *PromptPackBuilder) AddProfileOpReport(row observability.ProfileOpReport) {
	if b == nil {
		return
	}
	b.profileRows = append(b.profileRows, PromptPackProfileRowFromOpReport(row))
}

// AddRecommendation appends a debugger recommendation to the pack.
func (b *PromptPackBuilder) AddRecommendation(recommendation PromptPackRecommendation) {
	if b == nil {
		return
	}
	b.recommendations = append(b.recommendations, recommendation)
}

// Build returns the deterministic bounded JSON prompt pack and its summary.
func (b *PromptPackBuilder) Build() ([]byte, PromptPackSummary, error) {
	if b == nil {
		return BuildPromptPack(PromptPackInput{})
	}
	return BuildPromptPack(PromptPackInput{
		Config:          b.config,
		ErrorReport:     b.errorReport,
		SourceSnippets:  b.sourceSnippets,
		ProfileRows:     b.profileRows,
		Recommendations: b.recommendations,
	})
}

// PromptPackProfileRowFromOpReport converts an observability profile row to
// the prompt-pack representation.
func PromptPackProfileRowFromOpReport(row observability.ProfileOpReport) PromptPackProfileRow {
	return PromptPackProfileRow{
		Feature:        row.Feature,
		Kind:           row.Kind,
		Op:             row.Op,
		PatternID:      row.PatternID,
		PatternVersion: row.PatternVersion,
		SampleCount:    row.SampleCount,
		CPUNanos:       promptPackDurationNanos(row.CPUDuration),
		AllocNanos:     promptPackDurationNanos(row.AllocDuration),
		BlockNanos:     promptPackDurationNanos(row.BlockDuration),
	}
}

// BuildPromptPack returns input encoded as deterministic bounded JSON.
func BuildPromptPack(input PromptPackInput) ([]byte, PromptPackSummary, error) {
	payload, summary, err := promptPackNormalizeInput(input)
	if err != nil {
		return nil, PromptPackSummary{}, err
	}

	data, summary, err := promptPackMarshal(payload, input.Config, summary)
	if err != nil {
		return nil, PromptPackSummary{}, err
	}
	if len(data) <= summary.MaxBytes {
		return data, summary, nil
	}

	for len(data) > summary.MaxBytes {
		changed := false
		switch {
		case promptPackTrimLongestSnippet(payload.SourceSnippets):
			changed = true
		case promptPackTrimLongestRecommendation(payload.Recommendations):
			changed = true
		case len(payload.ProfileRows) > 0:
			payload.ProfileRows = payload.ProfileRows[:len(payload.ProfileRows)-1]
			summary.OmittedProfileRows++
			changed = true
		case len(payload.Recommendations) > 0:
			payload.Recommendations = payload.Recommendations[:len(payload.Recommendations)-1]
			summary.OmittedRecommendations++
			changed = true
		case len(payload.SourceSnippets) > 0:
			payload.SourceSnippets = payload.SourceSnippets[:len(payload.SourceSnippets)-1]
			summary.OmittedSourceSnippets++
			changed = true
		case promptPackTrimErrorReport(payload.ErrorReport):
			changed = true
		}

		if !changed {
			return nil, summary, ErrPromptPackBudget
		}
		summary.Truncated = true
		data, summary, err = promptPackMarshal(payload, input.Config, summary)
		if err != nil {
			return nil, PromptPackSummary{}, err
		}
	}

	return data, summary, nil
}

type promptPackPayload struct {
	Version         string                     `json:"version"`
	ErrorReport     *ErrorReport               `json:"error_report,omitempty"`
	SourceSnippets  []PromptPackSourceSnippet  `json:"source_snippets,omitempty"`
	ProfileRows     []PromptPackProfileRow     `json:"profile_rows,omitempty"`
	Recommendations []PromptPackRecommendation `json:"recommendations,omitempty"`
	Metadata        PromptPackSummary          `json:"metadata"`
}

func promptPackNormalizeInput(input PromptPackInput) (promptPackPayload, PromptPackSummary, error) {
	report := input.ErrorReport
	if report == nil && input.Error != nil {
		next := BuildErrorReport(input.Error)
		report = &next
	}
	if report != nil {
		next := *report
		next.Chain = append([]ErrorReportFrame(nil), report.Chain...)
		report = &next
	}

	sourceSnippets, err := promptPackNormalizeSourceSnippets(input.SourceSnippets)
	if err != nil {
		return promptPackPayload{}, PromptPackSummary{}, err
	}

	return promptPackPayload{
			Version:         PromptPackVersion,
			ErrorReport:     report,
			SourceSnippets:  sourceSnippets,
			ProfileRows:     promptPackNormalizeProfileRows(input.ProfileRows),
			Recommendations: promptPackNormalizeRecommendations(input.Recommendations),
		}, PromptPackSummary{
			MaxBytes: promptPackMaxBytes(input.Config),
		}, nil
}

func promptPackNormalizeSourceSnippets(snippets []PromptPackSourceSnippet) ([]PromptPackSourceSnippet, error) {
	if len(snippets) == 0 {
		return nil, nil
	}

	normalized := make([]PromptPackSourceSnippet, 0, len(snippets))
	seen := make(map[string]struct{}, len(snippets))
	for _, snippet := range snippets {
		snippet.Name = strings.TrimSpace(snippet.Name)
		snippet.Path = promptPackCleanSnippetPath(snippet.Path)
		snippet.Language = strings.TrimSpace(snippet.Language)
		if snippet.StartLine < 0 {
			snippet.StartLine = 0
		}
		if snippet.EndLine < 0 {
			snippet.EndLine = 0
		}
		if snippet.StartLine > 0 && snippet.EndLine > 0 && snippet.EndLine < snippet.StartLine {
			snippet.EndLine = snippet.StartLine
		}
		if snippet.Name == "" && snippet.Path == "" && snippet.Language == "" && snippet.Content == "" {
			continue
		}
		if promptPackContainsGeneratedGo(snippet.Path) {
			return nil, fmt.Errorf("%w: %s", ErrPromptPackGeneratedSource, snippet.Path)
		}

		key := promptPackSourceSnippetKey(snippet)
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		normalized = append(normalized, snippet)
	}

	sort.Slice(normalized, func(i, j int) bool {
		return promptPackSourceSnippetKey(normalized[i]) < promptPackSourceSnippetKey(normalized[j])
	})
	return normalized, nil
}

func promptPackNormalizeProfileRows(rows []PromptPackProfileRow) []PromptPackProfileRow {
	if len(rows) == 0 {
		return nil
	}

	normalized := make([]PromptPackProfileRow, 0, len(rows))
	seen := make(map[string]struct{}, len(rows))
	for _, row := range rows {
		row.Feature = strings.TrimSpace(row.Feature)
		row.Kind = strings.TrimSpace(row.Kind)
		row.Op = strings.TrimSpace(row.Op)
		row.PatternID = strings.TrimSpace(row.PatternID)
		row.PatternVersion = strings.TrimSpace(row.PatternVersion)
		if row.SampleCount < 0 {
			row.SampleCount = 0
		}
		if row.CPUNanos < 0 {
			row.CPUNanos = 0
		}
		if row.AllocNanos < 0 {
			row.AllocNanos = 0
		}
		if row.BlockNanos < 0 {
			row.BlockNanos = 0
		}
		if row.Feature == "" && row.Kind == "" && row.Op == "" && row.PatternID == "" && row.PatternVersion == "" {
			continue
		}

		key := promptPackProfileRowKey(row)
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		normalized = append(normalized, row)
	}

	sort.Slice(normalized, func(i, j int) bool {
		return promptPackProfileRowLess(normalized[i], normalized[j])
	})
	return normalized
}

func promptPackNormalizeRecommendations(recommendations []PromptPackRecommendation) []PromptPackRecommendation {
	if len(recommendations) == 0 {
		return nil
	}

	normalized := make([]PromptPackRecommendation, 0, len(recommendations))
	seen := make(map[string]struct{}, len(recommendations))
	for _, recommendation := range recommendations {
		recommendation.Route = strings.TrimSpace(recommendation.Route)
		recommendation.Message = strings.TrimSpace(recommendation.Message)
		recommendation.Target = strings.TrimSpace(recommendation.Target)
		if recommendation.Route == "" && recommendation.Message == "" && recommendation.Target == "" {
			continue
		}

		key := promptPackRecommendationKey(recommendation)
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		normalized = append(normalized, recommendation)
	}

	sort.Slice(normalized, func(i, j int) bool {
		return promptPackRecommendationKey(normalized[i]) < promptPackRecommendationKey(normalized[j])
	})
	return normalized
}

func promptPackMarshal(payload promptPackPayload, config PromptPackConfig, summary PromptPackSummary) ([]byte, PromptPackSummary, error) {
	redaction := promptPackRedactionConfig(config)
	for i := 0; i < 16; i++ {
		payload.Metadata = summary
		data, err := marshalJSONLine(Redact(payload, redaction))
		if err != nil {
			return nil, PromptPackSummary{}, err
		}
		if len(data) == summary.TotalBytes {
			return data, summary, nil
		}
		summary.TotalBytes = len(data)
	}
	return nil, PromptPackSummary{}, errors.New("prompt pack byte count did not converge")
}

func promptPackRedactionConfig(config PromptPackConfig) *RedactionConfig {
	maxBytes := promptPackMaxBytes(config)
	if config.Redaction == nil {
		return &RedactionConfig{MaxStringLen: maxBytes}
	}
	redaction := *config.Redaction
	if redaction.MaxStringLen <= 0 {
		redaction.MaxStringLen = maxBytes
	}
	return &redaction
}

func promptPackMaxBytes(config PromptPackConfig) int {
	if config.MaxBytes <= 0 {
		return DefaultPromptPackMaxBytes
	}
	return config.MaxBytes
}

func promptPackCleanSnippetPath(path string) string {
	path = strings.TrimSpace(path)
	path = strings.ReplaceAll(path, "\\", "/")
	for strings.Contains(path, "//") {
		path = strings.ReplaceAll(path, "//", "/")
	}
	path = strings.TrimPrefix(path, "./")
	return path
}

func promptPackContainsGeneratedGo(value string) bool {
	value = strings.ToLower(strings.ReplaceAll(value, "\\", "/"))
	return strings.Contains(value, "/dist/go/") ||
		strings.HasPrefix(value, "dist/go/") ||
		strings.HasSuffix(value, ".gen.go")
}

func promptPackTrimLongestSnippet(snippets []PromptPackSourceSnippet) bool {
	index := -1
	longest := 0
	for i, snippet := range snippets {
		if len(snippet.Content) > longest {
			index = i
			longest = len(snippet.Content)
		}
	}
	if index < 0 || longest == 0 {
		return false
	}

	nextMax := longest / 2
	if nextMax < 32 {
		nextMax = 0
	}
	snippets[index].Content = promptPackTruncateStringBytes(snippets[index].Content, nextMax)
	snippets[index].Truncated = true
	return true
}

func promptPackTrimLongestRecommendation(recommendations []PromptPackRecommendation) bool {
	index := -1
	longest := 0
	for i, recommendation := range recommendations {
		length := len(recommendation.Message) + len(recommendation.Target)
		if length > longest {
			index = i
			longest = length
		}
	}
	if index < 0 || longest == 0 {
		return false
	}

	if len(recommendations[index].Message) >= len(recommendations[index].Target) {
		nextMax := len(recommendations[index].Message) / 2
		if nextMax < 32 {
			nextMax = 0
		}
		recommendations[index].Message = promptPackTruncateStringBytes(recommendations[index].Message, nextMax)
	} else {
		nextMax := len(recommendations[index].Target) / 2
		if nextMax < 32 {
			nextMax = 0
		}
		recommendations[index].Target = promptPackTruncateStringBytes(recommendations[index].Target, nextMax)
	}
	return true
}

func promptPackTrimErrorReport(report *ErrorReport) bool {
	if report == nil {
		return false
	}
	if len(report.Chain) > 0 {
		report.Chain = report.Chain[:len(report.Chain)-1]
		report.Truncated = true
		return true
	}
	if report.Message != "" {
		nextMax := len(report.Message) / 2
		if nextMax < 32 {
			nextMax = 0
		}
		report.Message = promptPackTruncateStringBytes(report.Message, nextMax)
		report.Truncated = true
		return true
	}
	return false
}

func promptPackTruncateStringBytes(value string, maxBytes int) string {
	if maxBytes <= 0 {
		return ""
	}
	if len(value) <= maxBytes {
		return value
	}
	const suffix = truncationSuffix
	if maxBytes <= len(suffix) {
		return promptPackValidPrefix(value, maxBytes)
	}

	limit := maxBytes - len(suffix)
	prefix := promptPackValidPrefix(value, limit)
	return prefix + suffix
}

func promptPackValidPrefix(value string, maxBytes int) string {
	if maxBytes <= 0 {
		return ""
	}
	if len(value) <= maxBytes {
		return value
	}
	for maxBytes > 0 && !utf8.ValidString(value[:maxBytes]) {
		maxBytes--
	}
	return value[:maxBytes]
}

func promptPackDurationNanos(duration time.Duration) int64 {
	if duration <= 0 {
		return 0
	}
	return int64(duration)
}

func promptPackSourceSnippetKey(snippet PromptPackSourceSnippet) string {
	return strings.Join([]string{
		snippet.Path,
		fmt.Sprintf("%012d", snippet.StartLine),
		fmt.Sprintf("%012d", snippet.EndLine),
		snippet.Language,
		snippet.Name,
		snippet.Content,
	}, "\x00")
}

func promptPackProfileRowKey(row PromptPackProfileRow) string {
	return strings.Join([]string{
		row.Feature,
		row.Kind,
		row.Op,
		row.PatternID,
		row.PatternVersion,
		fmt.Sprintf("%012d", row.SampleCount),
		fmt.Sprintf("%020d", row.CPUNanos),
		fmt.Sprintf("%020d", row.AllocNanos),
		fmt.Sprintf("%020d", row.BlockNanos),
	}, "\x00")
}

func promptPackProfileRowLess(left, right PromptPackProfileRow) bool {
	if left.CPUNanos != right.CPUNanos {
		return left.CPUNanos > right.CPUNanos
	}
	if left.AllocNanos != right.AllocNanos {
		return left.AllocNanos > right.AllocNanos
	}
	if left.BlockNanos != right.BlockNanos {
		return left.BlockNanos > right.BlockNanos
	}
	if left.SampleCount != right.SampleCount {
		return left.SampleCount > right.SampleCount
	}
	return promptPackProfileRowIdentityKey(left) < promptPackProfileRowIdentityKey(right)
}

func promptPackProfileRowIdentityKey(row PromptPackProfileRow) string {
	return strings.Join([]string{
		row.Feature,
		row.Kind,
		row.Op,
		row.PatternID,
		row.PatternVersion,
	}, "\x00")
}

func promptPackRecommendationKey(recommendation PromptPackRecommendation) string {
	return strings.Join([]string{
		fmt.Sprintf("%012d", recommendation.Priority),
		recommendation.Route,
		recommendation.Target,
		recommendation.Message,
	}, "\x00")
}
