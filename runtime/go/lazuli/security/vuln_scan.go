package security

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

// ErrInvalidVulnerabilityScanConfig reports invalid vulnerability scan hook
// metadata, thresholds, or ignore rules.
var ErrInvalidVulnerabilityScanConfig = errors.New("lazuli/security: invalid_vulnerability_scan_config")

// VulnerabilityScannerEnvVar describes one environment variable that a deploy
// adapter may pass to a scanner command. The runtime only normalizes metadata;
// it never invokes scanner commands.
type VulnerabilityScannerEnvVar struct {
	Name  string `json:"name"`
	Value string `json:"value"`
}

// VulnerabilityScannerCommand describes provider-neutral scanner command
// metadata for generated hooks. Command is argv-style and is not executed by
// this package.
type VulnerabilityScannerCommand struct {
	Name         string                       `json:"name,omitempty"`
	Command      []string                     `json:"command,omitempty"`
	WorkingDir   string                       `json:"working_dir,omitempty"`
	Environment  []VulnerabilityScannerEnvVar `json:"environment,omitempty"`
	OutputFormat string                       `json:"output_format,omitempty"`
}

// VulnerabilityScannerEnv returns a scanner environment variable assignment.
func VulnerabilityScannerEnv(name, value string) VulnerabilityScannerEnvVar {
	return VulnerabilityScannerEnvVar{Name: name, Value: value}
}

// Validate checks that scanner command metadata can be normalized.
func (c VulnerabilityScannerCommand) Validate() error {
	_, err := c.Normalized()
	return err
}

// Normalized returns deterministic scanner command metadata. Empty metadata is
// allowed so callers can evaluate already-collected reports without a command.
func (c VulnerabilityScannerCommand) Normalized() (VulnerabilityScannerCommand, error) {
	return normalizeVulnerabilityScannerCommand(c)
}

// VulnerabilityScanThreshold configures fail/pass evaluation for a scan. A
// zero MinimumSeverity defaults to VulnerabilitySeverityLow.
type VulnerabilityScanThreshold struct {
	MinimumSeverity VulnerabilitySeverity `json:"minimum_severity,omitempty"`
	IncludeUnknown  bool                  `json:"include_unknown,omitempty"`
}

// VulnerabilityScanIgnoreRule suppresses matching findings before threshold
// evaluation. Empty string fields are wildcards. A zero Severity is also a
// wildcard; any non-zero Severity must match exactly.
type VulnerabilityScanIgnoreRule struct {
	ID           string                `json:"id,omitempty"`
	Module       string                `json:"module,omitempty"`
	Version      string                `json:"version,omitempty"`
	Package      string                `json:"package,omitempty"`
	FixedVersion string                `json:"fixed_version,omitempty"`
	Severity     VulnerabilitySeverity `json:"severity,omitempty"`
	Reason       string                `json:"reason,omitempty"`
}

// VulnerabilityIgnoredFinding records a finding ignored by a scan policy.
type VulnerabilityIgnoredFinding struct {
	Finding VulnerabilityFinding        `json:"finding"`
	Rule    VulnerabilityScanIgnoreRule `json:"rule"`
	Reason  string                      `json:"reason,omitempty"`
}

// VulnerabilityScanPolicy describes provider-neutral scan hook metadata and
// report evaluation rules.
type VulnerabilityScanPolicy struct {
	Scanner     VulnerabilityScannerCommand   `json:"scanner,omitempty"`
	Threshold   VulnerabilityScanThreshold    `json:"threshold,omitempty"`
	IgnoreRules []VulnerabilityScanIgnoreRule `json:"ignore_rules,omitempty"`
}

// VulnerabilityScanReport is a normalized report with ignored findings removed
// from the active Findings slice.
type VulnerabilityScanReport struct {
	Scanner  VulnerabilityScannerCommand   `json:"scanner,omitempty"`
	Findings []VulnerabilityFinding        `json:"findings,omitempty"`
	Ignored  []VulnerabilityIgnoredFinding `json:"ignored,omitempty"`
}

// VulnerabilityScanDecision is the fail/pass decision for a normalized scan.
type VulnerabilityScanDecision struct {
	Passed          bool                          `json:"passed"`
	Failed          bool                          `json:"failed"`
	Threshold       VulnerabilityScanThreshold    `json:"threshold"`
	HighestSeverity VulnerabilitySeverity         `json:"highest_severity"`
	FailingFindings []VulnerabilityFinding        `json:"failing_findings,omitempty"`
	IgnoredFindings []VulnerabilityIgnoredFinding `json:"ignored_findings,omitempty"`
}

