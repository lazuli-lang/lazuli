package deploy

import (
	"errors"
	"fmt"
	"sort"
	"strings"
)

const (
	defaultReleaseName        = "release"
	defaultReleaseEnvironment = "production"
	defaultBlueColor          = "blue"
	defaultGreenColor         = "green"
)

// ReleaseStrategy identifies a deployment rollout strategy.
type ReleaseStrategy string

const (
	// ReleaseStrategyBlueGreen describes a two-environment release where
	// traffic moves from the current color to the candidate color.
	ReleaseStrategyBlueGreen ReleaseStrategy = "blue-green"
	// ReleaseStrategyCanary describes a gradual traffic rollout by percentage.
	ReleaseStrategyCanary ReleaseStrategy = "canary"
)

// ErrInvalidReleasePlan reports an invalid release or rollback plan.
var ErrInvalidReleasePlan = errors.New("lazuli/deploy: invalid release plan")

// ReleasePlan is a deterministic, execution-neutral deployment checklist.
type ReleasePlan struct {
	Name        string
	Version     string
	Environment string
	Strategy    ReleaseStrategy

	Steps          []ReleaseStep
	MigrationGates []MigrationGate
	Validations    []ReleaseValidation
	Rollback       []RollbackAction
}

// ReleaseStep is one ordered release action.
type ReleaseStep struct {
	Name   string
	Action string
}

// MigrationGate is a pre-traffic migration safety check.
type MigrationGate struct {
	Name  string
	Check string
}

// ReleaseValidation is a post-deploy or in-rollout validation check.
type ReleaseValidation struct {
	Name  string
	Check string
}

// RollbackAction is one ordered rollback action.
type RollbackAction struct {
	Name   string
	Action string
}

// BlueGreenReleaseConfig configures a standard blue-green release plan.
type BlueGreenReleaseConfig struct {
	Name        string
	Version     string
	Environment string

	CurrentColor   string
	CandidateColor string

	MigrationGates []MigrationGate
	Validations    []ReleaseValidation
	Rollback       []RollbackAction
}

// CanaryReleaseConfig configures a standard canary release plan.
type CanaryReleaseConfig struct {
	Name        string
	Version     string
	Environment string

	// Percentages are sorted, deduplicated, and completed with 100 when set.
	// Empty uses 5, 25, 50, and 100.
	Percentages []int

	MigrationGates []MigrationGate
	Validations    []ReleaseValidation
	Rollback       []RollbackAction
}

// Step returns one release step.
func Step(name, action string) ReleaseStep {
	return ReleaseStep{Name: name, Action: action}
}

// Gate returns one migration gate.
func Gate(name, check string) MigrationGate {
	return MigrationGate{Name: name, Check: check}
}

// Validation returns one release validation check.
func Validation(name, check string) ReleaseValidation {
	return ReleaseValidation{Name: name, Check: check}
}

// Rollback returns one rollback action.
func Rollback(name, action string) RollbackAction {
	return RollbackAction{Name: name, Action: action}
}

