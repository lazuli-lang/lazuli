package perf

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sort"
	"strings"
	"sync"
	"time"
)

// LoadTestConfig describes an in-process HTTP load test.
type LoadTestConfig struct {
	// Requests is the total number of requests to execute.
	Requests int
	// Concurrency is the maximum number of in-flight requests.
	Concurrency int
	// Method is the HTTP method. Empty defaults to GET.
	Method string
	// Path is the request target path. Empty defaults to /.
	Path string
	// Header is copied onto every request.
	Header http.Header
	// Body is copied into every request body.
	Body []byte
	// RampUp spreads scheduled request starts over this duration.
	RampUp time.Duration
}

// Validate returns an error when c cannot be used for a load test.
func (c LoadTestConfig) Validate() error {
	if c.Requests <= 0 {
		return errors.New("loadtest: requests must be positive")
	}
	if c.Concurrency <= 0 {
		return errors.New("loadtest: concurrency must be positive")
	}
	if c.Method != "" && !validHTTPToken(c.Method) {
		return fmt.Errorf("loadtest: invalid method %q", c.Method)
	}
	if c.Path != "" && !strings.HasPrefix(c.Path, "/") {
		return fmt.Errorf("loadtest: path must start with /")
	}
	if c.RampUp < 0 {
		return errors.New("loadtest: ramp up must not be negative")
	}
	return nil
}

func (c LoadTestConfig) method() string {
	if c.Method == "" {
		return http.MethodGet
	}
	return c.Method
}

func (c LoadTestConfig) path() string {
	if c.Path == "" {
		return "/"
	}
	return c.Path
}

// ScheduledRequest is a planned request start in a load test.
type ScheduledRequest struct {
	Index      int
	Worker     int
	StartAfter time.Duration
}

// PlanLoadTestSchedule returns a deterministic request schedule for c.
func PlanLoadTestSchedule(c LoadTestConfig) ([]ScheduledRequest, error) {
	if err := c.Validate(); err != nil {
		return nil, err
	}

	schedule := make([]ScheduledRequest, c.Requests)
	for i := range schedule {
		startAfter := time.Duration(0)
		if c.RampUp > 0 && c.Requests > 1 {
			startAfter = time.Duration(int64(c.RampUp) * int64(i) / int64(c.Requests-1))
		}
		schedule[i] = ScheduledRequest{
			Index:      i,
			Worker:     i % c.Concurrency,
			StartAfter: startAfter,
		}
	}
	return schedule, nil
}

// LoadTestSample records one HTTP request outcome.
type LoadTestSample struct {
	Index      int
	StatusCode int
	Latency    time.Duration
	Error      string
}

// LoadTestSummary summarizes load test samples.
type LoadTestSummary struct {
	Requests    int
	Successes   int
	Failures    int
	StatusCodes map[int]int
	Errors      map[string]int
	MinLatency  time.Duration
	MaxLatency  time.Duration
	AvgLatency  time.Duration
}

// LoadTestResult is the complete output of a Runner.
type LoadTestResult struct {
	Schedule []ScheduledRequest
	Samples  []LoadTestSample
	Summary  LoadTestSummary
}

// Runner executes a load test against an in-process HTTP handler.
type Runner struct {
	Handler http.Handler
	Config  LoadTestConfig
}