// Validate checks scanner metadata, threshold, and ignore rules.
func (p VulnerabilityScanPolicy) Validate() error {
	_, err := normalizeVulnerabilityScanPolicy(p)
	return err
}

// NormalizeReport normalizes report and applies the policy's ignore rules.
func (p VulnerabilityScanPolicy) NormalizeReport(report VulnerabilityReport) (VulnerabilityScanReport, error) {
	return NormalizeVulnerabilityScanReport(report, p)
}

// Decide returns the fail/pass decision for report under policy.
func (p VulnerabilityScanPolicy) Decide(report VulnerabilityReport) (VulnerabilityScanDecision, error) {
	return DecideVulnerabilityScan(report, p)
}

// ActiveReport returns the non-ignored findings as a VulnerabilityReport.
func (r VulnerabilityScanReport) ActiveReport() VulnerabilityReport {
	return VulnerabilityReport{Findings: cloneVulnerabilityFindings(r.Findings)}
}

// NormalizeVulnerabilityScanReport normalizes report and applies ignore rules.
func NormalizeVulnerabilityScanReport(
	report VulnerabilityReport,
	policy VulnerabilityScanPolicy,
) (VulnerabilityScanReport, error) {
	normalizedPolicy, err := normalizeVulnerabilityScanPolicy(policy)
	if err != nil {
		return VulnerabilityScanReport{}, err
	}
	return normalizeVulnerabilityScanReport(report, normalizedPolicy), nil
}

// DecideVulnerabilityScan applies ignore rules and thresholds to report.
func DecideVulnerabilityScan(
	report VulnerabilityReport,
	policy VulnerabilityScanPolicy,
) (VulnerabilityScanDecision, error) {
	normalized, err := NormalizeVulnerabilityScanReport(report, policy)
	if err != nil {
		return VulnerabilityScanDecision{}, err
	}
	normalizedPolicy, err := normalizeVulnerabilityScanPolicy(policy)
	if err != nil {
		return VulnerabilityScanDecision{}, err
	}

	result := normalized.ActiveReport().EvaluateFailThreshold(normalizedPolicy.Threshold.failThreshold())
	return VulnerabilityScanDecision{
		Passed:          !result.Failed,
		Failed:          result.Failed,
		Threshold:       normalizedPolicy.Threshold,
		HighestSeverity: result.HighestSeverity,
		FailingFindings: cloneVulnerabilityFindings(result.Findings),
		IgnoredFindings: cloneVulnerabilityIgnoredFindings(normalized.Ignored),
	}, nil
}

func (t VulnerabilityScanThreshold) failThreshold() VulnerabilityFailThreshold {
	return VulnerabilityFailThreshold{
		MinimumSeverity: t.MinimumSeverity,
		IncludeUnknown:  t.IncludeUnknown,
	}
}

func normalizeVulnerabilityScanPolicy(policy VulnerabilityScanPolicy) (VulnerabilityScanPolicy, error) {
	var errs []error

	scanner, err := normalizeVulnerabilityScannerCommand(policy.Scanner)
	if err != nil {
		errs = append(errs, err)
	}
	threshold, err := normalizeVulnerabilityScanThreshold(policy.Threshold)
	if err != nil {
		errs = append(errs, err)
	}
	rules, ruleErrs := normalizeVulnerabilityScanIgnoreRules(policy.IgnoreRules)
	errs = append(errs, ruleErrs...)

	if err := errors.Join(errs...); err != nil {
		return VulnerabilityScanPolicy{}, err
	}
	return VulnerabilityScanPolicy{
		Scanner:     scanner,
		Threshold:   threshold,
		IgnoreRules: rules,
	}, nil
}

