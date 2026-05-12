package security

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sort"
	"strconv"
	"strings"
)

// ErrInvalidVulnerabilityReport is returned when a vulnerability report JSON
// snippet cannot be decoded.
var ErrInvalidVulnerabilityReport = errors.New("lazuli/security: invalid_vulnerability_report")

// VulnerabilitySeverity is a normalized vulnerability severity.
type VulnerabilitySeverity uint8

const (
	// VulnerabilitySeverityUnknown is used when a scanner does not emit a
	// recognizable severity value.
	VulnerabilitySeverityUnknown VulnerabilitySeverity = iota
	VulnerabilitySeverityNone
	VulnerabilitySeverityLow
	VulnerabilitySeverityMedium
	VulnerabilitySeverityHigh
	VulnerabilitySeverityCritical
)

// String returns the stable lowercase severity label.
func (s VulnerabilitySeverity) String() string {
	switch s {
	case VulnerabilitySeverityNone:
		return "none"
	case VulnerabilitySeverityLow:
		return "low"
	case VulnerabilitySeverityMedium:
		return "medium"
	case VulnerabilitySeverityHigh:
		return "high"
	case VulnerabilitySeverityCritical:
		return "critical"
	default:
		return "unknown"
	}
}

// NormalizeVulnerabilitySeverity maps common scanner severity spellings and
// CVSS numeric scores to a stable severity value.
func NormalizeVulnerabilitySeverity(raw string) VulnerabilitySeverity {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return VulnerabilitySeverityUnknown
	}
	if score, err := strconv.ParseFloat(trimmed, 64); err == nil {
		return vulnerabilitySeverityFromScore(score)
	}

	normalized := strings.ToLower(trimmed)
	normalized = strings.NewReplacer("-", "", "_", "", " ", "").Replace(normalized)
	switch normalized {
	case "none", "negligible":
		return VulnerabilitySeverityNone
	case "low", "minor", "informational", "info":
		return VulnerabilitySeverityLow
	case "medium", "moderate", "warning":
		return VulnerabilitySeverityMedium
	case "high", "important", "major":
		return VulnerabilitySeverityHigh
	case "critical", "crit", "severe":
		return VulnerabilitySeverityCritical
	default:
		return VulnerabilitySeverityUnknown
	}
}

// VulnerabilityReport is a normalized vulnerability scan report.
type VulnerabilityReport struct {
	Findings []VulnerabilityFinding `json:"findings,omitempty"`
}

// VulnerabilityFinding describes one vulnerability occurrence in a module or
// package.
type VulnerabilityFinding struct {
	ID           string                `json:"id,omitempty"`
	Summary      string                `json:"summary,omitempty"`
	Module       string                `json:"module,omitempty"`
	Version      string                `json:"version,omitempty"`
	Package      string                `json:"package,omitempty"`
	FixedVersion string                `json:"fixed_version,omitempty"`
	Severity     VulnerabilitySeverity `json:"severity,omitempty"`
}

// AffectedModuleSummary summarizes unique vulnerabilities affecting one module.
type AffectedModuleSummary struct {
	Module          string                `json:"module"`
	Count           int                   `json:"count"`
	HighestSeverity VulnerabilitySeverity `json:"highest_severity"`
	Vulnerabilities []string              `json:"vulnerabilities,omitempty"`
	FixedVersions   []string              `json:"fixed_versions,omitempty"`
}

// VulnerabilityFailThreshold configures report failure evaluation. A zero
// MinimumSeverity defaults to VulnerabilitySeverityLow so the zero value fails
// on any known vulnerability.
type VulnerabilityFailThreshold struct {
	MinimumSeverity VulnerabilitySeverity
	IncludeUnknown  bool
}

// VulnerabilityFailResult describes the result of threshold evaluation.
type VulnerabilityFailResult struct {
	Failed          bool
	HighestSeverity VulnerabilitySeverity
	Findings        []VulnerabilityFinding
}

