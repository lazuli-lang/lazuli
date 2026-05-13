package deploy

import (
	"errors"
	"fmt"
	"strings"
)

// ErrInvalidBlueGreenPlan reports an invalid blue-green deployment plan.
var ErrInvalidBlueGreenPlan = errors.New("lazuli/deploy: invalid blue-green plan")

// BlueGreenDeployConfig configures a provider-neutral blue-green deploy plan.
// It describes slots, planned traffic movement, health gates, and rollback
// hints only. It does not execute provider calls.
type BlueGreenDeployConfig struct {
	Name        string
	Environment string

	ActiveSlot    string
	CandidateSlot string

	TrafficShifts []BlueGreenTrafficShift
	HealthGates   []BlueGreenHealthGate
	RollbackHints []BlueGreenRollbackHint
}

// BlueGreenDeployPlan is the normalized dry-run plan for a blue-green deploy.
type BlueGreenDeployPlan struct {
	DryRun      bool
	Name        string
	Environment string

	ActiveSlot    BlueGreenSlot
	CandidateSlot BlueGreenSlot

	TrafficPhases []BlueGreenTrafficPhase
	HealthGates   []BlueGreenHealthGate
	RollbackHints []BlueGreenRollbackHint
}

// BlueGreenSlot identifies a deploy slot and its current planned traffic.
type BlueGreenSlot struct {
	Name           string
	TrafficPercent int
}

// BlueGreenTrafficShift configures one candidate traffic target in a rollout.
// Empty health gate names use all configured health gates for that phase.
type BlueGreenTrafficShift struct {
	Name             string
	CandidatePercent int
	HealthGateNames  []string
}

// BlueGreenTrafficPhase is one normalized traffic movement phase.
type BlueGreenTrafficPhase struct {
	Name             string
	ActivePercent    int
	CandidatePercent int
	HealthGateNames  []string
}

// BlueGreenHealthGate is one blocking deploy gate checked by deploy adapters.
type BlueGreenHealthGate struct {
	Name  string
	Check string
}

// BlueGreenRollbackHint is one rollback hint for the deploy operator.
type BlueGreenRollbackHint struct {
	Name string
	Hint string
}

// TrafficShift returns a blue-green traffic shift targeting candidatePercent.
func TrafficShift(candidatePercent int, healthGateNames ...string) BlueGreenTrafficShift {
	return BlueGreenTrafficShift{
		CandidatePercent: candidatePercent,
		HealthGateNames:  append([]string(nil), healthGateNames...),
	}
}

// HealthCheck returns a blue-green health gate.
func HealthCheck(name, check string) BlueGreenHealthGate {
	return BlueGreenHealthGate{Name: name, Check: check}
}

// RollbackHint returns a blue-green rollback hint.
func RollbackHint(name, hint string) BlueGreenRollbackHint {
	return BlueGreenRollbackHint{Name: name, Hint: hint}
}

// Validate checks whether the blue-green config can produce a dry-run plan.
func (c BlueGreenDeployConfig) Validate() error {
	return ValidateBlueGreenDeployConfig(c)
}

// Plan returns the normalized dry-run blue-green deploy plan.
func (c BlueGreenDeployConfig) Plan() (BlueGreenDeployPlan, error) {
	return BuildBlueGreenDeployPlan(c)
}

// Validate checks the blue-green deploy plan.
func (p BlueGreenDeployPlan) Validate() error {
	return ValidateBlueGreenDeployPlan(p)
}

// RenderText renders the blue-green deploy plan as deterministic plain text.
func (p BlueGreenDeployPlan) RenderText() (string, error) {
	return RenderBlueGreenDeployPlanText(p)
}

// ValidateBlueGreenDeployConfig reports whether config can produce a dry-run
// blue-green deploy plan.
func ValidateBlueGreenDeployConfig(config BlueGreenDeployConfig) error {
	_, err := BuildBlueGreenDeployPlan(config)
	return err
}