// Run executes the configured load test. If ctx is canceled, Run stops starting
// new work and returns the partial result with ctx.Err().
func (r Runner) Run(ctx context.Context) (LoadTestResult, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if r.Handler == nil {
		return LoadTestResult{}, errors.New("loadtest: handler is required")
	}

	schedule, err := PlanLoadTestSchedule(r.Config)
	if err != nil {
		return LoadTestResult{}, err
	}

	result := LoadTestResult{
		Schedule: schedule,
		Samples:  make([]LoadTestSample, len(schedule)),
	}

	sem := make(chan struct{}, r.Config.Concurrency)
	var wg sync.WaitGroup
	started := 0
	for _, planned := range schedule {
		if err := waitForStart(ctx, planned.StartAfter); err != nil {
			wg.Wait()
			result.Samples = result.Samples[:started]
			result.Summary = SummarizeLoadTestSamples(result.Samples)
			return result, err
		}
		if err := acquire(ctx, sem); err != nil {
			wg.Wait()
			result.Samples = result.Samples[:started]
			result.Summary = SummarizeLoadTestSamples(result.Samples)
			return result, err
		}

		started++
		wg.Add(1)
		go func(planned ScheduledRequest) {
			defer wg.Done()
			defer func() {
				<-sem
			}()
			result.Samples[planned.Index] = r.runOne(ctx, planned.Index)
		}(planned)
	}
	wg.Wait()

	result.Summary = SummarizeLoadTestSamples(result.Samples)
	return result, ctx.Err()
}

func (r Runner) runOne(ctx context.Context, index int) LoadTestSample {
	started := time.Now()
	req := httptest.NewRequest(r.Config.method(), r.Config.path(), bytes.NewReader(r.Config.Body)).WithContext(ctx)
	for key, values := range r.Config.Header {
		for _, value := range values {
			req.Header.Add(key, value)
		}
	}

	rec := httptest.NewRecorder()
	r.Handler.ServeHTTP(rec, req)

	sample := LoadTestSample{
		Index:      index,
		StatusCode: rec.Code,
		Latency:    time.Since(started),
	}
	if err := ctx.Err(); err != nil {
		sample.Error = err.Error()
	} else if rec.Code >= http.StatusInternalServerError {
		sample.Error = http.StatusText(rec.Code)
		if sample.Error == "" {
			sample.Error = fmt.Sprintf("status %d", rec.Code)
		}
	}
	return sample
}

func waitForStart(ctx context.Context, after time.Duration) error {
	if after <= 0 {
		return ctx.Err()
	}

	timer := time.NewTimer(after)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

func acquire(ctx context.Context, sem chan struct{}) error {
	select {
	case <-ctx.Done():
		return ctx.Err()
	case sem <- struct{}{}:
		return nil
	}
}

// SummarizeLoadTestSamples returns latency, status, and error counts for samples.
func SummarizeLoadTestSamples(samples []LoadTestSample) LoadTestSummary {
	summary := LoadTestSummary{
		Requests:    len(samples),
		StatusCodes: make(map[int]int),
		Errors:      make(map[string]int),
	}

	var total time.Duration
	for i, sample := range samples {
		if sample.StatusCode != 0 {
			summary.StatusCodes[sample.StatusCode]++
		}
		if sample.Error != "" {
			summary.Failures++
			summary.Errors[sample.Error]++
		} else {
			summary.Successes++
		}

		if i == 0 || sample.Latency < summary.MinLatency {
			summary.MinLatency = sample.Latency
		}
		if sample.Latency > summary.MaxLatency {
			summary.MaxLatency = sample.Latency
		}
		total += sample.Latency
	}

	if len(samples) > 0 {
		summary.AvgLatency = total / time.Duration(len(samples))
	}
	return summary
}

func validHTTPToken(token string) bool {
	if token == "" {
		return false
	}
	for _, r := range token {
		if r > 127 || !isTokenByte(byte(r)) {
			return false
		}
	}
	return true
}

func isTokenByte(b byte) bool {
	if 'a' <= b && b <= 'z' || 'A' <= b && b <= 'Z' || '0' <= b && b <= '9' {
		return true
	}
	switch b {
	case '!', '#', '$', '%', '&', '\'', '*', '+', '-', '.', '^', '_', '`', '|', '~':
		return true
	default:
		return false
	}
}

// SortedLoadTestErrors returns error keys in stable order for callers that need
// deterministic presentation.
func SortedLoadTestErrors(summary LoadTestSummary) []string {
	keys := make([]string, 0, len(summary.Errors))
	for key := range summary.Errors {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}