// ParseVulnerabilityReport parses govulncheck-like JSON snippets into a
// normalized report. It accepts a single JSON object, an array, or a stream of
// newline-separated JSON objects.
func ParseVulnerabilityReport(data []byte) (VulnerabilityReport, error) {
	if len(bytes.TrimSpace(data)) == 0 {
		return VulnerabilityReport{}, fmt.Errorf("%w: empty report", ErrInvalidVulnerabilityReport)
	}

	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()

	builder := vulnerabilityReportBuilder{metadata: make(map[string]VulnerabilityFinding)}
	for {
		var value any
		if err := decoder.Decode(&value); err != nil {
			if err == io.EOF {
				break
			}
			return VulnerabilityReport{}, fmt.Errorf("%w: %v", ErrInvalidVulnerabilityReport, err)
		}
		builder.add(value)
	}
	return builder.report(), nil
}

// AffectedModules returns deterministic per-module vulnerability summaries.
func (r VulnerabilityReport) AffectedModules() []AffectedModuleSummary {
	type accumulator struct {
		summary AffectedModuleSummary
		ids     map[string]struct{}
		fixes   map[string]struct{}
	}

	byModule := make(map[string]*accumulator)
	for _, finding := range r.Findings {
		module := firstNonEmptyString(finding.Module, finding.Package)
		if module == "" {
			continue
		}
		acc := byModule[module]
		if acc == nil {
			acc = &accumulator{
				summary: AffectedModuleSummary{Module: module, HighestSeverity: VulnerabilitySeverityUnknown},
				ids:     make(map[string]struct{}),
				fixes:   make(map[string]struct{}),
			}
			byModule[module] = acc
		}
		if id := strings.TrimSpace(finding.ID); id != "" {
			if _, ok := acc.ids[id]; !ok {
				acc.ids[id] = struct{}{}
				acc.summary.Count++
				acc.summary.Vulnerabilities = append(acc.summary.Vulnerabilities, id)
			}
		} else {
			acc.summary.Count++
		}
		if fixed := strings.TrimSpace(finding.FixedVersion); fixed != "" {
			if _, ok := acc.fixes[fixed]; !ok {
				acc.fixes[fixed] = struct{}{}
				acc.summary.FixedVersions = append(acc.summary.FixedVersions, fixed)
			}
		}
		acc.summary.HighestSeverity = maxVulnerabilitySeverity(acc.summary.HighestSeverity, finding.Severity)
	}

	summaries := make([]AffectedModuleSummary, 0, len(byModule))
	for _, acc := range byModule {
		sort.Strings(acc.summary.Vulnerabilities)
		sort.Strings(acc.summary.FixedVersions)
		summaries = append(summaries, acc.summary)
	}
	sort.Slice(summaries, func(i, j int) bool {
		return summaries[i].Module < summaries[j].Module
	})
	return summaries
}

// EvaluateFailThreshold returns the findings that meet threshold and whether
// the report should fail.
func (r VulnerabilityReport) EvaluateFailThreshold(threshold VulnerabilityFailThreshold) VulnerabilityFailResult {
	minimum := threshold.MinimumSeverity
	if minimum == VulnerabilitySeverityUnknown {
		minimum = VulnerabilitySeverityLow
	}

	result := VulnerabilityFailResult{HighestSeverity: VulnerabilitySeverityUnknown}
	for _, finding := range r.Findings {
		result.HighestSeverity = maxVulnerabilitySeverity(result.HighestSeverity, finding.Severity)
		if finding.Severity == VulnerabilitySeverityUnknown {
			if threshold.IncludeUnknown {
				result.Findings = append(result.Findings, finding)
			}
			continue
		}
		if vulnerabilitySeverityRank(finding.Severity) >= vulnerabilitySeverityRank(minimum) {
			result.Findings = append(result.Findings, finding)
		}
	}
	result.Failed = len(result.Findings) > 0
	return result
}

// FailsThreshold reports whether any finding meets the configured threshold.
func (r VulnerabilityReport) FailsThreshold(threshold VulnerabilityFailThreshold) bool {
	return r.EvaluateFailThreshold(threshold).Failed
}

type vulnerabilityReportBuilder struct {
	metadata map[string]VulnerabilityFinding
	findings []VulnerabilityFinding
}

func (b *vulnerabilityReportBuilder) add(value any) {
	switch typed := value.(type) {
	case []any:
		for _, item := range typed {
			b.add(item)
		}
	case map[string]any:
		b.addObject(typed)
	}
}