// BuildBlueGreenDeployPlan returns a normalized dry-run blue-green deploy plan.
// It does not call any provider APIs.
func BuildBlueGreenDeployPlan(config BlueGreenDeployConfig) (BlueGreenDeployPlan, error) {
	active := releaseDefault(config.ActiveSlot, defaultBlueColor)
	candidate := strings.TrimSpace(config.CandidateSlot)
	if candidate == "" {
		candidate = alternateBlueGreenColor(active)
	}

	healthGates, healthErrs := normalizeBlueGreenHealthGates(config.HealthGates)
	trafficPhases, phaseErrs := normalizeBlueGreenTrafficShifts(config.TrafficShifts, healthGates)
	rollbackHints := append(defaultBlueGreenRollbackHints(active, candidate), config.RollbackHints...)
	normalizedRollback, rollbackErrs := normalizeBlueGreenRollbackHints(rollbackHints)

	plan := BlueGreenDeployPlan{
		DryRun:      true,
		Name:        releaseDefault(config.Name, defaultReleaseName),
		Environment: releaseDefault(config.Environment, defaultReleaseEnvironment),
		ActiveSlot: BlueGreenSlot{
			Name:           active,
			TrafficPercent: 100,
		},
		CandidateSlot: BlueGreenSlot{
			Name:           candidate,
			TrafficPercent: 0,
		},
		TrafficPhases: trafficPhases,
		HealthGates:   healthGates,
		RollbackHints: normalizedRollback,
	}

	var errs []error
	errs = append(errs, healthErrs...)
	errs = append(errs, phaseErrs...)
	errs = append(errs, rollbackErrs...)
	if err := errors.Join(errs...); err != nil {
		return BlueGreenDeployPlan{}, err
	}
	return normalizeBlueGreenDeployPlan(plan)
}

// ValidateBlueGreenDeployPlan validates a dry-run blue-green deploy plan.
func ValidateBlueGreenDeployPlan(plan BlueGreenDeployPlan) error {
	_, err := normalizeBlueGreenDeployPlan(plan)
	return err
}

// RenderBlueGreenDeployPlanText renders a deterministic blue-green deploy
// plan. It does not execute health gates or provider calls.
func RenderBlueGreenDeployPlanText(plan BlueGreenDeployPlan) (string, error) {
	normalized, err := normalizeBlueGreenDeployPlan(plan)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString("Blue-green deploy: ")
	b.WriteString(normalized.Name)
	b.WriteByte('\n')
	b.WriteString("Environment: ")
	b.WriteString(normalized.Environment)
	b.WriteByte('\n')
	b.WriteString("Dry run: true\n\n")

	b.WriteString("Slots:\n")
	writeBlueGreenSlot(&b, 1, "active", normalized.ActiveSlot)
	writeBlueGreenSlot(&b, 2, "candidate", normalized.CandidateSlot)
	b.WriteByte('\n')

	b.WriteString("Traffic phases:\n")
	for i, phase := range normalized.TrafficPhases {
		b.WriteString(fmt.Sprintf("%d. %s: %s %d%%, %s %d%%",
			i+1,
			phase.Name,
			normalized.ActiveSlot.Name,
			phase.ActivePercent,
			normalized.CandidateSlot.Name,
			phase.CandidatePercent,
		))
		if len(phase.HealthGateNames) > 0 {
			b.WriteString("; gates: ")
			b.WriteString(strings.Join(phase.HealthGateNames, ", "))
		}
		b.WriteByte('\n')
	}
	b.WriteByte('\n')

	b.WriteString("Health gates:\n")
	if len(normalized.HealthGates) == 0 {
		b.WriteString("None.\n\n")
	} else {
		for i, gate := range normalized.HealthGates {
			b.WriteString(fmt.Sprintf("%d. %s: %s\n", i+1, gate.Name, gate.Check))
		}
		b.WriteByte('\n')
	}

	b.WriteString("Rollback hints:\n")
	for i, hint := range normalized.RollbackHints {
		b.WriteString(fmt.Sprintf("%d. %s: %s\n", i+1, hint.Name, hint.Hint))
	}
	return b.String(), nil
}

