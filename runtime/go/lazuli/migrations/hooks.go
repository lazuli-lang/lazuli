package migrations

import (
	"context"
	"errors"
	"fmt"
	"sort"
)

var (
	errInvalidMigrationHookPhase         = errors.New("migrations: hook phase invalid")
	errInvalidMigrationHookFailurePolicy = errors.New("migrations: hook failure policy invalid")
	errNilMigrationHook                  = errors.New("migrations: hook is required")
)

// HookPhase names the lifecycle phase a migration hook runs in.
type HookPhase string

const (
	// HookPhasePreMigration runs before the deploy adapter applies migrations.
	HookPhasePreMigration HookPhase = "pre_migration"
	// HookPhasePostMigration runs after the deploy adapter applies migrations.
	HookPhasePostMigration HookPhase = "post_migration"
)

// HookFailurePolicy controls whether hook failures stop the current phase or
// are collected while later hooks continue.
type HookFailurePolicy string

const (
	// HookFailureStop stops the phase on the first hook failure. It is the
	// default when HookRunner.FailurePolicy is empty.
	HookFailureStop HookFailurePolicy = "stop"
	// HookFailureContinue runs remaining hooks and joins all hook failures.
	HookFailureContinue HookFailurePolicy = "continue"
)

// HookContext is passed to a migration hook invocation.
type HookContext struct {
	// Phase is the lifecycle phase being run.
	Phase HookPhase
	// Name identifies the hook for logs, summaries, and errors.
	Name string
	// Order is the hook's declared order within its phase.
	Order int
}

// HookFunc is the codegen/adapter-neutral migration hook callback shape.
type HookFunc func(ctx context.Context, hook HookContext) error

// MigrationHook registers a callback for one migration hook phase.
type MigrationHook struct {
	// Name identifies the hook for logs, summaries, and errors. Generated code
	// should usually use the lowered hook path.
	Name string
	// Phase declares when the hook should run.
	Phase HookPhase
	// Order controls execution within a phase. Lower values run first; equal
	// values keep registration order.
	Order int
	// Run is invoked when Phase matches the runner phase.
	Run HookFunc
}

// HookExecution records one hook callback invocation.
type HookExecution struct {
	Name  string
	Phase HookPhase
	Order int
}

// HookFailure records one hook failure with contextual wrapping.
type HookFailure struct {
	Name  string
	Phase HookPhase
	Order int
	Err   error
}

// HookSummary describes the hooks attempted for one phase.
type HookSummary struct {
	Phase         HookPhase
	Executed      []HookExecution
	Failures      []HookFailure
	FailurePolicy HookFailurePolicy
}

// HookRunner runs registered migration hooks by phase.
type HookRunner struct {
	// Hooks contains all registered migration hooks. Run filters by phase.
	Hooks []MigrationHook
	// FailurePolicy controls non-context hook errors. Empty means
	// HookFailureStop.
	FailurePolicy HookFailurePolicy
}

// NewHookRunner returns a migration hook runner.
func NewHookRunner(hooks []MigrationHook, failurePolicy HookFailurePolicy) HookRunner {
	return HookRunner{Hooks: hooks, FailurePolicy: failurePolicy}
}

// RunMigrationHooks runs hooks for phase using failurePolicy.
func RunMigrationHooks(
	ctx context.Context,
	phase HookPhase,
	hooks []MigrationHook,
	failurePolicy HookFailurePolicy,
) (HookSummary, error) {
	return NewHookRunner(hooks, failurePolicy).Run(ctx, phase)
}

// Run executes hooks for phase in ascending Order, preserving registration
// order for equal values. Context cancellation and deadlines always stop the
// phase and are returned unchanged, independent of FailurePolicy.
func (r HookRunner) Run(ctx context.Context, phase HookPhase) (HookSummary, error) {
	if ctx == nil {
		ctx = context.Background()
	}

	summary := HookSummary{Phase: phase}
	if !isKnownHookPhase(phase) {
		return summary, errInvalidMigrationHookPhase
	}

	policy, err := normalizeHookFailurePolicy(r.FailurePolicy)
	if err != nil {
		return summary, err
	}
	summary.FailurePolicy = policy

	if err := ctx.Err(); err != nil {
		return summary, err
	}

	hooks := hooksForPhase(r.Hooks, phase)
	for _, hook := range hooks {
		if err := ctx.Err(); err != nil {
			return summary, err
		}

		if hook.Run == nil {
			failure := migrationHookFailure(hook, errNilMigrationHook)
			summary.Failures = append(summary.Failures, failure)
			if policy == HookFailureStop {
				return summary, failure.Err
			}
			continue
		}

		run := HookExecution{Name: hook.Name, Phase: hook.Phase, Order: hook.Order}
		summary.Executed = append(summary.Executed, run)
		err := hook.Run(ctx, HookContext{Name: hook.Name, Phase: hook.Phase, Order: hook.Order})
		if err != nil {
			if isHookContextError(err) {
				return summary, err
			}
			failure := migrationHookFailure(hook, err)
			summary.Failures = append(summary.Failures, failure)
			if policy == HookFailureStop {
				return summary, failure.Err
			}
		}

		if err := ctx.Err(); err != nil {
			return summary, err
		}
	}

	return summary, migrationHookFailuresError(summary.Failures)
}

func hooksForPhase(hooks []MigrationHook, phase HookPhase) []MigrationHook {
	selected := make([]MigrationHook, 0, len(hooks))
	for _, hook := range hooks {
		if hook.Phase == phase {
			selected = append(selected, hook)
		}
	}
	sort.SliceStable(selected, func(i, j int) bool {
		return selected[i].Order < selected[j].Order
	})
	return selected
}

func migrationHookFailure(hook MigrationHook, err error) HookFailure {
	return HookFailure{
		Name:  hook.Name,
		Phase: hook.Phase,
		Order: hook.Order,
		Err:   fmt.Errorf("migrations: %s hook %q failed: %w", hook.Phase, displayHookName(hook.Name), err),
	}
}

func migrationHookFailuresError(failures []HookFailure) error {
	if len(failures) == 0 {
		return nil
	}

	errs := make([]error, 0, len(failures))
	for _, failure := range failures {
		errs = append(errs, failure.Err)
	}
	return errors.Join(errs...)
}

func normalizeHookFailurePolicy(policy HookFailurePolicy) (HookFailurePolicy, error) {
	switch policy {
	case "", HookFailureStop:
		return HookFailureStop, nil
	case HookFailureContinue:
		return HookFailureContinue, nil
	default:
		return "", errInvalidMigrationHookFailurePolicy
	}
}

func isKnownHookPhase(phase HookPhase) bool {
	switch phase {
	case HookPhasePreMigration, HookPhasePostMigration:
		return true
	default:
		return false
	}
}

func displayHookName(name string) string {
	if name == "" {
		return "<unnamed>"
	}
	return name
}

func isHookContextError(err error) bool {
	return errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded)
}