func (b *vulnerabilityReportBuilder) addObject(obj map[string]any) {
	for _, key := range []string{"vulns", "vulnerabilities", "findings"} {
		for _, item := range arrayValue(obj[key]) {
			b.add(item)
		}
	}
	if osv := mapValue(obj["osv"]); osv != nil {
		if metadata, ok := vulnerabilityFindingFromObject(osv); ok && metadata.ID != "" {
			b.metadata[metadata.ID] = mergeVulnerabilityMetadata(b.metadata[metadata.ID], metadata)
		}
	}
	if finding := mapValue(obj["finding"]); finding != nil {
		if parsed, ok := vulnerabilityFindingFromObject(finding); ok {
			b.findings = append(b.findings, parsed)
		}
	}
	if parsed, ok := vulnerabilityFindingFromObject(obj); ok {
		b.findings = append(b.findings, parsed)
	}
}

func (b *vulnerabilityReportBuilder) report() VulnerabilityReport {
	report := VulnerabilityReport{Findings: make([]VulnerabilityFinding, 0, len(b.findings)+len(b.metadata))}
	seen := make(map[string]struct{}, len(b.findings))
	for _, finding := range b.findings {
		finding = mergeVulnerabilityMetadata(finding, b.metadata[finding.ID])
		if normalized, ok := normalizeVulnerabilityFinding(finding); ok {
			report.Findings = append(report.Findings, normalized)
			if normalized.ID != "" {
				seen[normalized.ID] = struct{}{}
			}
		}
	}

	ids := make([]string, 0, len(b.metadata))
	for id := range b.metadata {
		if _, ok := seen[id]; !ok {
			ids = append(ids, id)
		}
	}
	sort.Strings(ids)
	for _, id := range ids {
		if normalized, ok := normalizeVulnerabilityFinding(b.metadata[id]); ok {
			report.Findings = append(report.Findings, normalized)
		}
	}
	return report
}

func vulnerabilityFindingFromObject(obj map[string]any) (VulnerabilityFinding, bool) {
	finding := VulnerabilityFinding{
		ID:           firstStringField(obj, "id", "osv", "vulnerability", "vulnerability_id", "vuln"),
		Summary:      firstStringField(obj, "summary", "details", "description"),
		Module:       firstStringField(obj, "module", "module_path"),
		Version:      firstStringField(obj, "version", "module_version"),
		Package:      firstStringField(obj, "package", "package_path", "import_path"),
		FixedVersion: firstStringField(obj, "fixed_version", "fixedVersion", "fixed"),
		Severity:     vulnerabilitySeverityFromObject(obj),
	}
	for _, frame := range arrayValue(obj["trace"]) {
		if trace := mapValue(frame); trace != nil {
			fillVulnerabilityFinding(&finding, trace)
		}
	}
	if finding.Module == "" || finding.Package == "" {
		module, pkg := firstAffectedModulePackage(arrayValue(obj["affected"]))
		finding.Module = firstNonEmptyString(finding.Module, module)
		finding.Package = firstNonEmptyString(finding.Package, pkg)
	}
	return finding, finding.ID != "" || finding.Summary != "" || finding.Module != "" || finding.Package != ""
}

func fillVulnerabilityFinding(finding *VulnerabilityFinding, obj map[string]any) {
	finding.Module = firstNonEmptyString(finding.Module, firstStringField(obj, "module", "module_path"))
	finding.Version = firstNonEmptyString(finding.Version, firstStringField(obj, "version", "module_version"))
	finding.Package = firstNonEmptyString(finding.Package, firstStringField(obj, "package", "package_path", "import_path"))
	finding.FixedVersion = firstNonEmptyString(finding.FixedVersion, firstStringField(obj, "fixed_version", "fixedVersion", "fixed"))
}

func firstAffectedModulePackage(affected []any) (string, string) {
	for _, item := range affected {
		entry := mapValue(item)
		if entry == nil {
			continue
		}
		if pkg := packageName(entry["package"]); pkg != "" {
			return pkg, pkg
		}
		module := firstStringField(entry, "module", "module_path")
		pkg := firstStringField(entry, "package", "package_path", "import_path")
		if module != "" || pkg != "" {
			return module, pkg
		}
	}
	return "", ""
}

func packageName(value any) string {
	if text := stringValue(value); text != "" {
		return text
	}
	return firstStringField(mapValue(value), "name", "path")
}

