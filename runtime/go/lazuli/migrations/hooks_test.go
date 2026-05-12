package migrations

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestHookRunnerRunsPhaseHooksInOrder(t *testing.T) {
	t.Parallel()

	var calls []string
	hooks := []MigrationHook{
		testMigrationHook(t, &calls, "post-skip", HookPhasePostMigration, 0, nil),
		testMigrationHook(t, &calls, "pre-second", HookPhasePreMigration, 20, nil),
		testMigrationHook(t, &calls, "pre-first", HookPhasePreMigration, 10, nil),
		testMigrationHook(t, &calls, "pre-same-order", HookPhasePreMigration, 10, nil),
	}

	summary, err := NewHookRunner(hooks, HookFailureStop).Run(context.Background(), HookPhasePreMigration)
	if err != nil {
		t.Fatalf("Run returned error: %v", err)
	}

	if got, want := strings.Join(calls, ","), "pre-first,pre-same-order,pre-second"; got != want {
		t.Fatalf("calls = %q, want %q", got, want)
	}
	if got, want := hookExecutionNames(summary.Executed), []string{"pre-first", "pre-same-order", "pre-second"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("executed hooks = %v, want %v", got, want)
	}
	if summary.Phase != HookPhasePreMigration {
		t.Fatalf("summary phase = %q, want pre migration", summary.Phase)
	}
	if summary.FailurePolicy != HookFailureStop {
		t.Fatalf("failure policy = %q, want stop", summary.FailurePolicy)
	}
}

func TestHookRunnerStopsOnFirstFailureByDefault(t *testing.T) {
	t.Parallel()

	sentinel := errors.New("pre hook failed")
	var calls []string
	hooks := []MigrationHook{
		testMigrationHook(t, &calls, "ok", HookPhasePreMigration, 1, nil),
		testMigrationHook(t, &calls, "fail", HookPhasePreMigration, 2, sentinel),
		testMigrationHook(t, &calls, "skip", HookPhasePreMigration, 3, nil),
	}

	summary, err := HookRunner{Hooks: hooks}.Run(context.Background(), HookPhasePreMigration)
	if !errors.Is(err, sentinel) {
		t.Fatalf("Run error = %v, want sentinel", err)
	}
	if got, want := strings.Join(calls, ","), "ok,fail"; got != want {
		t.Fatalf("calls = %q, want %q", got, want)
	}
	if got, want := hookExecutionNames(summary.Executed), []string{"ok", "fail"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("executed hooks = %v, want %v", got, want)
	}
	if len(summary.Failures) != 1 || !errors.Is(summary.Failures[0].Err, sentinel) {
		t.Fatalf("failures = %+v, want sentinel failure", summary.Failures)
	}
}

func TestHookRunnerContinuesOnFailurePolicy(t *testing.T) {
	t.Parallel()

	firstErr := errors.New("first hook failed")
	secondErr := errors.New("second hook failed")
	var calls []string
	hooks := []MigrationHook{
		testMigrationHook(t, &calls, "fail-first", HookPhasePostMigration, 1, firstErr),
		testMigrationHook(t, &calls, "ok", HookPhasePostMigration, 2, nil),
		testMigrationHook(t, &calls, "fail-second", HookPhasePostMigration, 3, secondErr),
	}

	summary, err := RunMigrationHooks(context.Background(), HookPhasePostMigration, hooks, HookFailureContinue)
	if !errors.Is(err, firstErr) {
		t.Fatalf("Run error = %v, want first failure", err)
	}
	if !errors.Is(err, secondErr) {
		t.Fatalf("Run error = %v, want second failure", err)
	}
	if got, want := strings.Join(calls, ","), "fail-first,ok,fail-second"; got != want {
		t.Fatalf("calls = %q, want %q", got, want)
	}
	if len(summary.Failures) != 2 {
		t.Fatalf("failure count = %d, want 2", len(summary.Failures))
	}
	if summary.FailurePolicy != HookFailureContinue {
		t.Fatalf("failure policy = %q, want continue", summary.FailurePolicy)
	}
}

