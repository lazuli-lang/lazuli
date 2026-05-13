package deploy

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"

	"lazuli.dev/runtime/lazuli/migrations"
)

const (
	// DefaultMigrationGateTiming is used when deploy policy leaves migration
	// gate timing to the adapter.
	DefaultMigrationGateTiming = MigrationGateTimingBeforeDeploy
	// DefaultMigrationLockPolicy is used when deploy policy leaves migration
	// lock behavior to the adapter.
	DefaultMigrationLockPolicy = MigrationLockRequired
	// DefaultMigrationDestructivePolicy is used when deploy policy leaves
	// destructive migration handling to the adapter.
	DefaultMigrationDestructivePolicy = MigrationDestructiveRequireApproval
)

// ErrInvalidMigrationGateConfig reports an invalid deploy migration gate plan.
var ErrInvalidMigrationGateConfig = errors.New("lazuli/deploy: invalid migration gate config")

// MigrationGateTiming names when deploy migration gates participate in a
// release. It mirrors migrations.DeployPolicy.Migrations.
type MigrationGateTiming string

const (
	// MigrationGateTimingBeforeDeploy blocks deployment before traffic moves.
	MigrationGateTimingBeforeDeploy MigrationGateTiming = "before_deploy"
	// MigrationGateTimingManual records gates for a human-controlled rollout.
	MigrationGateTimingManual MigrationGateTiming = "manual"
	// MigrationGateTimingDisabled omits normal migration gates.
	MigrationGateTimingDisabled MigrationGateTiming = "disabled"
)

// MigrationGatePhase identifies the deploy phase where a planned hook or gate
// belongs.
type MigrationGatePhase string

const (
	// MigrationGatePhasePreMigration runs before migration application.
	MigrationGatePhasePreMigration MigrationGatePhase = "pre_migration"
	// MigrationGatePhaseBeforeDeploy runs before the deploy promotes traffic.
	MigrationGatePhaseBeforeDeploy MigrationGatePhase = "before_deploy"
	// MigrationGatePhasePostMigration runs after migration application.
	MigrationGatePhasePostMigration MigrationGatePhase = "post_migration"
	// MigrationGatePhaseManual is recorded for manual migration gates.
	MigrationGatePhaseManual MigrationGatePhase = "manual"
)

// MigrationLockPolicy names the advisory lock behavior for migrations.
type MigrationLockPolicy string

const (
	// MigrationLockRequired fails the deploy when the migration lock cannot be
	// acquired before its timeout.
	MigrationLockRequired MigrationLockPolicy = "required"
	// MigrationLockOptional records lock timeout decisions without blocking the
	// deploy.
	MigrationLockOptional MigrationLockPolicy = "optional"
	// MigrationLockNone disables migration lock acquisition.
	MigrationLockNone MigrationLockPolicy = "none"
)

// MigrationDestructivePolicy names how destructive migration steps affect the
// deploy gate plan.
type MigrationDestructivePolicy string

const (
	// MigrationDestructiveRequireApproval blocks unapproved destructive steps.
	MigrationDestructiveRequireApproval MigrationDestructivePolicy = "require_approval"
	// MigrationDestructiveAllow records destructive steps without blocking.
	MigrationDestructiveAllow MigrationDestructivePolicy = "allow"
	// MigrationDestructiveBlock blocks all destructive steps.
	MigrationDestructiveBlock MigrationDestructivePolicy = "block"
)

// MigrationTimeoutAction describes the deploy decision when a planned timeout
// is reached by a future executor.
type MigrationTimeoutAction string

const (
	// MigrationTimeoutFailDeploy means the deploy should fail on timeout.
	MigrationTimeoutFailDeploy MigrationTimeoutAction = "fail_deploy"
	// MigrationTimeoutContinue means timeout should be surfaced but not block
	// the deploy phase represented by this plan.
	MigrationTimeoutContinue MigrationTimeoutAction = "continue"
	// MigrationTimeoutIgnore means the timeout is ignored for this policy.
	MigrationTimeoutIgnore MigrationTimeoutAction = "ignore"
)