func mergeVulnerabilityMetadata(finding, metadata VulnerabilityFinding) VulnerabilityFinding {
	if finding.ID == "" {
		finding.ID = metadata.ID
	}
	if finding.Summary == "" {
		finding.Summary = metadata.Summary
	}
	if finding.Module == "" {
		finding.Module = metadata.Module
	}
	if finding.Package == "" {
		finding.Package = metadata.Package
	}
	if finding.Severity == VulnerabilitySeverityUnknown {
		finding.Severity = metadata.Severity
	}
	return finding
}

func normalizeVulnerabilityFinding(finding VulnerabilityFinding) (VulnerabilityFinding, bool) {
	finding.ID = strings.TrimSpace(finding.ID)
	finding.Summary = strings.TrimSpace(finding.Summary)
	finding.Module = strings.TrimSpace(finding.Module)
	finding.Version = strings.TrimSpace(finding.Version)
	finding.Package = strings.TrimSpace(finding.Package)
	finding.FixedVersion = strings.TrimSpace(finding.FixedVersion)
	if finding.Module == "" {
		finding.Module = finding.Package
	}
	return finding, finding.ID != "" || finding.Summary != "" || finding.Module != "" || finding.Package != ""
}

func vulnerabilitySeverityFromObject(obj map[string]any) VulnerabilitySeverity {
	severity := VulnerabilitySeverityUnknown
	for _, key := range []string{"severity", "severity_level", "level", "risk", "risk_level", "cvss", "score"} {
		severity = maxVulnerabilitySeverity(severity, vulnerabilitySeverityFromValue(obj[key]))
	}
	if databaseSpecific := mapValue(obj["database_specific"]); databaseSpecific != nil {
		severity = maxVulnerabilitySeverity(severity, vulnerabilitySeverityFromObject(databaseSpecific))
	}
	return severity
}

func vulnerabilitySeverityFromValue(value any) VulnerabilitySeverity {
	switch typed := value.(type) {
	case string:
		return NormalizeVulnerabilitySeverity(typed)
	case json.Number:
		score, err := typed.Float64()
		if err != nil {
			return VulnerabilitySeverityUnknown
		}
		return vulnerabilitySeverityFromScore(score)
	case float64:
		return vulnerabilitySeverityFromScore(typed)
	case []any:
		severity := VulnerabilitySeverityUnknown
		for _, item := range typed {
			severity = maxVulnerabilitySeverity(severity, vulnerabilitySeverityFromValue(item))
		}
		return severity
	case map[string]any:
		return vulnerabilitySeverityFromObject(typed)
	default:
		return VulnerabilitySeverityUnknown
	}
}

func vulnerabilitySeverityFromScore(score float64) VulnerabilitySeverity {
	switch {
	case score < 0:
		return VulnerabilitySeverityUnknown
	case score == 0:
		return VulnerabilitySeverityNone
	case score < 4:
		return VulnerabilitySeverityLow
	case score < 7:
		return VulnerabilitySeverityMedium
	case score < 9:
		return VulnerabilitySeverityHigh
	default:
		return VulnerabilitySeverityCritical
	}
}

func maxVulnerabilitySeverity(a, b VulnerabilitySeverity) VulnerabilitySeverity {
	if vulnerabilitySeverityRank(b) > vulnerabilitySeverityRank(a) {
		return b
	}
	return a
}

func vulnerabilitySeverityRank(severity VulnerabilitySeverity) int {
	switch severity {
	case VulnerabilitySeverityNone:
		return 0
	case VulnerabilitySeverityLow:
		return 1
	case VulnerabilitySeverityMedium:
		return 2
	case VulnerabilitySeverityHigh:
		return 3
	case VulnerabilitySeverityCritical:
		return 4
	default:
		return -1
	}
}

func firstStringField(obj map[string]any, keys ...string) string {
	for _, key := range keys {
		if value := stringValue(obj[key]); value != "" {
			return value
		}
	}
	return ""
}

func firstNonEmptyString(values ...string) string {
	for _, value := range values {
		if trimmed := strings.TrimSpace(value); trimmed != "" {
			return trimmed
		}
	}
	return ""
}

func stringValue(value any) string {
	text, ok := value.(string)
	if !ok {
		return ""
	}
	return strings.TrimSpace(text)
}

func mapValue(value any) map[string]any {
	if typed, ok := value.(map[string]any); ok {
		return typed
	}
	return nil
}

func arrayValue(value any) []any {
	if typed, ok := value.([]any); ok {
		return typed
	}
	return nil
}