// BuildBlueGreenReleasePlan returns a standard blue-green release and rollback
// plan with caller-supplied gates, validations, and rollback actions appended.
func BuildBlueGreenReleasePlan(config BlueGreenReleaseConfig) (ReleasePlan, error) {
	name := releaseDefault(config.Name, defaultReleaseName)
	environment := releaseDefault(config.Environment, defaultReleaseEnvironment)
	version := strings.TrimSpace(config.Version)
	current := releaseDefault(config.CurrentColor, defaultBlueColor)
	candidate := strings.TrimSpace(config.CandidateColor)
	if candidate == "" {
		candidate = alternateBlueGreenColor(current)
	}
	if strings.EqualFold(current, candidate) {
		return ReleasePlan{}, invalidReleasePlan("blue_green.candidate_color", "must differ from current_color")
	}

	versionLabel := releaseVersionLabel(version)
	plan := ReleasePlan{
		Name:           name,
		Version:        version,
		Environment:    environment,
		Strategy:       ReleaseStrategyBlueGreen,
		MigrationGates: append([]MigrationGate(nil), config.MigrationGates...),
		Validations:    append([]ReleaseValidation(nil), config.Validations...),
		Steps: []ReleaseStep{
			Step("prepare "+candidate, fmt.Sprintf("Deploy %s to %s in %s with traffic disabled.", versionLabel, candidate, environment)),
		},
		Rollback: []RollbackAction{
			Rollback("route traffic to "+current, fmt.Sprintf("Route all traffic back to %s.", current)),
			Rollback("hold "+candidate, fmt.Sprintf("Keep %s deployed for logs and investigation while serving traffic from %s.", candidate, current)),
		},
	}
	if len(config.MigrationGates) > 0 {
		plan.Steps = append(plan.Steps, Step("run migration gates", "Confirm migration gates pass before routing traffic."))
	}
	plan.Steps = append(plan.Steps,
		Step("warm "+candidate, fmt.Sprintf("Warm %s and run readiness checks before promotion.", candidate)),
		Step("shift traffic", fmt.Sprintf("Route traffic from %s to %s.", current, candidate)),
		Step("validate "+candidate, fmt.Sprintf("Run release validation checks against %s.", candidate)),
		Step("hold "+current, fmt.Sprintf("Keep %s available until the rollback window closes.", current)),
	)
	plan.Rollback = append(plan.Rollback, config.Rollback...)

	return normalizeReleasePlan(plan)
}

// BuildCanaryReleasePlan returns a standard canary release and rollback plan.
func BuildCanaryReleasePlan(config CanaryReleaseConfig) (ReleasePlan, error) {
	percentages, err := normalizeCanaryPercentages(config.Percentages)
	if err != nil {
		return ReleasePlan{}, err
	}

	name := releaseDefault(config.Name, defaultReleaseName)
	environment := releaseDefault(config.Environment, defaultReleaseEnvironment)
	version := strings.TrimSpace(config.Version)
	versionLabel := releaseVersionLabel(version)
	plan := ReleasePlan{
		Name:           name,
		Version:        version,
		Environment:    environment,
		Strategy:       ReleaseStrategyCanary,
		MigrationGates: append([]MigrationGate(nil), config.MigrationGates...),
		Validations:    append([]ReleaseValidation(nil), config.Validations...),
		Steps: []ReleaseStep{
			Step("prepare canary", fmt.Sprintf("Deploy %s to canary targets in %s with 0%% traffic.", versionLabel, environment)),
		},
		Rollback: []RollbackAction{
			Rollback("set canary to 0%", fmt.Sprintf("Route 0%% traffic to %s.", versionLabel)),
			Rollback("restore stable version", "Keep the previous stable version serving all traffic."),
		},
	}
	if len(config.MigrationGates) > 0 {
		plan.Steps = append(plan.Steps, Step("run migration gates", "Confirm migration gates pass before increasing canary traffic."))
	}
	for _, percentage := range percentages {
		if percentage == 100 {
			plan.Steps = append(plan.Steps, Step("promote 100% traffic", fmt.Sprintf("Route 100%% traffic to %s.", versionLabel)))
			continue
		}
		plan.Steps = append(plan.Steps,
			Step(fmt.Sprintf("shift %d%% traffic", percentage), fmt.Sprintf("Route %d%% traffic to %s.", percentage, versionLabel)),
			Step(fmt.Sprintf("validate %d%% canary", percentage), fmt.Sprintf("Run release validation checks before increasing canary traffic above %d%%.", percentage)),
		)
	}
	plan.Rollback = append(plan.Rollback, config.Rollback...)

	return normalizeReleasePlan(plan)
}

// Validate checks the release plan.
func (p ReleasePlan) Validate() error {
	return ValidateReleasePlan(p)
}

// RenderMarkdown renders the release plan as deterministic Markdown.
func (p ReleasePlan) RenderMarkdown() (string, error) {
	return RenderReleasePlanMarkdown(p)
}

// RenderText renders the release plan as deterministic plain text.
func (p ReleasePlan) RenderText() (string, error) {
	return RenderReleasePlanText(p)
}

// ValidateReleasePlan validates a release plan without mutating the input.
func ValidateReleasePlan(plan ReleasePlan) error {
	_, err := normalizeReleasePlan(plan)
	return err
}