func normalizeBlueGreenDeployPlan(plan BlueGreenDeployPlan) (BlueGreenDeployPlan, error) {
	var errs []error

	plan.Name = strings.TrimSpace(plan.Name)
	if plan.Name == "" {
		errs = append(errs, invalidBlueGreenPlan("name", "value is required"))
	} else if hasControlRune(plan.Name) {
		errs = append(errs, invalidBlueGreenPlan("name", "contains control characters"))
	}

	plan.Environment = strings.TrimSpace(plan.Environment)
	if plan.Environment == "" {
		errs = append(errs, invalidBlueGreenPlan("environment", "value is required"))
	} else if hasControlRune(plan.Environment) {
		errs = append(errs, invalidBlueGreenPlan("environment", "contains control characters"))
	}

	if !plan.DryRun {
		errs = append(errs, invalidBlueGreenPlan("dry_run", "must be true"))
	}

	active, activeErrs := normalizeBlueGreenSlot(plan.ActiveSlot, "active_slot")
	candidate, candidateErrs := normalizeBlueGreenSlot(plan.CandidateSlot, "candidate_slot")
	errs = append(errs, activeErrs...)
	errs = append(errs, candidateErrs...)
	if active.Name != "" && candidate.Name != "" && strings.EqualFold(active.Name, candidate.Name) {
		errs = append(errs, invalidBlueGreenPlan("candidate_slot.name", "must differ from active_slot.name"))
	}
	if active.TrafficPercent != 100 {
		errs = append(errs, invalidBlueGreenPlan("active_slot.traffic_percent", "must be 100 before traffic phases run"))
	}
	if candidate.TrafficPercent != 0 {
		errs = append(errs, invalidBlueGreenPlan("candidate_slot.traffic_percent", "must be 0 before traffic phases run"))
	}
	plan.ActiveSlot = active
	plan.CandidateSlot = candidate

	healthGates, healthErrs := normalizeBlueGreenHealthGates(plan.HealthGates)
	errs = append(errs, healthErrs...)
	plan.HealthGates = healthGates

	trafficPhases, phaseErrs := normalizeBlueGreenTrafficPhases(plan.TrafficPhases, healthGates)
	errs = append(errs, phaseErrs...)
	plan.TrafficPhases = trafficPhases

	rollbackHints, rollbackErrs := normalizeBlueGreenRollbackHints(plan.RollbackHints)
	errs = append(errs, rollbackErrs...)
	if len(rollbackHints) == 0 {
		errs = append(errs, invalidBlueGreenPlan("rollback_hints", "at least one hint is required"))
	}
	plan.RollbackHints = rollbackHints

	if err := errors.Join(errs...); err != nil {
		return BlueGreenDeployPlan{}, err
	}
	return plan, nil
}

func normalizeBlueGreenSlot(slot BlueGreenSlot, field string) (BlueGreenSlot, []error) {
	var errs []error
	slot.Name = strings.TrimSpace(slot.Name)
	if slot.Name == "" {
		errs = append(errs, invalidBlueGreenPlan(field+".name", "value is required"))
	} else if !validComposeName(slot.Name) {
		errs = append(errs, invalidBlueGreenPlan(field+".name", fmt.Sprintf("invalid name %q", slot.Name)))
	}
	if slot.TrafficPercent < 0 || slot.TrafficPercent > 100 {
		errs = append(errs, invalidBlueGreenPlan(field+".traffic_percent", "must be between 0 and 100"))
	}
	return slot, errs
}

func normalizeBlueGreenTrafficShifts(shifts []BlueGreenTrafficShift, gates []BlueGreenHealthGate) ([]BlueGreenTrafficPhase, []error) {
	if len(shifts) == 0 {
		shifts = defaultBlueGreenTrafficShifts(allBlueGreenHealthGateNames(gates))
	} else {
		shifts = append([]BlueGreenTrafficShift(nil), shifts...)
		defaultGateNames := allBlueGreenHealthGateNames(gates)
		if shifts[0].CandidatePercent != 0 {
			shifts = append([]BlueGreenTrafficShift{TrafficShift(0, defaultGateNames...)}, shifts...)
		}
		if shifts[len(shifts)-1].CandidatePercent != 100 {
			shifts = append(shifts, TrafficShift(100, defaultGateNames...))
		}
	}

	phases := make([]BlueGreenTrafficPhase, 0, len(shifts))
	for _, shift := range shifts {
		phases = append(phases, BlueGreenTrafficPhase{
			Name:             shift.Name,
			CandidatePercent: shift.CandidatePercent,
			HealthGateNames:  append([]string(nil), shift.HealthGateNames...),
		})
	}
	return normalizeBlueGreenTrafficPhases(phases, gates)
}