func normalizeVulnerabilityScannerCommand(command VulnerabilityScannerCommand) (VulnerabilityScannerCommand, error) {
	var errs []error
	hasMetadata := command.Name != "" ||
		len(command.Command) > 0 ||
		command.WorkingDir != "" ||
		len(command.Environment) > 0 ||
		command.OutputFormat != ""
	if !hasMetadata {
		return VulnerabilityScannerCommand{}, nil
	}

	command.Name = strings.TrimSpace(command.Name)
	if vulnerabilityScanHasControlRune(command.Name) {
		errs = append(errs, invalidVulnerabilityScanConfig("scanner.name", "cannot contain control characters"))
	}

	commandTokens, commandErrs := normalizeVulnerabilityScanCommandTokens(command.Command, "scanner.command")
	errs = append(errs, commandErrs...)
	command.Command = commandTokens
	if command.Name == "" && len(command.Command) > 0 {
		command.Name = vulnerabilityScannerNameFromCommand(command.Command[0])
	}
	if command.Name == "" {
		errs = append(errs, invalidVulnerabilityScanConfig("scanner.name", "value is required when scanner metadata is set"))
	}

	command.WorkingDir = strings.TrimSpace(command.WorkingDir)
	if vulnerabilityScanHasControlRune(command.WorkingDir) {
		errs = append(errs, invalidVulnerabilityScanConfig("scanner.working_dir", "cannot contain control characters"))
	}

	env, envErrs := normalizeVulnerabilityScannerEnv(command.Environment, "scanner.environment")
	errs = append(errs, envErrs...)
	command.Environment = env

	command.OutputFormat = strings.ToLower(strings.TrimSpace(command.OutputFormat))
	if vulnerabilityScanHasControlRune(command.OutputFormat) {
		errs = append(errs, invalidVulnerabilityScanConfig("scanner.output_format", "cannot contain control characters"))
	}

	if err := errors.Join(errs...); err != nil {
		return VulnerabilityScannerCommand{}, err
	}
	return command, nil
}

func normalizeVulnerabilityScanCommandTokens(command []string, field string) ([]string, []error) {
	if len(command) == 0 {
		return nil, nil
	}

	var errs []error
	normalized := make([]string, 0, len(command))
	for i, token := range command {
		itemField := fmt.Sprintf("%s[%d]", field, i)
		token = strings.TrimSpace(token)
		if token == "" {
			errs = append(errs, invalidVulnerabilityScanConfig(itemField, "value is required"))
			continue
		}
		if vulnerabilityScanHasControlRune(token) {
			errs = append(errs, invalidVulnerabilityScanConfig(itemField, "cannot contain control characters"))
			continue
		}
		normalized = append(normalized, token)
	}
	if len(normalized) == 0 {
		errs = append(errs, invalidVulnerabilityScanConfig(field, "at least one command token is required"))
	}
	return normalized, errs
}

