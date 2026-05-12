package testkit

import (
	"runtime"
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"
)

const (
	defaultLeakCheckTimeout      = 500 * time.Millisecond
	defaultLeakCheckPollInterval = 10 * time.Millisecond
)

var defaultLeakCheckIgnoredFunctions = []string{
	"runtime.forcegchelper",
	"runtime.bgsweep",
	"runtime.bgscavenge",
	"runtime.runfinq",
	"runtime.gcBgMarkWorker",
	"runtime.unique_runtime_registerUniqueMapCleanup",
	"testing.(*M).startAlarm.func1",
	"testing.(*testContext).waitParallel",
}

// LeakCheckOptions configures goroutine leak checks.
type LeakCheckOptions struct {
	// Timeout is how long cleanup waits for goroutines to exit before failing.
	// The default is 500ms. A non-positive value performs one snapshot.
	Timeout time.Duration

	// PollInterval is the delay between snapshots while waiting for goroutines
	// to exit. The default is 10ms. Non-positive values use the default.
	PollInterval time.Duration

	// IgnoredFunctions are goroutine stack function names that should not count
	// as leaks. Entries may be full function names or suffixes.
	IgnoredFunctions []string
}

// LeakCheckOption configures CheckGoroutineLeaks.
type LeakCheckOption func(*LeakCheckOptions)

// WithLeakCheckTimeout sets how long cleanup waits for goroutines to exit.
// A non-positive timeout performs one snapshot without polling.
func WithLeakCheckTimeout(timeout time.Duration) LeakCheckOption {
	return func(options *LeakCheckOptions) {
		options.Timeout = timeout
	}
}

// WithLeakCheckPollInterval sets the delay between cleanup snapshots.
func WithLeakCheckPollInterval(interval time.Duration) LeakCheckOption {
	return func(options *LeakCheckOptions) {
		options.PollInterval = interval
	}
}

// WithLeakCheckIgnoredFunctions ignores goroutines whose stack contains any of
// the provided function names. Names may be full names or suffixes.
func WithLeakCheckIgnoredFunctions(functions ...string) LeakCheckOption {
	return func(options *LeakCheckOptions) {
		options.IgnoredFunctions = append(options.IgnoredFunctions, functions...)
	}
}

// CheckGoroutineLeaks registers a cleanup that fails the test if it leaves new
// goroutines running.
//
// Call it near the start of a test, before starting test-owned goroutines. The
// checker snapshots the current goroutines, ignores known testing/runtime
// background goroutines, then waits during cleanup for test-owned goroutines to
// stop before reporting the remaining stacks.
func CheckGoroutineLeaks(t testing.TB, opts ...LeakCheckOption) {
	t.Helper()

	options := applyLeakCheckOptions(opts)
	baseline := captureGoroutines()

	t.Cleanup(func() {
		t.Helper()

		leaks := waitForGoroutineLeaks(baseline, options)
		if len(leaks) == 0 {
			return
		}
		t.Fatalf("testkit: goroutine leak detected after %s:\n%s",
			options.Timeout, formatLeakedGoroutines(leaks))
	})
}

func applyLeakCheckOptions(opts []LeakCheckOption) LeakCheckOptions {
	options := LeakCheckOptions{
		Timeout:          defaultLeakCheckTimeout,
		PollInterval:     defaultLeakCheckPollInterval,
		IgnoredFunctions: append([]string(nil), defaultLeakCheckIgnoredFunctions...),
	}
	for _, opt := range opts {
		if opt != nil {
			opt(&options)
		}
	}
	if options.PollInterval <= 0 {
		options.PollInterval = defaultLeakCheckPollInterval
	}
	options.IgnoredFunctions = normalizeIgnoredFunctions(options.IgnoredFunctions)
	return options
}

func normalizeIgnoredFunctions(functions []string) []string {
	seen := make(map[string]struct{}, len(functions))
	normalized := make([]string, 0, len(functions))
	for _, fn := range functions {
		fn = strings.TrimSpace(fn)
		if fn == "" {
			continue
		}
		if _, ok := seen[fn]; ok {
			continue
		}
		seen[fn] = struct{}{}
		normalized = append(normalized, fn)
	}
	return normalized
}

type goroutineSnapshot map[int64]goroutineStack

type goroutineStack struct {
	id        int64
	state     string
	functions []string
	stack     string
}

