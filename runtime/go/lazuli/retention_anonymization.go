package lazuli

import (
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode"
)

var (
	// ErrRetentionDurationInvalid is returned when a retention window cannot be
	// converted to a cutoff time.
	ErrRetentionDurationInvalid = errors.New("lazuli: retention duration invalid")

	// ErrRetentionPlanInvalid is returned when retention/anonymization metadata
	// cannot produce a deterministic dry-run plan.
	ErrRetentionPlanInvalid = errors.New("lazuli: retention plan invalid")
)

// RetentionFieldAction is the provider-neutral field mutation selected for a
// row anonymization job. Keep is an explicit no-op for metadata that wants to
// document that a field was considered and intentionally left untouched.
type RetentionFieldAction int

const (
	RetentionFieldKeep RetentionFieldAction = iota
	RetentionFieldNull
	RetentionFieldZero
	RetentionFieldRedact
)

// String renders the field action as the stable lowercase token used in plans.
func (a RetentionFieldAction) String() string {
	switch a {
	case RetentionFieldKeep:
		return "keep"
	case RetentionFieldNull:
		return "null"
	case RetentionFieldZero:
		return "zero"
	case RetentionFieldRedact:
		return "redact"
	default:
		return "unknown"
	}
}

// String renders the terminal retention action as the stable lowercase token
// used in plans.
func (a RetentionAction) String() string {
	switch a {
	case RetentionDelete:
		return "delete"
	case RetentionAnonymize:
		return "anonymize"
	case RetentionArchive:
		return "archive"
	default:
		return "unknown"
	}
}

// RetentionField describes one field-level action for resources whose terminal
// retention action is RetentionAnonymize.
type RetentionField struct {
	Name   string
	Action RetentionFieldAction
	Reason string
}

// RetentionResourceMetadata is the generator-neutral resource shape used to
// plan retention jobs without binding the runtime to a store or generated row
// type.
type RetentionResourceMetadata struct {
	Name       string
	Feature    string
	SoftDelete bool
	Retention  *RetentionSpec
	Fields     []RetentionField
}

// NewRetentionResourceMetadata returns a retention planning snapshot for a
// generated Resource[T]. The retention spec and field slice are copied.
func NewRetentionResourceMetadata[T any](resource *Resource[T], fields ...RetentionField) RetentionResourceMetadata {
	metadata := RetentionResourceMetadata{
		Fields: cloneRetentionFields(fields),
	}
	if resource == nil {
		return metadata
	}

	metadata.Name = resource.Name
	metadata.Feature = resource.Feature
	metadata.SoftDelete = resource.SoftDelete
	if resource.Retention != nil {
		spec := *resource.Retention
		metadata.Retention = &spec
	}
	return metadata
}

// RetentionAnonymizationPlan is a dry-run list of resource-level retention
// jobs. Applying delete/archive/update statements belongs to a concrete adapter
// or job runner.
type RetentionAnonymizationPlan struct {
	DryRun      bool
	GeneratedAt Time
	Summary     RetentionAnonymizationSummary
	Entries     []RetentionAnonymizationPlanEntry
}

// RetentionAnonymizationSummary reports compact counts for dry-run output.
type RetentionAnonymizationSummary struct {
	ResourceCount    int
	PlannedCount     int
	SkippedCount     int
	DeleteCount      int
	AnonymizeCount   int
	ArchiveCount     int
	FieldActionCount int
}

// RetentionAnonymizationPlanEntry describes the selected terminal job for one
// resource.
type RetentionAnonymizationPlanEntry struct {
	Resource   string
	Feature    string
	Action     RetentionAction
	Window     Duration
	Cutoff     Time
	SoftDelete bool
	Fields     []RetentionField
	Reason     string
}

// RetentionCutoff returns the exclusive deleted_at cutoff for a retention
// spec. Durations accepted by time.ParseDuration are subtracted as exact
// durations; Lazuli calendar literals "d", "w", "mo", and "y" are subtracted
// with time.AddDate.
func RetentionCutoff(spec RetentionSpec, now Time) (Time, error) {
	now = normalizeRetentionNow(now)
	return retentionCutoffFromWindow(spec.Window, now)
}

// ValidateRetentionResources checks resource metadata without mutating input.
func ValidateRetentionResources(resources []RetentionResourceMetadata) error {
	_, err := normalizeRetentionResources(resources, normalizeRetentionNow(Time{}))
	return err
}

