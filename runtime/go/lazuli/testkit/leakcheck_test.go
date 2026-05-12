package testkit

import (
	"strings"
	"testing"
	"time"
)

const leakCheckBlockedFunction = "leakCheckBlockedGoroutine"

func TestCheckGoroutineLeaksPassesAfterCleanupStopsGoroutine(t *testing.T) {
	CheckGoroutineLeaks(t,
		WithLeakCheckTimeout(time.Second),
		WithLeakCheckPollInterval(time.Millisecond),
	)

	release, done := startLeakCheckBlockedGoroutine(t)
	t.Cleanup(func() {
		close(release)
		waitForLeakCheckGoroutineDone(t, done)
	})
}

func TestLeakCheckDetectsNewGoroutine(t *testing.T) {
	baseline := captureGoroutines()
	release, done := startLeakCheckBlockedGoroutine(t)
	defer func() {
		close(release)
		waitForLeakCheckGoroutineDone(t, done)
	}()

	leaks := leakedGoroutines(baseline, captureGoroutines(), applyLeakCheckOptions(nil))
	if !hasLeakCheckFunction(leaks, leakCheckBlockedFunction) {
		t.Fatalf("leaks did not contain %s; leaks:\n%s", leakCheckBlockedFunction, formatLeakedGoroutines(leaks))
	}
}

func TestLeakCheckIgnoresConfiguredFunction(t *testing.T) {
	baseline := captureGoroutines()
	release, done := startLeakCheckBlockedGoroutine(t)
	defer func() {
		close(release)
		waitForLeakCheckGoroutineDone(t, done)
	}()

	options := applyLeakCheckOptions([]LeakCheckOption{
		WithLeakCheckIgnoredFunctions(leakCheckBlockedFunction),
	})
	leaks := leakedGoroutines(baseline, captureGoroutines(), options)
	if hasLeakCheckFunction(leaks, leakCheckBlockedFunction) {
		t.Fatalf("ignored function %s was reported; leaks:\n%s", leakCheckBlockedFunction, formatLeakedGoroutines(leaks))
	}
}

func TestLeakCheckWaitsForGoroutineToExit(t *testing.T) {
	baseline := captureGoroutines()
	release, done := startLeakCheckBlockedGoroutine(t)

	time.AfterFunc(25*time.Millisecond, func() {
		close(release)
	})

	options := applyLeakCheckOptions([]LeakCheckOption{
		WithLeakCheckTimeout(time.Second),
		WithLeakCheckPollInterval(time.Millisecond),
	})
	if leaks := waitForGoroutineLeaks(baseline, options); len(leaks) != 0 {
		t.Fatalf("waitForGoroutineLeaks() leaks = %d, want 0:\n%s", len(leaks), formatLeakedGoroutines(leaks))
	}
	waitForLeakCheckGoroutineDone(t, done)
}

func TestLeakCheckIgnoresKnownRuntimeStacks(t *testing.T) {
	baseline := goroutineSnapshot{
		1: {id: 1, state: "running", functions: []string{"example.Test"}, stack: "goroutine 1 [running]:\nexample.Test()\n"},
	}
	current := goroutineSnapshot{
		1: baseline[1],
		2: {
			id:        2,
			state:     "force gc (idle)",
			functions: []string{"runtime.gopark", "runtime.forcegchelper"},
			stack:     "goroutine 2 [force gc (idle)]:\nruntime.gopark()\nruntime.forcegchelper()\n",
		},
	}

	if leaks := leakedGoroutines(baseline, current, applyLeakCheckOptions(nil)); len(leaks) != 0 {
		t.Fatalf("leaks = %d, want 0:\n%s", len(leaks), formatLeakedGoroutines(leaks))
	}
}

func TestLeakCheckParsesGoroutineDump(t *testing.T) {
	dump := strings.Join([]string{
		"goroutine 12 [chan receive]:",
		"lazuli.dev/runtime/lazuli/testkit.leakCheckBlockedGoroutine(0xc00001a150, 0xc00001a1c0)",
		"\tC:/repo/runtime/go/lazuli/testkit/leakcheck_test.go:111 +0x25",
		"created by testing.(*T).Run in goroutine 1",
		"\tC:/go/src/testing/testing.go:1999 +0x465",
	}, "\n")

	stack, ok := parseGoroutineStack(dump)
	if !ok {
		t.Fatal("parseGoroutineStack() did not parse stack")
	}
	if stack.id != 12 || stack.state != "chan receive" {
		t.Fatalf("parsed header = id %d state %q, want id 12 state chan receive", stack.id, stack.state)
	}
	assertLeakCheckFunction(t, stack, "lazuli.dev/runtime/lazuli/testkit.leakCheckBlockedGoroutine")
	assertLeakCheckFunction(t, stack, "testing.(*T).Run")
}

func startLeakCheckBlockedGoroutine(t *testing.T) (chan struct{}, <-chan struct{}) {
	t.Helper()

	release := make(chan struct{})
	done := make(chan struct{})
	go leakCheckBlockedGoroutine(release, done)
	waitForLeakCheckFunction(t, leakCheckBlockedFunction)
	return release, done
}

func leakCheckBlockedGoroutine(release <-chan struct{}, done chan<- struct{}) {
	defer close(done)
	<-release
}

func waitForLeakCheckFunction(t *testing.T, function string) {
	t.Helper()

	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		for _, stack := range captureGoroutines() {
			if stackHasFunction(stack, function) {
				return
			}
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatalf("timed out waiting for goroutine function %s", function)
}

func waitForLeakCheckGoroutineDone(t *testing.T, done <-chan struct{}) {
	t.Helper()

	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for leak check goroutine to stop")
	}
}

func hasLeakCheckFunction(stacks []goroutineStack, function string) bool {
	for _, stack := range stacks {
		if stackHasFunction(stack, function) {
			return true
		}
	}
	return false
}

func stackHasFunction(stack goroutineStack, function string) bool {
	for _, fn := range stack.functions {
		if functionNameMatches(fn, function) {
			return true
		}
	}
	return false
}

func assertLeakCheckFunction(t *testing.T, stack goroutineStack, function string) {
	t.Helper()

	if !stackHasFunction(stack, function) {
		t.Fatalf("stack functions = %v, want %s", stack.functions, function)
	}
}