func normalizeVulnerabilityScannerEnv(vars []VulnerabilityScannerEnvVar, field string) ([]VulnerabilityScannerEnvVar, []error) {
	if len(vars) == 0 {
		return nil, nil
	}

	var errs []error
	normalized := make([]VulnerabilityScannerEnvVar, 0, len(vars))
	seen := make(map[string]int, len(vars))
	for i, env := range vars {
		name := strings.TrimSpace(env.Name)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if !validVulnerabilityScanEnvName(name) {
			errs = append(errs, invalidVulnerabilityScanConfig(itemField+".name", fmt.Sprintf("invalid environment variable name %q", env.Name)))
			continue
		}
		if first, ok := seen[name]; ok {
			errs = append(errs, invalidVulnerabilityScanConfig(itemField+".name", fmt.Sprintf("duplicates %s[%d].name %q", field, first, name)))
			continue
		}
		seen[name] = i

		if vulnerabilityScanHasControlRune(env.Value) {
			errs = append(errs, invalidVulnerabilityScanConfig(itemField+"."+name, "value cannot contain control characters"))
			continue
		}
		normalized = append(normalized, VulnerabilityScannerEnvVar{Name: name, Value: env.Value})
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, errs
}

func normalizeVulnerabilityScanThreshold(threshold VulnerabilityScanThreshold) (VulnerabilityScanThreshold, error) {
	if threshold.MinimumSeverity == VulnerabilitySeverityUnknown {
		threshold.MinimumSeverity = VulnerabilitySeverityLow
	}
	if !isKnownVulnerabilityScanThresholdSeverity(threshold.MinimumSeverity) {
		return VulnerabilityScanThreshold{}, invalidVulnerabilityScanConfig("threshold.minimum_severity", "unsupported severity")
	}
	return threshold, nil
}

func normalizeVulnerabilityScanIgnoreRules(rules []VulnerabilityScanIgnoreRule) ([]VulnerabilityScanIgnoreRule, []error) {
	if len(rules) == 0 {
		return nil, nil
	}

	var errs []error
	normalized := make([]VulnerabilityScanIgnoreRule, 0, len(rules))
	for i, rule := range rules {
		norm, ruleErrs := normalizeVulnerabilityScanIgnoreRule(rule, i)
		errs = append(errs, ruleErrs...)
		if len(ruleErrs) == 0 {
			normalized = append(normalized, norm)
		}
	}
	return normalized, errs
}

func normalizeVulnerabilityScanIgnoreRule(rule VulnerabilityScanIgnoreRule, index int) (VulnerabilityScanIgnoreRule, []error) {
	field := fmt.Sprintf("ignore_rules[%d]", index)
	var errs []error

	rule.ID = strings.TrimSpace(rule.ID)
	rule.Module = strings.TrimSpace(rule.Module)
	rule.Version = strings.TrimSpace(rule.Version)
	rule.Package = strings.TrimSpace(rule.Package)
	rule.FixedVersion = strings.TrimSpace(rule.FixedVersion)
	rule.Reason = strings.TrimSpace(rule.Reason)

	for _, item := range []struct {
		name  string
		value string
	}{
		{name: "id", value: rule.ID},
		{name: "module", value: rule.Module},
		{name: "version", value: rule.Version},
		{name: "package", value: rule.Package},
		{name: "fixed_version", value: rule.FixedVersion},
		{name: "reason", value: rule.Reason},
	} {
		if vulnerabilityScanHasControlRune(item.value) {
			errs = append(errs, invalidVulnerabilityScanConfig(field+"."+item.name, "cannot contain control characters"))
		}
	}
	if !isKnownVulnerabilityScanIgnoreSeverity(rule.Severity) {
		errs = append(errs, invalidVulnerabilityScanConfig(field+".severity", "unsupported severity"))
	}
	if !rule.hasSelector() {
		errs = append(errs, invalidVulnerabilityScanConfig(field, "at least one selector is required"))
	}
	return rule, errs
}

func normalizeVulnerabilityScanReport(report VulnerabilityReport, policy VulnerabilityScanPolicy) VulnerabilityScanReport {
	out := VulnerabilityScanReport{
		Scanner:  cloneVulnerabilityScannerCommand(policy.Scanner),
		Findings: make([]VulnerabilityFinding, 0, len(report.Findings)),
	}

	activeSeen := make(map[vulnerabilityScanFindingKey]struct{}, len(report.Findings))
	ignoredSeen := make(map[vulnerabilityScanIgnoredFindingKey]struct{})
	for _, finding := range report.Findings {
		normalized, ok := normalizeVulnerabilityFinding(finding)
		if !ok {
			continue
		}
		if rule, ok := matchingVulnerabilityScanIgnoreRule(normalized, policy.IgnoreRules); ok {
			ignored := VulnerabilityIgnoredFinding{
				Finding: normalized,
				Rule:    rule,
				Reason:  rule.Reason,
			}
			key := ignored.key()
			if _, ok := ignoredSeen[key]; ok {
				continue
			}
			ignoredSeen[key] = struct{}{}
			out.Ignored = append(out.Ignored, ignored)
			continue
		}

		key := vulnerabilityScanFindingKeyFromFinding(normalized)
		if _, ok := activeSeen[key]; ok {
			continue
		}
		activeSeen[key] = struct{}{}
		out.Findings = append(out.Findings, normalized)
	}
	return out
}

func matchingVulnerabilityScanIgnoreRule(
	finding VulnerabilityFinding,
	rules []VulnerabilityScanIgnoreRule,
) (VulnerabilityScanIgnoreRule, bool) {
	for _, rule := range rules {
		if rule.matches(finding) {
			return rule, true
		}
	}
	return VulnerabilityScanIgnoreRule{}, false
}

func (r VulnerabilityScanIgnoreRule) matches(finding VulnerabilityFinding) bool {
	if r.ID != "" && r.ID != finding.ID {
		return false
	}
	if r.Module != "" && r.Module != finding.Module {
		return false
	}
	if r.Version != "" && r.Version != finding.Version {
		return false
	}
	if r.Package != "" && r.Package != finding.Package {
		return false
	}
	if r.FixedVersion != "" && r.FixedVersion != finding.FixedVersion {
		return false
	}
	if r.Severity != VulnerabilitySeverityUnknown && r.Severity != finding.Severity {
		return false
	}
	return true
}

func (r VulnerabilityScanIgnoreRule) hasSelector() bool {
	return r.ID != "" ||
		r.Module != "" ||
		r.Version != "" ||
		r.Package != "" ||
		r.FixedVersion != "" ||
		r.Severity != VulnerabilitySeverityUnknown
}

func vulnerabilityScannerNameFromCommand(command string) string {
	command = strings.TrimRight(command, `/\`)
	if index := strings.LastIndexAny(command, `/\`); index >= 0 {
		command = command[index+1:]
	}
	return command
}

func validVulnerabilityScanEnvName(name string) bool {
	if name == "" {
		return false
	}
	for i, r := range name {
		if r > unicode.MaxASCII {
			return false
		}
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r == '_':
		case i > 0 && r >= '0' && r <= '9':
		default:
			return false
		}
	}
	return true
}

func vulnerabilityScanHasControlRune(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func isKnownVulnerabilityScanThresholdSeverity(severity VulnerabilitySeverity) bool {
	switch severity {
	case VulnerabilitySeverityNone,
		VulnerabilitySeverityLow,
		VulnerabilitySeverityMedium,
		VulnerabilitySeverityHigh,
		VulnerabilitySeverityCritical:
		return true
	default:
		return false
	}
}

func isKnownVulnerabilityScanIgnoreSeverity(severity VulnerabilitySeverity) bool {
	return severity == VulnerabilitySeverityUnknown || isKnownVulnerabilityScanThresholdSeverity(severity)
}

type vulnerabilityScanFindingKey struct {
	id           string
	summary      string
	module       string
	version      string
	pkg          string
	fixedVersion string
	severity     VulnerabilitySeverity
}

type vulnerabilityScanIgnoredFindingKey struct {
	finding vulnerabilityScanFindingKey
	rule    vulnerabilityScanIgnoreRuleKey
}

type vulnerabilityScanIgnoreRuleKey struct {
	id           string
	module       string
	version      string
	pkg          string
	fixedVersion string
	severity     VulnerabilitySeverity
	reason       string
}

func vulnerabilityScanFindingKeyFromFinding(finding VulnerabilityFinding) vulnerabilityScanFindingKey {
	return vulnerabilityScanFindingKey{
		id:           finding.ID,
		summary:      finding.Summary,
		module:       finding.Module,
		version:      finding.Version,
		pkg:          finding.Package,
		fixedVersion: finding.FixedVersion,
		severity:     finding.Severity,
	}
}

func vulnerabilityScanIgnoreRuleKeyFromRule(rule VulnerabilityScanIgnoreRule) vulnerabilityScanIgnoreRuleKey {
	return vulnerabilityScanIgnoreRuleKey{
		id:           rule.ID,
		module:       rule.Module,
		version:      rule.Version,
		pkg:          rule.Package,
		fixedVersion: rule.FixedVersion,
		severity:     rule.Severity,
		reason:       rule.Reason,
	}
}

func (i VulnerabilityIgnoredFinding) key() vulnerabilityScanIgnoredFindingKey {
	return vulnerabilityScanIgnoredFindingKey{
		finding: vulnerabilityScanFindingKeyFromFinding(i.Finding),
		rule:    vulnerabilityScanIgnoreRuleKeyFromRule(i.Rule),
	}
}

func cloneVulnerabilityFindings(findings []VulnerabilityFinding) []VulnerabilityFinding {
	if len(findings) == 0 {
		return nil
	}
	return append([]VulnerabilityFinding(nil), findings...)
}

func cloneVulnerabilityIgnoredFindings(findings []VulnerabilityIgnoredFinding) []VulnerabilityIgnoredFinding {
	if len(findings) == 0 {
		return nil
	}
	out := make([]VulnerabilityIgnoredFinding, len(findings))
	copy(out, findings)
	return out
}

func cloneVulnerabilityScannerCommand(command VulnerabilityScannerCommand) VulnerabilityScannerCommand {
	command.Command = append([]string(nil), command.Command...)
	command.Environment = append([]VulnerabilityScannerEnvVar(nil), command.Environment...)
	return command
}

func invalidVulnerabilityScanConfig(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidVulnerabilityScanConfig, field, detail)
}
