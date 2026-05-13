package diagnostics

import (
	"fmt"
	"sort"
	"strings"
	"time"
)

const deprecationDateLayout = "2006-01-02"

// DeprecationMetadata is the normalized lifecycle metadata attached to a
// deprecated surface.
type DeprecationMetadata struct {
	Since       string `json:"since,omitempty"`
	Replacement string `json:"replacement,omitempty"`
	Sunset      string `json:"sunset,omitempty"`
}

// DeprecationLocation identifies where deprecation metadata was declared.
type DeprecationLocation struct {
	Path   string `json:"path,omitempty"`
	Line   int    `json:"line,omitempty"`
	Column int    `json:"column,omitempty"`
}

// DeprecationReportInput describes one deprecated surface before report
// severity and deadline state have been derived.
type DeprecationReportInput struct {
	Surface  string
	Metadata DeprecationMetadata
	Location DeprecationLocation
	Severity Severity
	Message  string
}

// DeprecationDeadlineStatus describes the parsed state of a sunset deadline.
type DeprecationDeadlineStatus string

const (
	DeprecationDeadlineNone    DeprecationDeadlineStatus = "none"
	DeprecationDeadlinePending DeprecationDeadlineStatus = "pending"
	DeprecationDeadlinePassed  DeprecationDeadlineStatus = "passed"
	DeprecationDeadlineInvalid DeprecationDeadlineStatus = "invalid"
)

// String renders s as the stable lowercase token used in report output.
func (s DeprecationDeadlineStatus) String() string {
	switch s {
	case "", DeprecationDeadlineNone:
		return string(DeprecationDeadlineNone)
	case DeprecationDeadlinePending:
		return string(DeprecationDeadlinePending)
	case DeprecationDeadlinePassed:
		return string(DeprecationDeadlinePassed)
	case DeprecationDeadlineInvalid:
		return string(DeprecationDeadlineInvalid)
	default:
		return "unknown"
	}
}

// DeprecationReportEntry is one normalized deprecation diagnostic row.
type DeprecationReportEntry struct {
	Surface        string                    `json:"surface"`
	Severity       Severity                  `json:"severity"`
	Message        string                    `json:"message"`
	Location       DeprecationLocation       `json:"location,omitempty"`
	Since          string                    `json:"since,omitempty"`
	Replacement    string                    `json:"replacement,omitempty"`
	Deadline       string                    `json:"deadline,omitempty"`
	DeadlineStatus DeprecationDeadlineStatus `json:"deadline_status"`
}

// DeprecationReportGroup is a deterministic severity group of deprecation
// report entries.
type DeprecationReportGroup struct {
	Key      string                   `json:"key"`
	Severity Severity                 `json:"severity"`
	Entries  []DeprecationReportEntry `json:"entries"`
}

// DeprecationReport is the sorted report for a set of deprecated surfaces.
type DeprecationReport struct {
	Entries []DeprecationReportEntry `json:"entries"`
	Groups  []DeprecationReportGroup `json:"groups"`
}

// BuildDeprecationReport normalizes metadata into sorted report entries and
// deterministic severity groups. now is used only for YYYY-MM-DD sunset
// comparisons; a zero time skips past-deadline escalation.
func BuildDeprecationReport(now time.Time, inputs []DeprecationReportInput) DeprecationReport {
	entries := make([]DeprecationReportEntry, 0, len(inputs))
	for _, input := range inputs {
		entries = append(entries, buildDeprecationReportEntry(now, input))
	}

	entries = SortedDeprecationReportEntries(entries)
	return DeprecationReport{
		Entries: entries,
		Groups:  GroupDeprecationReportEntriesBySeverity(entries),
	}
}

// SortDeprecationReportEntries sorts entries by location, surface, severity,
// replacement, deadline, and message while preserving order for exact ties.
func SortDeprecationReportEntries(entries []DeprecationReportEntry) {
	sort.SliceStable(entries, func(i, j int) bool {
		return deprecationReportEntryLess(entries[i], entries[j])
	})
}

// SortedDeprecationReportEntries returns a sorted copy of entries.
func SortedDeprecationReportEntries(entries []DeprecationReportEntry) []DeprecationReportEntry {
	out := make([]DeprecationReportEntry, len(entries))
	copy(out, entries)
	SortDeprecationReportEntries(out)
	return out
}

// GroupDeprecationReportEntriesBySeverity returns severity groups in the fixed
// error, warning, info, hint, unknown order. Entries inside each group are
// sorted with the same ordering as SortedDeprecationReportEntries.
func GroupDeprecationReportEntriesBySeverity(entries []DeprecationReportEntry) []DeprecationReportGroup {
	sorted := SortedDeprecationReportEntries(entries)
	byKey := make(map[string][]DeprecationReportEntry)
	severityByKey := make(map[string]Severity)

	for _, entry := range sorted {
		key := entry.Severity.String()
		byKey[key] = append(byKey[key], entry)
		if _, ok := severityByKey[key]; !ok {
			severityByKey[key] = entry.Severity
		}
	}

	keys := deprecationReportGroupKeys(byKey)
	groups := make([]DeprecationReportGroup, 0, len(keys))
	for _, key := range keys {
		groups = append(groups, DeprecationReportGroup{
			Key:      key,
			Severity: severityByKey[key],
			Entries:  byKey[key],
		})
	}
	return groups
}