func normalizeBlueGreenTrafficPhases(phases []BlueGreenTrafficPhase, gates []BlueGreenHealthGate) ([]BlueGreenTrafficPhase, []error) {
	var errs []error
	if len(phases) == 0 {
		return nil, []error{invalidBlueGreenPlan("traffic_phases", "at least one phase is required")}
	}

	gateByName := make(map[string]string, len(gates))
	for _, gate := range gates {
		gateByName[strings.ToLower(gate.Name)] = gate.Name
	}
	defaultGateNames := allBlueGreenHealthGateNames(gates)

	out := make([]BlueGreenTrafficPhase, 0, len(phases))
	seenNames := make(map[string]struct{}, len(phases))
	previousPercent := -1
	for i, phase := range phases {
		field := fmt.Sprintf("traffic_phases[%d]", i)
		phase.Name = strings.TrimSpace(phase.Name)
		if phase.Name == "" {
			phase.Name = defaultBlueGreenPhaseName(phase.CandidatePercent)
		}
		if hasControlRune(phase.Name) {
			errs = append(errs, invalidBlueGreenPlan(field+".name", "contains control characters"))
		}
		nameKey := strings.ToLower(phase.Name)
		if _, ok := seenNames[nameKey]; ok {
			errs = append(errs, invalidBlueGreenPlan(field+".name", fmt.Sprintf("duplicate %q", phase.Name)))
		} else {
			seenNames[nameKey] = struct{}{}
		}

		if phase.CandidatePercent < 0 || phase.CandidatePercent > 100 {
			errs = append(errs, invalidBlueGreenPlan(field+".candidate_percent", "must be between 0 and 100"))
		}
		if previousPercent >= 0 && phase.CandidatePercent <= previousPercent {
			errs = append(errs, invalidBlueGreenPlan(field+".candidate_percent", "must increase from the previous phase"))
		}
		previousPercent = phase.CandidatePercent

		expectedActive := 100 - phase.CandidatePercent
		if phase.ActivePercent == 0 && expectedActive != 0 {
			phase.ActivePercent = expectedActive
		}
		if phase.ActivePercent != expectedActive {
			errs = append(errs, invalidBlueGreenPlan(field+".active_percent", "must equal 100 minus candidate_percent"))
		}

		gateNames := phase.HealthGateNames
		if len(gateNames) == 0 && len(defaultGateNames) > 0 {
			gateNames = defaultGateNames
		}
		normalizedGateNames, gateErrs := normalizeBlueGreenGateNames(gateNames, gateByName, field+".health_gate_names")
		errs = append(errs, gateErrs...)
		phase.HealthGateNames = normalizedGateNames

		out = append(out, phase)
	}

	if len(out) > 0 {
		if out[0].CandidatePercent != 0 {
			errs = append(errs, invalidBlueGreenPlan("traffic_phases[0].candidate_percent", "first phase must start at 0"))
		}
		if out[len(out)-1].CandidatePercent != 100 {
			errs = append(errs, invalidBlueGreenPlan(fmt.Sprintf("traffic_phases[%d].candidate_percent", len(out)-1), "last phase must promote the candidate to 100"))
		}
	}

	if err := errors.Join(errs...); err != nil {
		return nil, errs
	}
	return out, nil
}

func normalizeBlueGreenHealthGates(gates []BlueGreenHealthGate) ([]BlueGreenHealthGate, []error) {
	out := make([]BlueGreenHealthGate, 0, len(gates))
	seen := make(map[string]struct{}, len(gates))
	var errs []error
	for i, gate := range gates {
		field := fmt.Sprintf("health_gates[%d]", i)
		gate.Name = strings.TrimSpace(gate.Name)
		gate.Check = strings.TrimSpace(gate.Check)
		if gate.Name == "" {
			errs = append(errs, invalidBlueGreenPlan(field+".name", "value is required"))
			continue
		}
		if hasControlRune(gate.Name) {
			errs = append(errs, invalidBlueGreenPlan(field+".name", "contains control characters"))
			continue
		}
		key := strings.ToLower(gate.Name)
		if _, ok := seen[key]; ok {
			errs = append(errs, invalidBlueGreenPlan(field+".name", fmt.Sprintf("duplicate %q", gate.Name)))
			continue
		}
		if gate.Check == "" {
			errs = append(errs, invalidBlueGreenPlan(field+".check", "value is required"))
			continue
		}
		if hasControlRune(gate.Check) {
			errs = append(errs, invalidBlueGreenPlan(field+".check", "contains control characters"))
			continue
		}
		seen[key] = struct{}{}
		out = append(out, gate)
	}
	return out, errs
}