// MigrationGatePlanConfig connects migration plans and deploy policy into a
// side-effect-free deploy gate plan.
type MigrationGatePlanConfig struct {
	Policy      migrations.DeployPolicy
	OnlinePlan  migrations.OnlineMigrationPlan
	TenantPlan  migrations.TenantMigrationPlan
	UpgradePlan migrations.UpgradePlan
	Contracts   []migrations.TenantMigrationContract
}

// MigrationGatePlan is the normalized dry-run deploy migration gate plan.
type MigrationGatePlan struct {
	DryRun      bool
	Timing      MigrationGateTiming
	Lock        MigrationLockDecision
	Destructive MigrationDestructiveDecision
	Hooks       []MigrationDeployHook
	Gates       []MigrationGateDecision
	Timeouts    []MigrationTimeoutDecision
}

// MigrationLockDecision is the normalized advisory-lock decision.
type MigrationLockDecision struct {
	Policy         MigrationLockPolicy
	Timeout        time.Duration
	AdapterDefault bool
	Blocking       bool
	TimeoutAction  MigrationTimeoutAction
}

// MigrationDestructiveDecision summarizes destructive online migration steps.
type MigrationDestructiveDecision struct {
	Policy           MigrationDestructivePolicy
	DestructiveSteps int
	ApprovedSteps    int
	Blocking         bool
	Reason           string
}

// MigrationDeployHook records a deploy hook without executing it.
type MigrationDeployHook struct {
	Name     string
	Phase    MigrationGatePhase
	Command  string
	Blocking bool
}

// MigrationGateDecision records one migration gate and whether it blocks the
// deploy phase.
type MigrationGateDecision struct {
	Name     string
	Phase    MigrationGatePhase
	Blocking bool
	Count    int
	Reason   string
}

// MigrationTimeoutDecision records a timeout source and the deploy decision a
// future executor should apply when it expires.
type MigrationTimeoutDecision struct {
	Name           string
	Scope          string
	Phase          MigrationGatePhase
	Timeout        time.Duration
	AdapterDefault bool
	Source         string
	Action         MigrationTimeoutAction
}

// Validate checks that config can produce a dry-run migration gate plan.
func (c MigrationGatePlanConfig) Validate() error {
	return ValidateMigrationGateConfig(c)
}

// Plan returns the normalized dry-run migration gate plan.
func (c MigrationGatePlanConfig) Plan() (MigrationGatePlan, error) {
	return BuildMigrationGatePlan(c)
}

// RenderDryRunSummary renders the normalized migration gate plan without
// executing hooks or migrations.
func (c MigrationGatePlanConfig) RenderDryRunSummary() (string, error) {
	return RenderMigrationGateDryRunSummary(c)
}

// RenderDryRunPlan renders the normalized migration gate plan without
// executing hooks or migrations.
func (c MigrationGatePlanConfig) RenderDryRunPlan() (string, error) {
	return RenderMigrationGateDryRunSummary(c)
}

// ValidateMigrationGateConfig checks whether config can produce a dry-run plan.
func ValidateMigrationGateConfig(config MigrationGatePlanConfig) error {
	_, err := BuildMigrationGatePlan(config)
	return err
}

// BuildMigrationGatePlan returns a normalized migration gate plan. It does not
// execute hooks, acquire locks, or run migrations.
func BuildMigrationGatePlan(config MigrationGatePlanConfig) (MigrationGatePlan, error) {
	timing, lockPolicy, destructivePolicy, normalizedPolicy, err := normalizeMigrationGatePolicy(config.Policy)
	if err != nil {
		return MigrationGatePlan{}, err
	}

	destructive := buildMigrationDestructiveDecision(destructivePolicy, config.OnlinePlan)
	plan := MigrationGatePlan{
		DryRun:      true,
		Timing:      timing,
		Lock:        buildMigrationLockDecision(lockPolicy, normalizedPolicy.LockTimeout),
		Destructive: destructive,
	}

	var errs []error
	hooks, hookErrs := buildMigrationDeployHooks(normalizedPolicy)
	errs = append(errs, hookErrs...)
	plan.Hooks = hooks

	if timing != MigrationGateTimingDisabled {
		onlineGates, onlineErrs := buildOnlineMigrationGates(timing, config.OnlinePlan)
		errs = append(errs, onlineErrs...)
		plan.Gates = append(plan.Gates, onlineGates...)
		plan.Gates = append(plan.Gates, buildTenantMigrationGate(timing, config.TenantPlan)...)
		plan.Gates = append(plan.Gates, buildUpgradeMigrationGate(timing, config.UpgradePlan)...)
	}
	if destructive.Blocking {
		plan.Gates = append(plan.Gates, buildDestructiveMigrationGate(timing, destructive))
	}

	timeouts, timeoutErrs := buildMigrationTimeoutDecisions(timing, plan.Lock, config.Contracts)
	errs = append(errs, timeoutErrs...)
	plan.Timeouts = timeouts

	if err := errors.Join(errs...); err != nil {
		return MigrationGatePlan{}, err
	}
	return plan, nil
}