func buildDeprecationReportEntry(now time.Time, input DeprecationReportInput) DeprecationReportEntry {
	metadata := normalizeDeprecationMetadata(input.Metadata)
	location := normalizeDeprecationLocation(input.Location)
	status := deprecationDeadlineStatus(now, metadata.Sunset)
	severity := deprecationReportSeverity(input.Severity, status)

	entry := DeprecationReportEntry{
		Surface:        strings.TrimSpace(input.Surface),
		Severity:       severity,
		Location:       location,
		Since:          metadata.Since,
		Replacement:    metadata.Replacement,
		Deadline:       metadata.Sunset,
		DeadlineStatus: status,
	}
	entry.Message = deprecationReportMessage(entry, strings.TrimSpace(input.Message))
	return entry
}

func normalizeDeprecationMetadata(metadata DeprecationMetadata) DeprecationMetadata {
	return DeprecationMetadata{
		Since:       strings.TrimSpace(metadata.Since),
		Replacement: strings.TrimSpace(metadata.Replacement),
		Sunset:      strings.TrimSpace(metadata.Sunset),
	}
}

func normalizeDeprecationLocation(location DeprecationLocation) DeprecationLocation {
	if location.Line < 0 {
		location.Line = 0
	}
	if location.Column < 0 {
		location.Column = 0
	}
	location.Path = strings.TrimSpace(location.Path)
	return location
}

func deprecationDeadlineStatus(now time.Time, deadline string) DeprecationDeadlineStatus {
	if deadline == "" {
		return DeprecationDeadlineNone
	}

	parsed, err := time.Parse(deprecationDateLayout, deadline)
	if err != nil {
		return DeprecationDeadlineInvalid
	}
	if now.IsZero() {
		return DeprecationDeadlinePending
	}

	year, month, day := now.Date()
	today := time.Date(year, month, day, 0, 0, 0, 0, time.UTC)
	if parsed.Before(today) {
		return DeprecationDeadlinePassed
	}
	return DeprecationDeadlinePending
}

func deprecationReportSeverity(severity Severity, status DeprecationDeadlineStatus) Severity {
	normalized := normalizeDeprecationReportSeverity(severity)
	switch status {
	case DeprecationDeadlineInvalid:
		return deprecationReportAtLeastSeverity(normalized, SeverityError)
	case DeprecationDeadlinePassed:
		return deprecationReportAtLeastSeverity(normalized, SeverityWarning)
	default:
		return normalized
	}
}

func normalizeDeprecationReportSeverity(severity Severity) Severity {
	switch severity {
	case SeverityError, SeverityWarning, SeverityInfo, SeverityHint:
		return severity
	default:
		return SeverityWarning
	}
}

func deprecationReportAtLeastSeverity(severity, minimum Severity) Severity {
	if catalogSeverityRank(severity) <= catalogSeverityRank(minimum) {
		return severity
	}
	return minimum
}

func deprecationReportMessage(entry DeprecationReportEntry, override string) string {
	if override != "" {
		return override
	}

	surface := entry.Surface
	if surface == "" {
		surface = "deprecated surface"
	}

	switch entry.DeadlineStatus {
	case DeprecationDeadlineInvalid:
		return fmt.Sprintf("%s has invalid deprecation deadline %q; expected YYYY-MM-DD.", surface, entry.Deadline)
	case DeprecationDeadlinePassed:
		return fmt.Sprintf("%s is deprecated and its deadline %s has passed.", surface, entry.Deadline)
	}

	var builder strings.Builder
	builder.WriteString(surface)
	builder.WriteString(" is deprecated")
	if entry.Since != "" {
		builder.WriteString(" since ")
		builder.WriteString(entry.Since)
	}
	if entry.Replacement != "" {
		builder.WriteString("; use ")
		builder.WriteString(entry.Replacement)
		builder.WriteString(" instead")
	}
	if entry.Deadline != "" {
		builder.WriteString("; deadline ")
		builder.WriteString(entry.Deadline)
	}
	builder.WriteString(".")
	return builder.String()
}

func deprecationReportGroupKeys(byKey map[string][]DeprecationReportEntry) []string {
	knownOrder := []string{
		SeverityError.String(),
		SeverityWarning.String(),
		SeverityInfo.String(),
		SeverityHint.String(),
	}
	keys := make([]string, 0, len(byKey))
	seen := make(map[string]bool, len(knownOrder))
	for _, key := range knownOrder {
		seen[key] = true
		if len(byKey[key]) > 0 {
			keys = append(keys, key)
		}
	}

	unknown := make([]string, 0)
	for key := range byKey {
		if !seen[key] {
			unknown = append(unknown, key)
		}
	}
	sort.Strings(unknown)
	keys = append(keys, unknown...)
	return keys
}

func deprecationReportEntryLess(a, b DeprecationReportEntry) bool {
	if a.Location.Path != b.Location.Path {
		return a.Location.Path < b.Location.Path
	}
	if a.Location.Line != b.Location.Line {
		return a.Location.Line < b.Location.Line
	}
	if a.Location.Column != b.Location.Column {
		return a.Location.Column < b.Location.Column
	}
	if a.Surface != b.Surface {
		return a.Surface < b.Surface
	}
	if catalogSeverityRank(a.Severity) != catalogSeverityRank(b.Severity) {
		return catalogSeverityRank(a.Severity) < catalogSeverityRank(b.Severity)
	}
	if a.Replacement != b.Replacement {
		return a.Replacement < b.Replacement
	}
	if a.Deadline != b.Deadline {
		return a.Deadline < b.Deadline
	}
	if a.Since != b.Since {
		return a.Since < b.Since
	}
	return a.Message < b.Message
}