func normalizeBlueGreenGateNames(names []string, gateByName map[string]string, field string) ([]string, []error) {
	out := make([]string, 0, len(names))
	seen := make(map[string]struct{}, len(names))
	var errs []error
	for i, name := range names {
		itemField := fmt.Sprintf("%s[%d]", field, i)
		name = strings.TrimSpace(name)
		if name == "" {
			errs = append(errs, invalidBlueGreenPlan(itemField, "value is required"))
			continue
		}
		if hasControlRune(name) {
			errs = append(errs, invalidBlueGreenPlan(itemField, "contains control characters"))
			continue
		}
		key := strings.ToLower(name)
		canonical, ok := gateByName[key]
		if !ok {
			errs = append(errs, invalidBlueGreenPlan(itemField, fmt.Sprintf("unknown health gate %q", name)))
			continue
		}
		if _, ok := seen[key]; ok {
			errs = append(errs, invalidBlueGreenPlan(itemField, fmt.Sprintf("duplicate health gate %q", name)))
			continue
		}
		seen[key] = struct{}{}
		out = append(out, canonical)
	}
	return out, errs
}

func normalizeBlueGreenRollbackHints(hints []BlueGreenRollbackHint) ([]BlueGreenRollbackHint, []error) {
	out := make([]BlueGreenRollbackHint, 0, len(hints))
	seen := make(map[string]struct{}, len(hints))
	var errs []error
	for i, hint := range hints {
		field := fmt.Sprintf("rollback_hints[%d]", i)
		hint.Name = strings.TrimSpace(hint.Name)
		hint.Hint = strings.TrimSpace(hint.Hint)
		if hint.Name == "" {
			errs = append(errs, invalidBlueGreenPlan(field+".name", "value is required"))
			continue
		}
		if hasControlRune(hint.Name) {
			errs = append(errs, invalidBlueGreenPlan(field+".name", "contains control characters"))
			continue
		}
		key := strings.ToLower(hint.Name)
		if _, ok := seen[key]; ok {
			errs = append(errs, invalidBlueGreenPlan(field+".name", fmt.Sprintf("duplicate %q", hint.Name)))
			continue
		}
		if hint.Hint == "" {
			errs = append(errs, invalidBlueGreenPlan(field+".hint", "value is required"))
			continue
		}
		if hasControlRune(hint.Hint) {
			errs = append(errs, invalidBlueGreenPlan(field+".hint", "contains control characters"))
			continue
		}
		seen[key] = struct{}{}
		out = append(out, hint)
	}
	return out, errs
}

func defaultBlueGreenTrafficShifts(gateNames []string) []BlueGreenTrafficShift {
	return []BlueGreenTrafficShift{
		TrafficShift(0, gateNames...),
		TrafficShift(10, gateNames...),
		TrafficShift(50, gateNames...),
		TrafficShift(100, gateNames...),
	}
}

func defaultBlueGreenRollbackHints(active, candidate string) []BlueGreenRollbackHint {
	return []BlueGreenRollbackHint{
		RollbackHint("restore active slot", fmt.Sprintf("Route 100%% traffic to %s and 0%% to %s.", active, candidate)),
		RollbackHint("hold candidate slot", fmt.Sprintf("Keep %s deployed for logs and investigation while %s serves traffic.", candidate, active)),
	}
}

func defaultBlueGreenPhaseName(candidatePercent int) string {
	switch candidatePercent {
	case 0:
		return "warm candidate"
	case 100:
		return "promote candidate"
	default:
		return fmt.Sprintf("shift %d%% traffic", candidatePercent)
	}
}

func allBlueGreenHealthGateNames(gates []BlueGreenHealthGate) []string {
	names := make([]string, 0, len(gates))
	for _, gate := range gates {
		names = append(names, gate.Name)
	}
	return names
}

func writeBlueGreenSlot(b *strings.Builder, number int, role string, slot BlueGreenSlot) {
	b.WriteString(fmt.Sprintf("%d. %s: %s (%d%% traffic)\n", number, role, slot.Name, slot.TrafficPercent))
}

func invalidBlueGreenPlan(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidBlueGreenPlan, field, detail)
}