// RenderMigrationGateDryRunPlan renders a deterministic YAML-like dry-run
// summary. It does not execute hooks, acquire locks, or run migrations.
func RenderMigrationGateDryRunPlan(config MigrationGatePlanConfig) (string, error) {
	return RenderMigrationGateDryRunSummary(config)
}

// RenderMigrationGateDryRunSummary renders a deterministic YAML-like dry-run
// summary. It does not execute hooks, acquire locks, or run migrations.
func RenderMigrationGateDryRunSummary(config MigrationGatePlanConfig) (string, error) {
	plan, err := BuildMigrationGatePlan(config)
	if err != nil {
		return "", err
	}
	return renderMigrationGatePlan(plan), nil
}

// BlockingGates returns a copy of the blocking migration gates.
func (p MigrationGatePlan) BlockingGates() []MigrationGateDecision {
	return filterMigrationGates(p.Gates, true)
}

// NonBlockingGates returns a copy of the nonblocking migration gates.
func (p MigrationGatePlan) NonBlockingGates() []MigrationGateDecision {
	return filterMigrationGates(p.Gates, false)
}

// ReleaseGates returns blocking gates in the existing ReleasePlan gate shape.
func (p MigrationGatePlan) ReleaseGates() []MigrationGate {
	blocking := p.BlockingGates()
	gates := make([]MigrationGate, 0, len(blocking))
	for _, gate := range blocking {
		gates = append(gates, Gate(gate.Name, gate.Reason))
	}
	return gates
}

func normalizeMigrationGatePolicy(policy migrations.DeployPolicy) (
	MigrationGateTiming,
	MigrationLockPolicy,
	MigrationDestructivePolicy,
	migrations.DeployPolicy,
	error,
) {
	var errs []error
	policy.Migrations = strings.ToLower(strings.TrimSpace(policy.Migrations))
	policy.MigrationLock = strings.ToLower(strings.TrimSpace(policy.MigrationLock))
	policy.DestructiveMigrations = strings.ToLower(strings.TrimSpace(policy.DestructiveMigrations))
	if hasControlRune(policy.PreHook) {
		errs = append(errs, invalidMigrationGateConfig("policy.pre_hook", "contains control characters"))
	}
	if hasControlRune(policy.PostHook) {
		errs = append(errs, invalidMigrationGateConfig("policy.post_hook", "contains control characters"))
	}
	policy.PreHook = strings.TrimSpace(policy.PreHook)
	policy.PostHook = strings.TrimSpace(policy.PostHook)

	timing := MigrationGateTiming(policy.Migrations)
	switch timing {
	case "":
		timing = DefaultMigrationGateTiming
	case MigrationGateTimingBeforeDeploy, MigrationGateTimingManual, MigrationGateTimingDisabled:
	default:
		errs = append(errs, invalidMigrationGateConfig("policy.migrations", fmt.Sprintf("unsupported value %q", policy.Migrations)))
	}

	lockPolicy := MigrationLockPolicy(policy.MigrationLock)
	switch lockPolicy {
	case "":
		lockPolicy = DefaultMigrationLockPolicy
	case MigrationLockRequired, MigrationLockOptional, MigrationLockNone:
	default:
		errs = append(errs, invalidMigrationGateConfig("policy.migration_lock", fmt.Sprintf("unsupported value %q", policy.MigrationLock)))
	}

	destructivePolicy := MigrationDestructivePolicy(policy.DestructiveMigrations)
	switch destructivePolicy {
	case "":
		destructivePolicy = DefaultMigrationDestructivePolicy
	case MigrationDestructiveRequireApproval, MigrationDestructiveAllow, MigrationDestructiveBlock:
	default:
		errs = append(errs, invalidMigrationGateConfig("policy.destructive_migrations", fmt.Sprintf("unsupported value %q", policy.DestructiveMigrations)))
	}

	if policy.LockTimeout < 0 {
		errs = append(errs, invalidMigrationGateConfig("policy.lock_timeout", "must be positive"))
	}
	if err := errors.Join(errs...); err != nil {
		return "", "", "", migrations.DeployPolicy{}, err
	}
	return timing, lockPolicy, destructivePolicy, policy, nil
}