// BuildRetentionAnonymizationPlan evaluates resource retention metadata and
// returns a deterministic dry-run plan. Resources without a retention spec are
// counted as skipped and omitted from Entries.
func BuildRetentionAnonymizationPlan(resources []RetentionResourceMetadata, now Time) (RetentionAnonymizationPlan, error) {
	now = normalizeRetentionNow(now)
	normalized, err := normalizeRetentionResources(resources, now)
	if err != nil {
		return RetentionAnonymizationPlan{}, err
	}

	plan := RetentionAnonymizationPlan{
		DryRun:      true,
		GeneratedAt: now,
		Summary: RetentionAnonymizationSummary{
			ResourceCount: len(resources),
		},
	}

	for _, resource := range normalized {
		if resource.Retention == nil {
			plan.Summary.SkippedCount++
			continue
		}

		entry := RetentionAnonymizationPlanEntry{
			Resource:   resource.Name,
			Feature:    resource.Feature,
			Action:     resource.Retention.Then,
			Window:     resource.Retention.Window,
			SoftDelete: resource.SoftDelete,
			Reason:     "retention policy matched",
		}
		cutoff, err := retentionCutoffFromWindow(resource.Retention.Window, now)
		if err != nil {
			return RetentionAnonymizationPlan{}, err
		}
		entry.Cutoff = cutoff
		if entry.Action == RetentionAnonymize {
			entry.Fields = cloneRetentionFields(resource.Fields)
		}

		plan.Summary.PlannedCount++
		switch entry.Action {
		case RetentionDelete:
			plan.Summary.DeleteCount++
		case RetentionAnonymize:
			plan.Summary.AnonymizeCount++
			plan.Summary.FieldActionCount += len(entry.Fields)
		case RetentionArchive:
			plan.Summary.ArchiveCount++
		}
		plan.Entries = append(plan.Entries, entry)
	}

	sortRetentionPlanEntries(plan.Entries)
	return plan, nil
}

