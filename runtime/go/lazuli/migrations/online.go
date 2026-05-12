package migrations

import (
	"errors"
	"fmt"
	"strings"
)

var (
	// ErrOnlineMigrationStepNameRequired is returned when an online migration
	// step has no stable name for plan output and approval records.
	ErrOnlineMigrationStepNameRequired = errors.New("migrations: online migration step name required")
	// ErrInvalidOnlineMigrationPhase is returned when a step declares an
	// unknown online migration phase.
	ErrInvalidOnlineMigrationPhase = errors.New("migrations: invalid online migration phase")
	// ErrInvalidOnlineMigrationTransactionMode is returned when a step omits or
	// declares an unknown transaction safety mode.
	ErrInvalidOnlineMigrationTransactionMode = errors.New("migrations: invalid online migration transaction mode")
	// ErrDestructiveMigrationApprovalRequired is returned when a destructive
	// online migration step is planned without explicit approval.
	ErrDestructiveMigrationApprovalRequired = errors.New("migrations: destructive migration approval required")
)

// OnlineMigrationPhase names the phase where an online migration step should
// run. Plans are always emitted in preflight, expand, backfill, contract order.
type OnlineMigrationPhase string

const (
	// OnlineMigrationPhasePreflight runs checks before schema changes.
	OnlineMigrationPhasePreflight OnlineMigrationPhase = "preflight"
	// OnlineMigrationPhaseExpand adds forward-compatible schema or indexes.
	OnlineMigrationPhaseExpand OnlineMigrationPhase = "expand"
	// OnlineMigrationPhaseBackfill moves or repairs existing data.
	OnlineMigrationPhaseBackfill OnlineMigrationPhase = "backfill"
	// OnlineMigrationPhaseContract removes old schema after callers stop using it.
	OnlineMigrationPhaseContract OnlineMigrationPhase = "contract"
)

// OnlineMigrationTransactionMode declares whether a step expects to run inside
// a transaction. Some providers require non-transactional DDL for online index
// builds or concurrent validation.
type OnlineMigrationTransactionMode string

const (
	// OnlineMigrationTransactionTransactional marks a step as safe to run inside
	// the adapter's migration transaction.
	OnlineMigrationTransactionTransactional OnlineMigrationTransactionMode = "transactional"
	// OnlineMigrationTransactionNonTransactional marks a step as requiring
	// execution outside the adapter's migration transaction.
	OnlineMigrationTransactionNonTransactional OnlineMigrationTransactionMode = "non_transactional"
)

// OnlineMigrationStep is an adapter-neutral descriptor for one online migration
// operation. The package only plans and validates descriptors; concrete
// migration adapters own execution.
type OnlineMigrationStep struct {
	// Name is a stable identifier for plan output and approval records.
	Name string
	// Phase declares the online migration phase for this step.
	Phase OnlineMigrationPhase
	// TransactionMode declares whether the step runs transactionally.
	TransactionMode OnlineMigrationTransactionMode
	// Destructive marks steps that drop or irreversibly mutate schema or data.
	Destructive bool
	// DestructiveApproved must be true before a destructive step can be planned.
	DestructiveApproved bool
}

// OnlineMigrationPlan groups steps by online migration phase.
type OnlineMigrationPlan struct {
	Preflight []OnlineMigrationStep
	Expand    []OnlineMigrationStep
	Backfill  []OnlineMigrationStep
	Contract  []OnlineMigrationStep
}

// PlanOnlineMigration validates steps and groups them into online migration
// phases. Step order is preserved within each phase.
func PlanOnlineMigration(steps []OnlineMigrationStep) (OnlineMigrationPlan, error) {
	var plan OnlineMigrationPlan
	for i, step := range steps {
		normalized, err := normalizeOnlineMigrationStep(step)
		if err != nil {
			return plan, fmt.Errorf("migrations: online migration step %d: %w", i, err)
		}

		switch normalized.Phase {
		case OnlineMigrationPhasePreflight:
			plan.Preflight = append(plan.Preflight, normalized)
		case OnlineMigrationPhaseExpand:
			plan.Expand = append(plan.Expand, normalized)
		case OnlineMigrationPhaseBackfill:
			plan.Backfill = append(plan.Backfill, normalized)
		case OnlineMigrationPhaseContract:
			plan.Contract = append(plan.Contract, normalized)
		}
	}
	return plan, nil
}

// Steps returns all planned steps in online migration execution order.
func (p OnlineMigrationPlan) Steps() []OnlineMigrationStep {
	total := len(p.Preflight) + len(p.Expand) + len(p.Backfill) + len(p.Contract)
	steps := make([]OnlineMigrationStep, 0, total)
	steps = append(steps, p.Preflight...)
	steps = append(steps, p.Expand...)
	steps = append(steps, p.Backfill...)
	steps = append(steps, p.Contract...)
	return steps
}

func normalizeOnlineMigrationStep(step OnlineMigrationStep) (OnlineMigrationStep, error) {
	step.Name = strings.TrimSpace(step.Name)
	step.Phase = OnlineMigrationPhase(strings.TrimSpace(string(step.Phase)))
	step.TransactionMode = OnlineMigrationTransactionMode(strings.TrimSpace(string(step.TransactionMode)))

	if step.Name == "" {
		return step, ErrOnlineMigrationStepNameRequired
	}
	if !validOnlineMigrationPhase(step.Phase) {
		return step, fmt.Errorf("%w %q", ErrInvalidOnlineMigrationPhase, step.Phase)
	}
	if !validOnlineMigrationTransactionMode(step.TransactionMode) {
		return step, fmt.Errorf("%w %q", ErrInvalidOnlineMigrationTransactionMode, step.TransactionMode)
	}
	if step.Destructive && !step.DestructiveApproved {
		return step, fmt.Errorf("%w for %q", ErrDestructiveMigrationApprovalRequired, step.Name)
	}
	return step, nil
}

func validOnlineMigrationPhase(phase OnlineMigrationPhase) bool {
	switch phase {
	case OnlineMigrationPhasePreflight,
		OnlineMigrationPhaseExpand,
		OnlineMigrationPhaseBackfill,
		OnlineMigrationPhaseContract:
		return true
	default:
		return false
	}
}

func validOnlineMigrationTransactionMode(mode OnlineMigrationTransactionMode) bool {
	switch mode {
	case OnlineMigrationTransactionTransactional,
		OnlineMigrationTransactionNonTransactional:
		return true
	default:
		return false
	}
}