func buildMigrationLockDecision(policy MigrationLockPolicy, timeout time.Duration) MigrationLockDecision {
	decision := MigrationLockDecision{
		Policy:         policy,
		Timeout:        timeout,
		AdapterDefault: timeout == 0,
	}
	switch policy {
	case MigrationLockRequired:
		decision.Blocking = true
		decision.TimeoutAction = MigrationTimeoutFailDeploy
	case MigrationLockOptional:
		decision.TimeoutAction = MigrationTimeoutContinue
	case MigrationLockNone:
		decision.TimeoutAction = MigrationTimeoutIgnore
	}
	return decision
}

func buildMigrationDestructiveDecision(
	policy MigrationDestructivePolicy,
	onlinePlan migrations.OnlineMigrationPlan,
) MigrationDestructiveDecision {
	destructive, approved := countDestructiveMigrationSteps(onlinePlan)
	decision := MigrationDestructiveDecision{
		Policy:           policy,
		DestructiveSteps: destructive,
		ApprovedSteps:    approved,
	}
	switch {
	case destructive == 0:
		decision.Reason = "no destructive migration steps planned"
	case policy == MigrationDestructiveBlock:
		decision.Blocking = true
		decision.Reason = "destructive migration steps are blocked by deploy policy"
	case policy == MigrationDestructiveRequireApproval && approved < destructive:
		decision.Blocking = true
		decision.Reason = "destructive migration steps require approval"
	case policy == MigrationDestructiveRequireApproval:
		decision.Reason = "all destructive migration steps are approved"
	default:
		decision.Reason = "destructive migration steps are allowed by deploy policy"
	}
	return decision
}

func buildMigrationDeployHooks(policy migrations.DeployPolicy) ([]MigrationDeployHook, []error) {
	hooks := make([]MigrationDeployHook, 0, 2)
	var errs []error
	if policy.PreHook != "" {
		if hasControlRune(policy.PreHook) {
			errs = append(errs, invalidMigrationGateConfig("policy.pre_hook", "contains control characters"))
		} else {
			hooks = append(hooks, MigrationDeployHook{
				Name:     "pre_migration",
				Phase:    MigrationGatePhasePreMigration,
				Command:  policy.PreHook,
				Blocking: true,
			})
		}
	}
	if policy.PostHook != "" {
		if hasControlRune(policy.PostHook) {
			errs = append(errs, invalidMigrationGateConfig("policy.post_hook", "contains control characters"))
		} else {
			hooks = append(hooks, MigrationDeployHook{
				Name:     "post_migration",
				Phase:    MigrationGatePhasePostMigration,
				Command:  policy.PostHook,
				Blocking: true,
			})
		}
	}
	return hooks, errs
}