func waitForGoroutineLeaks(baseline goroutineSnapshot, options LeakCheckOptions) []goroutineStack {
	if options.Timeout <= 0 {
		return leakedGoroutines(baseline, captureGoroutines(), options)
	}

	deadline := time.Now().Add(options.Timeout)
	var (
		lastKey     string
		stableCount int
		stableLeaks []goroutineStack
	)
	for {
		leaks := leakedGoroutines(baseline, captureGoroutines(), options)
		if len(leaks) == 0 {
			return nil
		}
		key := goroutineLeakKey(leaks)
		if key == lastKey {
			stableCount++
		} else {
			lastKey = key
			stableCount = 1
		}
		if stableCount >= 2 {
			stableLeaks = leaks
		}
		if !time.Now().Before(deadline) {
			if len(stableLeaks) != 0 {
				return stableLeaks
			}
			return leaks
		}

		sleep := options.PollInterval
		if remaining := time.Until(deadline); remaining < sleep {
			sleep = remaining
		}
		if sleep > 0 {
			time.Sleep(sleep)
		}
	}
}

func goroutineLeakKey(leaks []goroutineStack) string {
	var key strings.Builder
	for _, leak := range leaks {
		key.WriteString(strconv.FormatInt(leak.id, 10))
		key.WriteByte('\n')
		key.WriteString(leak.state)
		key.WriteByte('\n')
		for _, fn := range leak.functions {
			key.WriteString(fn)
			key.WriteByte('\n')
		}
	}
	return key.String()
}

func captureGoroutines() goroutineSnapshot {
	buf := make([]byte, 64*1024)
	for {
		n := runtime.Stack(buf, true)
		if n < len(buf) {
			return parseGoroutineDump(string(buf[:n]))
		}
		buf = make([]byte, len(buf)*2)
	}
}

func parseGoroutineDump(dump string) goroutineSnapshot {
	snapshot := make(goroutineSnapshot)
	for _, block := range strings.Split(strings.TrimSpace(dump), "\n\n") {
		stack, ok := parseGoroutineStack(block)
		if !ok {
			continue
		}
		snapshot[stack.id] = stack
	}
	return snapshot
}

func parseGoroutineStack(block string) (goroutineStack, bool) {
	block = strings.TrimSpace(block)
	if block == "" {
		return goroutineStack{}, false
	}

	header, rest, _ := strings.Cut(block, "\n")
	id, state, ok := parseGoroutineHeader(header)
	if !ok {
		return goroutineStack{}, false
	}

	var functions []string
	for _, line := range strings.Split(rest, "\n") {
		if line == "" || strings.HasPrefix(line, "\t") {
			continue
		}
		fn := stackFunctionName(line)
		if fn != "" {
			functions = append(functions, fn)
		}
	}

	return goroutineStack{
		id:        id,
		state:     state,
		functions: functions,
		stack:     block,
	}, true
}

func parseGoroutineHeader(header string) (int64, string, bool) {
	if !strings.HasPrefix(header, "goroutine ") {
		return 0, "", false
	}
	rest := strings.TrimPrefix(header, "goroutine ")
	idText, rest, found := strings.Cut(rest, " ")
	if !found {
		return 0, "", false
	}
	id, err := strconv.ParseInt(idText, 10, 64)
	if err != nil {
		return 0, "", false
	}

	start := strings.IndexByte(rest, '[')
	end := strings.IndexByte(rest, ']')
	if start == -1 || end == -1 || end <= start {
		return id, "", true
	}
	return id, rest[start+1 : end], true
}

func stackFunctionName(line string) string {
	line = strings.TrimSpace(line)
	createdBy := false
	if strings.HasPrefix(line, "created by ") {
		createdBy = true
		line = strings.TrimPrefix(line, "created by ")
		if fn, _, found := strings.Cut(line, " in goroutine "); found {
			line = fn
		}
	}
	if !createdBy {
		if i := strings.LastIndexByte(line, '('); i >= 0 {
			line = line[:i]
		}
	}
	return strings.TrimSpace(line)
}

func leakedGoroutines(baseline, current goroutineSnapshot, options LeakCheckOptions) []goroutineStack {
	leaks := make([]goroutineStack, 0)
	for id, stack := range current {
		if _, ok := baseline[id]; ok {
			continue
		}
		if ignoreGoroutine(stack, options.IgnoredFunctions) {
			continue
		}
		leaks = append(leaks, stack)
	}
	sort.Slice(leaks, func(i, j int) bool {
		return leaks[i].id < leaks[j].id
	})
	return leaks
}

func ignoreGoroutine(stack goroutineStack, ignoredFunctions []string) bool {
	for _, fn := range stack.functions {
		for _, ignored := range ignoredFunctions {
			if functionNameMatches(fn, ignored) {
				return true
			}
		}
	}
	return false
}

func functionNameMatches(fn, pattern string) bool {
	if fn == pattern {
		return true
	}
	return strings.HasSuffix(fn, "."+pattern) || strings.HasSuffix(fn, "/"+pattern)
}

func formatLeakedGoroutines(leaks []goroutineStack) string {
	var out strings.Builder
	for i, leak := range leaks {
		if i > 0 {
			out.WriteString("\n\n")
		}
		out.WriteString(strings.TrimSpace(leak.stack))
		out.WriteByte('\n')
	}
	return out.String()
}