func normalizeRetentionResources(resources []RetentionResourceMetadata, now Time) ([]RetentionResourceMetadata, error) {
	normalized := make([]RetentionResourceMetadata, 0, len(resources))
	seen := make(map[string]int, len(resources))

	var errs []error
	for i, resource := range resources {
		clean, err := normalizeRetentionResource(resource, now, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := retentionResourceKey(clean)
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: resource[%d] %q duplicates resource[%d]", ErrRetentionPlanInvalid, i, clean.Name, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	sortRetentionResources(normalized)
	return normalized, nil
}

func normalizeRetentionResource(resource RetentionResourceMetadata, now Time, index int) (RetentionResourceMetadata, error) {
	clean := RetentionResourceMetadata{
		Name:       strings.TrimSpace(resource.Name),
		Feature:    strings.TrimSpace(resource.Feature),
		SoftDelete: resource.SoftDelete,
	}
	if resource.Retention != nil {
		spec := *resource.Retention
		spec.Window = Duration(strings.TrimSpace(string(spec.Window)))
		clean.Retention = &spec
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, retentionResourceError(index, "name is required"))
	} else if hasRetentionControl(clean.Name) {
		errs = append(errs, retentionResourceError(index, "name contains control characters"))
	}
	if hasRetentionControl(clean.Feature) {
		errs = append(errs, retentionResourceError(index, "feature contains control characters"))
	}

	fields, err := normalizeRetentionFields(resource.Fields, index)
	if err != nil {
		errs = append(errs, err)
	}
	clean.Fields = fields

	if clean.Retention != nil {
		if !clean.SoftDelete {
			errs = append(errs, retentionResourceError(index, "retention requires soft delete metadata"))
		}
		if !isKnownRetentionAction(clean.Retention.Then) {
			errs = append(errs, retentionResourceError(index, fmt.Sprintf("unknown retention action %q", clean.Retention.Then)))
		}
		if _, err := retentionCutoffFromWindow(clean.Retention.Window, now); err != nil {
			errs = append(errs, retentionResourceError(index, err.Error()))
		}
	}

	if err := errors.Join(errs...); err != nil {
		return RetentionResourceMetadata{}, err
	}
	return clean, nil
}

func normalizeRetentionFields(fields []RetentionField, resourceIndex int) ([]RetentionField, error) {
	normalized := make([]RetentionField, 0, len(fields))
	seen := make(map[string]int, len(fields))

	var errs []error
	for i, field := range fields {
		clean := RetentionField{
			Name:   strings.TrimSpace(field.Name),
			Action: field.Action,
			Reason: strings.TrimSpace(field.Reason),
		}

		if clean.Name == "" {
			errs = append(errs, retentionFieldError(resourceIndex, i, "name is required"))
			continue
		}
		if hasRetentionControl(clean.Name) {
			errs = append(errs, retentionFieldError(resourceIndex, i, "name contains control characters"))
		}
		if hasRetentionControl(clean.Reason) {
			errs = append(errs, retentionFieldError(resourceIndex, i, "reason contains control characters"))
		}
		if !isKnownRetentionFieldAction(clean.Action) {
			errs = append(errs, retentionFieldError(resourceIndex, i, fmt.Sprintf("unknown action %q", clean.Action)))
		}

		key := strings.ToLower(clean.Name)
		if first, ok := seen[key]; ok {
			errs = append(errs, retentionFieldError(resourceIndex, i, fmt.Sprintf("field %q duplicates field[%d]", clean.Name, first)))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return compareRetentionField(normalized[i], normalized[j]) < 0
	})
	return normalized, nil
}

func retentionCutoffFromWindow(window Duration, now Time) (Time, error) {
	raw := strings.TrimSpace(string(window))
	if raw == "" {
		return Time{}, fmt.Errorf("%w: window is required", ErrRetentionDurationInvalid)
	}
	if strings.HasPrefix(raw, "-") {
		return Time{}, fmt.Errorf("%w: window must be non-negative", ErrRetentionDurationInvalid)
	}

	if exact, err := time.ParseDuration(raw); err == nil {
		if exact < 0 {
			return Time{}, fmt.Errorf("%w: window must be non-negative", ErrRetentionDurationInvalid)
		}
		return now.Add(-exact), nil
	}

	value, suffix, ok := splitRetentionCalendarWindow(raw)
	if !ok {
		return Time{}, fmt.Errorf("%w: unsupported window %q", ErrRetentionDurationInvalid, raw)
	}
	amount, err := strconv.Atoi(value)
	if err != nil || amount < 0 {
		return Time{}, fmt.Errorf("%w: unsupported window %q", ErrRetentionDurationInvalid, raw)
	}

	switch suffix {
	case "d":
		return now.AddDate(0, 0, -amount), nil
	case "w":
		return now.AddDate(0, 0, -7*amount), nil
	case "mo":
		return now.AddDate(0, -amount, 0), nil
	case "y":
		return now.AddDate(-amount, 0, 0), nil
	default:
		return Time{}, fmt.Errorf("%w: unsupported window %q", ErrRetentionDurationInvalid, raw)
	}
}

func splitRetentionCalendarWindow(raw string) (value string, suffix string, ok bool) {
	for _, candidate := range []string{"mo", "d", "w", "y"} {
		if strings.HasSuffix(raw, candidate) {
			value = strings.TrimSpace(strings.TrimSuffix(raw, candidate))
			return value, candidate, value != ""
		}
	}
	return "", "", false
}

func sortRetentionResources(resources []RetentionResourceMetadata) {
	sort.SliceStable(resources, func(i, j int) bool {
		return compareRetentionResource(resources[i], resources[j]) < 0
	})
}

func sortRetentionPlanEntries(entries []RetentionAnonymizationPlanEntry) {
	sort.SliceStable(entries, func(i, j int) bool {
		return compareRetentionPlanEntry(entries[i], entries[j]) < 0
	})
}

func compareRetentionResource(left, right RetentionResourceMetadata) int {
	for _, cmp := range []int{
		compareRetentionFold(left.Feature, right.Feature),
		compareRetentionFold(left.Name, right.Name),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func compareRetentionPlanEntry(left, right RetentionAnonymizationPlanEntry) int {
	for _, cmp := range []int{
		compareRetentionFold(left.Feature, right.Feature),
		compareRetentionFold(left.Resource, right.Resource),
		compareRetentionInt(int(left.Action), int(right.Action)),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func compareRetentionField(left, right RetentionField) int {
	for _, cmp := range []int{
		compareRetentionFold(left.Name, right.Name),
		compareRetentionInt(int(left.Action), int(right.Action)),
		compareRetentionFold(left.Reason, right.Reason),
	} {
		if cmp != 0 {
			return cmp
		}
	}
	return 0
}

func retentionResourceKey(resource RetentionResourceMetadata) string {
	return strings.ToLower(resource.Feature) + "\x00" + strings.ToLower(resource.Name)
}

func cloneRetentionFields(fields []RetentionField) []RetentionField {
	if len(fields) == 0 {
		return nil
	}
	return append([]RetentionField(nil), fields...)
}

func normalizeRetentionNow(now Time) Time {
	if now.IsZero() {
		return time.Now().UTC()
	}
	return now.UTC()
}

func retentionResourceError(index int, reason string) error {
	return fmt.Errorf("%w: resource[%d] %s", ErrRetentionPlanInvalid, index, reason)
}

func retentionFieldError(resourceIndex, fieldIndex int, reason string) error {
	return fmt.Errorf("%w: resource[%d].fields[%d] %s", ErrRetentionPlanInvalid, resourceIndex, fieldIndex, reason)
}

func isKnownRetentionAction(action RetentionAction) bool {
	switch action {
	case RetentionDelete, RetentionAnonymize, RetentionArchive:
		return true
	default:
		return false
	}
}

func isKnownRetentionFieldAction(action RetentionFieldAction) bool {
	switch action {
	case RetentionFieldKeep, RetentionFieldNull, RetentionFieldZero, RetentionFieldRedact:
		return true
	default:
		return false
	}
}

func compareRetentionFold(left, right string) int {
	leftFold := strings.ToLower(left)
	rightFold := strings.ToLower(right)
	switch {
	case leftFold < rightFold:
		return -1
	case leftFold > rightFold:
		return 1
	case left < right:
		return -1
	case left > right:
		return 1
	default:
		return 0
	}
}

func compareRetentionInt(left, right int) int {
	switch {
	case left < right:
		return -1
	case left > right:
		return 1
	default:
		return 0
	}
}

func hasRetentionControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}