func buildOnlineMigrationGates(
	timing MigrationGateTiming,
	plan migrations.OnlineMigrationPlan,
) ([]MigrationGateDecision, []error) {
	var gates []MigrationGateDecision
	var errs []error
	groups := []struct {
		name          string
		phase         migrations.OnlineMigrationPhase
		steps         []migrations.OnlineMigrationStep
		deployPhase   MigrationGatePhase
		blockingPhase bool
	}{
		{"online preflight", migrations.OnlineMigrationPhasePreflight, plan.Preflight, MigrationGatePhaseBeforeDeploy, true},
		{"online expand", migrations.OnlineMigrationPhaseExpand, plan.Expand, MigrationGatePhaseBeforeDeploy, true},
		{"online backfill", migrations.OnlineMigrationPhaseBackfill, plan.Backfill, MigrationGatePhasePostMigration, false},
		{"online contract", migrations.OnlineMigrationPhaseContract, plan.Contract, MigrationGatePhasePostMigration, false},
	}

	for _, group := range groups {
		if len(group.steps) == 0 {
			continue
		}
		for i, step := range group.steps {
			if err := validateOnlineMigrationStepForGate(step, group.phase); err != nil {
				errs = append(errs, invalidMigrationGateConfig(fmt.Sprintf("%s[%d]", group.phase, i), err.Error()))
			}
		}
		gates = append(gates, MigrationGateDecision{
			Name:     group.name,
			Phase:    migrationGatePhaseForTiming(timing, group.deployPhase),
			Blocking: migrationGateBlockingForTiming(timing, group.blockingPhase),
			Count:    len(group.steps),
			Reason:   fmt.Sprintf("%s in %s online migration phase", migrationGatePlural(len(group.steps), "step", "steps"), group.phase),
		})
	}
	return gates, errs
}

func buildTenantMigrationGate(timing MigrationGateTiming, plan migrations.TenantMigrationPlan) []MigrationGateDecision {
	targets := tenantMigrationGateTargetCount(plan)
	if targets == 0 {
		return nil
	}
	batches := plan.BatchCount
	if batches == 0 {
		batches = len(plan.Batches)
	}
	if batches == 0 {
		batches = 1
	}
	return []MigrationGateDecision{{
		Name:     "tenant migrations",
		Phase:    migrationGatePhaseForTiming(timing, MigrationGatePhaseBeforeDeploy),
		Blocking: migrationGateBlockingForTiming(timing, true),
		Count:    targets,
		Reason: fmt.Sprintf(
			"%s across %s",
			migrationGatePlural(targets, "tenant target", "tenant targets"),
			migrationGatePlural(batches, "batch", "batches"),
		),
	}}
}

func buildUpgradeMigrationGate(timing MigrationGateTiming, plan migrations.UpgradePlan) []MigrationGateDecision {
	if len(plan.Recipes) == 0 {
		return nil
	}
	reason := migrationGatePlural(len(plan.Recipes), "upgrade recipe", "upgrade recipes")
	fromVersion := strings.TrimSpace(plan.FromVersion)
	toVersion := strings.TrimSpace(plan.ToVersion)
	if fromVersion != "" || toVersion != "" {
		reason = fmt.Sprintf("%s from %q to %q", reason, fromVersion, toVersion)
	}
	return []MigrationGateDecision{{
		Name:     "upgrade recipes",
		Phase:    migrationGatePhaseForTiming(timing, MigrationGatePhaseBeforeDeploy),
		Blocking: migrationGateBlockingForTiming(timing, true),
		Count:    len(plan.Recipes),
		Reason:   reason,
	}}
}

func buildDestructiveMigrationGate(
	timing MigrationGateTiming,
	decision MigrationDestructiveDecision,
) MigrationGateDecision {
	phase := MigrationGatePhaseBeforeDeploy
	if timing == MigrationGateTimingManual {
		phase = MigrationGatePhaseManual
	}
	return MigrationGateDecision{
		Name:     "destructive migrations",
		Phase:    phase,
		Blocking: true,
		Count:    decision.DestructiveSteps,
		Reason:   decision.Reason,
	}
}

func buildMigrationTimeoutDecisions(
	timing MigrationGateTiming,
	lock MigrationLockDecision,
	contracts []migrations.TenantMigrationContract,
) ([]MigrationTimeoutDecision, []error) {
	timeouts := []MigrationTimeoutDecision{{
		Name:           "migration_lock",
		Scope:          "lock",
		Phase:          migrationGatePhaseForTiming(timing, MigrationGatePhaseBeforeDeploy),
		Timeout:        lock.Timeout,
		AdapterDefault: lock.AdapterDefault,
		Source:         "deploy.lock_timeout",
		Action:         lock.TimeoutAction,
	}}

	var errs []error
	for i, contract := range contracts {
		name := migrationGateContractName(contract, i)
		timeout := contract.Timeout
		if timeout < 0 {
			errs = append(errs, invalidMigrationGateConfig(fmt.Sprintf("contracts[%d].timeout", i), "must be positive"))
			continue
		}
		timeouts = append(timeouts, MigrationTimeoutDecision{
			Name:           name,
			Scope:          "tenant_migration",
			Phase:          migrationGatePhaseForTiming(timing, MigrationGatePhaseBeforeDeploy),
			Timeout:        timeout,
			AdapterDefault: timeout == 0,
			Source:         "tenant_migration.timeout",
			Action:         migrationTimeoutActionForTiming(timing),
		})
	}
	return timeouts, errs
}