// RenderReleasePlanMarkdown renders a release plan as deterministic Markdown.
func RenderReleasePlanMarkdown(plan ReleasePlan) (string, error) {
	normalized, err := normalizeReleasePlan(plan)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString("# Release: ")
	b.WriteString(releaseMarkdownHeading(normalized.Name))
	b.WriteString("\n\n")
	b.WriteString("| Field | Value |\n")
	b.WriteString("| --- | --- |\n")
	releaseWriteMarkdownRow(&b, "Version", normalized.Version)
	releaseWriteMarkdownRow(&b, "Environment", normalized.Environment)
	releaseWriteMarkdownRow(&b, "Strategy", string(normalized.Strategy))
	b.WriteByte('\n')

	releaseWriteMarkdownActionTable(&b, "Steps", normalized.Steps)
	releaseWriteMarkdownGateTable(&b, normalized.MigrationGates)
	releaseWriteMarkdownValidationTable(&b, normalized.Validations)
	releaseWriteMarkdownRollbackTable(&b, normalized.Rollback)

	return b.String(), nil
}

// RenderReleasePlanText renders a release plan as deterministic plain text.
func RenderReleasePlanText(plan ReleasePlan) (string, error) {
	normalized, err := normalizeReleasePlan(plan)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString("Release: ")
	b.WriteString(normalized.Name)
	b.WriteByte('\n')
	b.WriteString("Version: ")
	b.WriteString(normalized.Version)
	b.WriteByte('\n')
	b.WriteString("Environment: ")
	b.WriteString(normalized.Environment)
	b.WriteByte('\n')
	b.WriteString("Strategy: ")
	b.WriteString(string(normalized.Strategy))
	b.WriteString("\n\n")

	releaseWriteTextSteps(&b, "Steps", normalized.Steps)
	releaseWriteTextGates(&b, normalized.MigrationGates)
	releaseWriteTextValidations(&b, normalized.Validations)
	releaseWriteTextRollback(&b, normalized.Rollback)

	return b.String(), nil
}

func normalizeReleasePlan(plan ReleasePlan) (ReleasePlan, error) {
	plan.Name = strings.TrimSpace(plan.Name)
	plan.Version = strings.TrimSpace(plan.Version)
	plan.Environment = strings.TrimSpace(plan.Environment)
	plan.Strategy = ReleaseStrategy(strings.ToLower(strings.TrimSpace(string(plan.Strategy))))

	var errs []error
	if plan.Name == "" {
		errs = append(errs, invalidReleasePlan("name", "value is required"))
	} else if hasControlRune(plan.Name) {
		errs = append(errs, invalidReleasePlan("name", "contains control characters"))
	}
	if plan.Version == "" {
		errs = append(errs, invalidReleasePlan("version", "value is required"))
	} else if hasControlRune(plan.Version) {
		errs = append(errs, invalidReleasePlan("version", "contains control characters"))
	}
	if plan.Environment == "" {
		errs = append(errs, invalidReleasePlan("environment", "value is required"))
	} else if hasControlRune(plan.Environment) {
		errs = append(errs, invalidReleasePlan("environment", "contains control characters"))
	}
	switch plan.Strategy {
	case ReleaseStrategyBlueGreen, ReleaseStrategyCanary:
	case "":
		errs = append(errs, invalidReleasePlan("strategy", "value is required"))
	default:
		errs = append(errs, invalidReleasePlan("strategy", fmt.Sprintf("unsupported value %q", plan.Strategy)))
	}

	steps, stepErrs := normalizeReleaseSteps(plan.Steps, "steps")
	errs = append(errs, stepErrs...)
	if len(steps) == 0 {
		errs = append(errs, invalidReleasePlan("steps", "at least one step is required"))
	}
	plan.Steps = steps

	gates, gateErrs := normalizeMigrationGates(plan.MigrationGates, "migration_gates")
	errs = append(errs, gateErrs...)
	plan.MigrationGates = gates

	validations, validationErrs := normalizeReleaseValidations(plan.Validations, "validations")
	errs = append(errs, validationErrs...)
	plan.Validations = validations

	rollback, rollbackErrs := normalizeRollbackActions(plan.Rollback, "rollback")
	errs = append(errs, rollbackErrs...)
	if len(rollback) == 0 {
		errs = append(errs, invalidReleasePlan("rollback", "at least one action is required"))
	}
	plan.Rollback = rollback

	if err := errors.Join(errs...); err != nil {
		return ReleasePlan{}, err
	}
	return plan, nil
}