func TestHookRunnerPropagatesContextCancellation(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	var calls []string
	hooks := []MigrationHook{
		{
			Name:  "cancel",
			Phase: HookPhasePreMigration,
			Order: 1,
			Run: func(context.Context, HookContext) error {
				calls = append(calls, "cancel")
				cancel()
				return nil
			},
		},
		testMigrationHook(t, &calls, "skip", HookPhasePreMigration, 2, nil),
	}

	summary, err := NewHookRunner(hooks, HookFailureContinue).Run(ctx, HookPhasePreMigration)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Run error = %v, want context.Canceled", err)
	}
	if got, want := strings.Join(calls, ","), "cancel"; got != want {
		t.Fatalf("calls = %q, want %q", got, want)
	}
	if got, want := hookExecutionNames(summary.Executed), []string{"cancel"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("executed hooks = %v, want %v", got, want)
	}
	if len(summary.Failures) != 0 {
		t.Fatalf("failures = %+v, want none for context cancellation", summary.Failures)
	}
}

func TestHookRunnerSkipsHooksWhenContextAlreadyCanceled(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	called := false
	hooks := []MigrationHook{
		{
			Name:  "should-not-run",
			Phase: HookPhasePreMigration,
			Order: 1,
			Run: func(context.Context, HookContext) error {
				called = true
				return nil
			},
		},
	}

	summary, err := NewHookRunner(hooks, HookFailureContinue).Run(ctx, HookPhasePreMigration)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Run error = %v, want context.Canceled", err)
	}
	if called {
		t.Fatal("hook was called after context cancellation")
	}
	if len(summary.Executed) != 0 {
		t.Fatalf("executed hooks = %+v, want none", summary.Executed)
	}
}

func TestHookRunnerValidatesPhasePolicyAndNilHook(t *testing.T) {
	t.Parallel()

	if _, err := NewHookRunner(nil, HookFailureStop).Run(context.Background(), HookPhase("during")); !errors.Is(err, errInvalidMigrationHookPhase) {
		t.Fatalf("invalid phase error = %v, want errInvalidMigrationHookPhase", err)
	}
	if _, err := NewHookRunner(nil, HookFailurePolicy("ignore")).Run(context.Background(), HookPhasePreMigration); !errors.Is(err, errInvalidMigrationHookFailurePolicy) {
		t.Fatalf("invalid policy error = %v, want errInvalidMigrationHookFailurePolicy", err)
	}

	summary, err := NewHookRunner([]MigrationHook{
		{Name: "missing", Phase: HookPhasePreMigration},
	}, HookFailureStop).Run(context.Background(), HookPhasePreMigration)
	if !errors.Is(err, errNilMigrationHook) {
		t.Fatalf("nil hook error = %v, want errNilMigrationHook", err)
	}
	if len(summary.Executed) != 0 {
		t.Fatalf("executed hooks = %+v, want none for nil hook", summary.Executed)
	}
	if len(summary.Failures) != 1 || !errors.Is(summary.Failures[0].Err, errNilMigrationHook) {
		t.Fatalf("failures = %+v, want nil hook failure", summary.Failures)
	}
}

func testMigrationHook(t *testing.T, calls *[]string, name string, phase HookPhase, order int, err error) MigrationHook {
	t.Helper()

	return MigrationHook{
		Name:  name,
		Phase: phase,
		Order: order,
		Run: func(_ context.Context, got HookContext) error {
			if got.Name != name {
				t.Fatalf("hook context name = %q, want %q", got.Name, name)
			}
			if got.Phase != phase {
				t.Fatalf("hook context phase = %q, want %q", got.Phase, phase)
			}
			if got.Order != order {
				t.Fatalf("hook context order = %d, want %d", got.Order, order)
			}
			*calls = append(*calls, name)
			return err
		},
	}
}

func hookExecutionNames(executed []HookExecution) []string {
	names := make([]string, 0, len(executed))
	for _, execution := range executed {
		names = append(names, execution.Name)
	}
	return names
}