func renderMigrationGatePlan(plan MigrationGatePlan) string {
	var b strings.Builder
	b.WriteString("migration_gate:\n")
	writeMigrationGateBool(&b, 2, "dry_run", plan.DryRun)
	writeMigrationGateString(&b, 2, "timing", string(plan.Timing))
	b.WriteString("  lock:\n")
	writeMigrationGateString(&b, 4, "policy", string(plan.Lock.Policy))
	writeMigrationGateTimeout(&b, 4, "timeout", plan.Lock.Timeout, plan.Lock.AdapterDefault)
	writeMigrationGateBool(&b, 4, "blocking", plan.Lock.Blocking)
	writeMigrationGateString(&b, 4, "timeout_action", string(plan.Lock.TimeoutAction))

	b.WriteString("  destructive_migrations:\n")
	writeMigrationGateString(&b, 4, "policy", string(plan.Destructive.Policy))
	b.WriteString("    destructive_steps: ")
	b.WriteString(strconv.Itoa(plan.Destructive.DestructiveSteps))
	b.WriteByte('\n')
	b.WriteString("    approved_steps: ")
	b.WriteString(strconv.Itoa(plan.Destructive.ApprovedSteps))
	b.WriteByte('\n')
	writeMigrationGateBool(&b, 4, "blocking", plan.Destructive.Blocking)
	writeMigrationGateString(&b, 4, "reason", plan.Destructive.Reason)

	b.WriteString("  hooks:\n")
	if len(plan.Hooks) == 0 {
		b.WriteString("    []\n")
	} else {
		for _, hook := range plan.Hooks {
			b.WriteString("    - name: ")
			b.WriteString(quoteYAML(hook.Name))
			b.WriteByte('\n')
			writeMigrationGateString(&b, 6, "phase", string(hook.Phase))
			writeMigrationGateString(&b, 6, "command", hook.Command)
			writeMigrationGateBool(&b, 6, "blocking", hook.Blocking)
		}
	}

	b.WriteString("  gates:\n")
	if len(plan.Gates) == 0 {
		b.WriteString("    []\n")
	} else {
		for _, gate := range plan.Gates {
			b.WriteString("    - name: ")
			b.WriteString(quoteYAML(gate.Name))
			b.WriteByte('\n')
			writeMigrationGateString(&b, 6, "phase", string(gate.Phase))
			writeMigrationGateBool(&b, 6, "blocking", gate.Blocking)
			b.WriteString("      count: ")
			b.WriteString(strconv.Itoa(gate.Count))
			b.WriteByte('\n')
			writeMigrationGateString(&b, 6, "reason", gate.Reason)
		}
	}

	b.WriteString("  timeouts:\n")
	for _, timeout := range plan.Timeouts {
		b.WriteString("    - name: ")
		b.WriteString(quoteYAML(timeout.Name))
		b.WriteByte('\n')
		writeMigrationGateString(&b, 6, "scope", timeout.Scope)
		writeMigrationGateString(&b, 6, "phase", string(timeout.Phase))
		writeMigrationGateTimeout(&b, 6, "timeout", timeout.Timeout, timeout.AdapterDefault)
		writeMigrationGateString(&b, 6, "source", timeout.Source)
		writeMigrationGateString(&b, 6, "action", string(timeout.Action))
	}

	b.WriteString("  summary:\n")
	b.WriteString("    blocking_gates: ")
	b.WriteString(strconv.Itoa(len(plan.BlockingGates())))
	b.WriteByte('\n')
	b.WriteString("    nonblocking_gates: ")
	b.WriteString(strconv.Itoa(len(plan.NonBlockingGates())))
	b.WriteByte('\n')
	b.WriteString("    hooks: ")
	b.WriteString(strconv.Itoa(len(plan.Hooks)))
	b.WriteByte('\n')
	b.WriteString("    timeouts: ")
	b.WriteString(strconv.Itoa(len(plan.Timeouts)))
	b.WriteByte('\n')
	return b.String()
}