func normalizeReleaseSteps(values []ReleaseStep, field string) ([]ReleaseStep, []error) {
	return normalizeReleaseItems(values, field, "action",
		func(value ReleaseStep) (string, string) {
			return value.Name, value.Action
		},
		Step,
	)
}

func normalizeMigrationGates(values []MigrationGate, field string) ([]MigrationGate, []error) {
	return normalizeReleaseItems(values, field, "check",
		func(value MigrationGate) (string, string) {
			return value.Name, value.Check
		},
		Gate,
	)
}

func normalizeReleaseValidations(values []ReleaseValidation, field string) ([]ReleaseValidation, []error) {
	return normalizeReleaseItems(values, field, "check",
		func(value ReleaseValidation) (string, string) {
			return value.Name, value.Check
		},
		Validation,
	)
}

func normalizeRollbackActions(values []RollbackAction, field string) ([]RollbackAction, []error) {
	return normalizeReleaseItems(values, field, "action",
		func(value RollbackAction) (string, string) {
			return value.Name, value.Action
		},
		Rollback,
	)
}

func normalizeReleaseItems[T any](
	values []T,
	field string,
	valueField string,
	split func(T) (string, string),
	build func(string, string) T,
) ([]T, []error) {
	out := make([]T, 0, len(values))
	seen := make(map[string]struct{}, len(values))
	var errs []error
	for i, value := range values {
		name, detail := split(value)
		name = strings.TrimSpace(name)
		detail = strings.TrimSpace(detail)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if name == "" {
			errs = append(errs, invalidReleasePlan(itemField+".name", "value is required"))
			continue
		}
		if hasControlRune(name) {
			errs = append(errs, invalidReleasePlan(itemField+".name", "contains control characters"))
			continue
		}
		if detail == "" {
			errs = append(errs, invalidReleasePlan(itemField+"."+valueField, "value is required"))
			continue
		}
		if hasControlRune(detail) {
			errs = append(errs, invalidReleasePlan(itemField+"."+valueField, "contains control characters"))
			continue
		}
		key := strings.ToLower(name)
		if _, ok := seen[key]; ok {
			errs = append(errs, invalidReleasePlan(itemField+".name", fmt.Sprintf("duplicate %q", name)))
			continue
		}
		seen[key] = struct{}{}
		out = append(out, build(name, detail))
	}
	return out, errs
}

func normalizeCanaryPercentages(percentages []int) ([]int, error) {
	if len(percentages) == 0 {
		return []int{5, 25, 50, 100}, nil
	}

	seen := make(map[int]struct{}, len(percentages)+1)
	var errs []error
	for i, percentage := range percentages {
		if percentage < 1 || percentage > 100 {
			errs = append(errs, invalidReleasePlan(fmt.Sprintf("canary.percentages[%d]", i), "must be between 1 and 100"))
			continue
		}
		seen[percentage] = struct{}{}
	}
	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	seen[100] = struct{}{}

	normalized := make([]int, 0, len(seen))
	for percentage := range seen {
		normalized = append(normalized, percentage)
	}
	sort.Ints(normalized)
	return normalized, nil
}

func releaseDefault(value, fallback string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return fallback
	}
	return value
}

func alternateBlueGreenColor(current string) string {
	if strings.EqualFold(strings.TrimSpace(current), defaultGreenColor) {
		return defaultBlueColor
	}
	return defaultGreenColor
}

func releaseVersionLabel(version string) string {
	version = strings.TrimSpace(version)
	if version == "" {
		return "the release"
	}
	return "version " + version
}

func releaseWriteMarkdownActionTable(b *strings.Builder, title string, steps []ReleaseStep) {
	b.WriteString("## ")
	b.WriteString(title)
	b.WriteString("\n\n")
	b.WriteString("| # | Name | Action |\n")
	b.WriteString("| --- | --- | --- |\n")
	for i, step := range steps {
		releaseWriteMarkdownNumberedRow(b, i+1, step.Name, step.Action)
	}
	b.WriteByte('\n')
}