func validateOnlineMigrationStepForGate(step migrations.OnlineMigrationStep, phase migrations.OnlineMigrationPhase) error {
	if strings.TrimSpace(step.Name) == "" {
		return errors.New("step name is required")
	}
	if step.Phase != "" && step.Phase != phase {
		return fmt.Errorf("step phase %q does not match %q", step.Phase, phase)
	}
	if step.TransactionMode != "" && !knownOnlineMigrationTransactionMode(step.TransactionMode) {
		return fmt.Errorf("unsupported transaction mode %q", step.TransactionMode)
	}
	return nil
}

func knownOnlineMigrationTransactionMode(mode migrations.OnlineMigrationTransactionMode) bool {
	switch mode {
	case migrations.OnlineMigrationTransactionTransactional,
		migrations.OnlineMigrationTransactionNonTransactional:
		return true
	default:
		return false
	}
}

func countDestructiveMigrationSteps(plan migrations.OnlineMigrationPlan) (int, int) {
	destructive := 0
	approved := 0
	for _, step := range plan.Steps() {
		if !step.Destructive {
			continue
		}
		destructive++
		if step.DestructiveApproved {
			approved++
		}
	}
	return destructive, approved
}

func tenantMigrationGateTargetCount(plan migrations.TenantMigrationPlan) int {
	if plan.TargetCount > 0 {
		return plan.TargetCount
	}
	targets := 0
	for _, batch := range plan.Batches {
		if batch.Count > 0 {
			targets += batch.Count
			continue
		}
		targets += len(batch.Targets)
	}
	return targets
}

func migrationGateContractName(contract migrations.TenantMigrationContract, index int) string {
	feature := strings.TrimSpace(contract.Feature)
	name := strings.TrimSpace(contract.Name)
	switch {
	case feature != "" && name != "":
		return feature + "." + name
	case name != "":
		return name
	case strings.TrimSpace(contract.HandlerPath) != "":
		return strings.TrimSpace(contract.HandlerPath)
	default:
		return fmt.Sprintf("tenant_migration-%d", index+1)
	}
}

func migrationGatePhaseForTiming(timing MigrationGateTiming, phase MigrationGatePhase) MigrationGatePhase {
	if timing == MigrationGateTimingManual {
		return MigrationGatePhaseManual
	}
	return phase
}

func migrationGateBlockingForTiming(timing MigrationGateTiming, blocking bool) bool {
	return timing == MigrationGateTimingBeforeDeploy && blocking
}

func migrationTimeoutActionForTiming(timing MigrationGateTiming) MigrationTimeoutAction {
	if timing == MigrationGateTimingManual || timing == MigrationGateTimingDisabled {
		return MigrationTimeoutContinue
	}
	return MigrationTimeoutFailDeploy
}

func filterMigrationGates(gates []MigrationGateDecision, blocking bool) []MigrationGateDecision {
	filtered := make([]MigrationGateDecision, 0, len(gates))
	for _, gate := range gates {
		if gate.Blocking == blocking {
			filtered = append(filtered, gate)
		}
	}
	return filtered
}

func migrationGatePlural(count int, singular, plural string) string {
	word := plural
	if count == 1 {
		word = singular
	}
	return strconv.Itoa(count) + " " + word
}

func writeMigrationGateString(b *strings.Builder, indent int, key, value string) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(": ")
	b.WriteString(quoteYAML(value))
	b.WriteByte('\n')
}

func writeMigrationGateBool(b *strings.Builder, indent int, key string, value bool) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(": ")
	b.WriteString(strconv.FormatBool(value))
	b.WriteByte('\n')
}

func writeMigrationGateTimeout(
	b *strings.Builder,
	indent int,
	key string,
	timeout time.Duration,
	adapterDefault bool,
) {
	value := "adapter_default"
	if !adapterDefault {
		value = timeout.String()
	}
	writeMigrationGateString(b, indent, key, value)
}

func invalidMigrationGateConfig(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidMigrationGateConfig, field, detail)
}