func releaseWriteMarkdownGateTable(b *strings.Builder, gates []MigrationGate) {
	b.WriteString("## Migration Gates\n\n")
	if len(gates) == 0 {
		b.WriteString("None.\n\n")
		return
	}
	b.WriteString("| # | Name | Check |\n")
	b.WriteString("| --- | --- | --- |\n")
	for i, gate := range gates {
		releaseWriteMarkdownNumberedRow(b, i+1, gate.Name, gate.Check)
	}
	b.WriteByte('\n')
}

func releaseWriteMarkdownValidationTable(b *strings.Builder, validations []ReleaseValidation) {
	b.WriteString("## Validation Checks\n\n")
	if len(validations) == 0 {
		b.WriteString("None.\n\n")
		return
	}
	b.WriteString("| # | Name | Check |\n")
	b.WriteString("| --- | --- | --- |\n")
	for i, validation := range validations {
		releaseWriteMarkdownNumberedRow(b, i+1, validation.Name, validation.Check)
	}
	b.WriteByte('\n')
}

func releaseWriteMarkdownRollbackTable(b *strings.Builder, rollback []RollbackAction) {
	b.WriteString("## Rollback Actions\n\n")
	b.WriteString("| # | Name | Action |\n")
	b.WriteString("| --- | --- | --- |\n")
	for i, action := range rollback {
		releaseWriteMarkdownNumberedRow(b, i+1, action.Name, action.Action)
	}
}

func releaseWriteMarkdownNumberedRow(b *strings.Builder, number int, name, value string) {
	b.WriteString("| ")
	b.WriteString(fmt.Sprintf("%d", number))
	b.WriteString(" | ")
	b.WriteString(releaseMarkdownCell(name))
	b.WriteString(" | ")
	b.WriteString(releaseMarkdownCell(value))
	b.WriteString(" |\n")
}

func releaseWriteMarkdownRow(b *strings.Builder, key, value string) {
	b.WriteString("| ")
	b.WriteString(releaseMarkdownCell(key))
	b.WriteString(" | ")
	b.WriteString(releaseMarkdownCell(value))
	b.WriteString(" |\n")
}

func releaseWriteTextSteps(b *strings.Builder, title string, steps []ReleaseStep) {
	b.WriteString(title)
	b.WriteString(":\n")
	for i, step := range steps {
		releaseWriteTextItem(b, i+1, step.Name, step.Action)
	}
	b.WriteByte('\n')
}

func releaseWriteTextGates(b *strings.Builder, gates []MigrationGate) {
	b.WriteString("Migration gates:\n")
	if len(gates) == 0 {
		b.WriteString("None.\n\n")
		return
	}
	for i, gate := range gates {
		releaseWriteTextItem(b, i+1, gate.Name, gate.Check)
	}
	b.WriteByte('\n')
}

func releaseWriteTextValidations(b *strings.Builder, validations []ReleaseValidation) {
	b.WriteString("Validation checks:\n")
	if len(validations) == 0 {
		b.WriteString("None.\n\n")
		return
	}
	for i, validation := range validations {
		releaseWriteTextItem(b, i+1, validation.Name, validation.Check)
	}
	b.WriteByte('\n')
}

func releaseWriteTextRollback(b *strings.Builder, rollback []RollbackAction) {
	b.WriteString("Rollback actions:\n")
	for i, action := range rollback {
		releaseWriteTextItem(b, i+1, action.Name, action.Action)
	}
}

func releaseWriteTextItem(b *strings.Builder, number int, name, value string) {
	b.WriteString(fmt.Sprintf("%d. %s: %s\n", number, name, value))
}

func releaseMarkdownHeading(value string) string {
	return strings.Join(strings.Fields(value), " ")
}

func releaseMarkdownCell(value string) string {
	value = strings.TrimSpace(value)
	value = strings.ReplaceAll(value, "|", `\|`)
	return value
}

func invalidReleasePlan(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidReleasePlan, field, detail)
}
