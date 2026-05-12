// Package apianalytics collects in-memory HTTP API request analytics.
package apianalytics

import (
	"net/http"
	"sort"
	"sync"
	"time"
)

const unknownMethod = "UNKNOWN"

// Record is one completed API request observation.
type Record struct {
	// Route is the stable route name used to group snapshots.
	Route string `json:"route"`
	// Endpoint is the concrete request endpoint, usually the URL path.
	Endpoint string `json:"endpoint"`
	// Method is the HTTP method for the request.
	Method string `json:"method"`
	// Status is the final HTTP response status code.
	Status int `json:"status"`
	// Duration is the elapsed handler time for the request.
	Duration time.Duration `json:"duration"`
	// Bytes is the number of response body bytes written.
	Bytes int64 `json:"bytes"`
}

// Snapshot is a point-in-time copy of all collected route analytics.
type Snapshot struct {
	Routes []RouteSnapshot `json:"routes"`
}

// RouteSnapshot is the aggregate analytics view for one route.
type RouteSnapshot struct {
	// Route is the stable route name for this group.
	Route string `json:"route"`
	// Requests is the total number of recorded requests.
	Requests uint64 `json:"requests"`
	// TotalDuration is the sum of all recorded request durations.
	TotalDuration time.Duration `json:"total_duration"`
	// AverageDuration is the mean recorded request duration.
	AverageDuration time.Duration `json:"average_duration"`
	// MinDuration is the shortest recorded request duration.
	MinDuration time.Duration `json:"min_duration"`
	// MaxDuration is the longest recorded request duration.
	MaxDuration time.Duration `json:"max_duration"`
	// Bytes is the total number of response body bytes written.
	Bytes int64 `json:"bytes"`
	// Endpoints is the request count by concrete endpoint.
	Endpoints []EndpointSnapshot `json:"endpoints"`
	// Methods is the request count by HTTP method.
	Methods []MethodSnapshot `json:"methods"`
	// Statuses is the request count by HTTP response status.
	Statuses []StatusSnapshot `json:"statuses"`
}

// EndpointSnapshot is the request count for one endpoint within a route.
type EndpointSnapshot struct {
	Endpoint string `json:"endpoint"`
	Requests uint64 `json:"requests"`
}

// MethodSnapshot is the request count for one HTTP method within a route.
type MethodSnapshot struct {
	Method   string `json:"method"`
	Requests uint64 `json:"requests"`
}

// StatusSnapshot is the request count for one HTTP status within a route.
type StatusSnapshot struct {
	Status   int    `json:"status"`
	Requests uint64 `json:"requests"`
}

// Collector stores API analytics in memory. It is safe for concurrent use.
type Collector struct {
	mu     sync.RWMutex
	routes map[string]*routeState
}

// NewCollector returns an empty in-memory analytics collector.
func NewCollector() *Collector {
	return &Collector{routes: make(map[string]*routeState)}
}

// Record adds one completed API request observation to the collector.
func (c *Collector) Record(record Record) {
	if c == nil {
		return
	}

	record = normalizeRecord(record)

	c.mu.Lock()
	defer c.mu.Unlock()

	if c.routes == nil {
		c.routes = make(map[string]*routeState)
	}
	state := c.routes[record.Route]
	if state == nil {
		state = newRouteState()
		c.routes[record.Route] = state
	}
	state.add(record)
}

// Snapshot returns a stable, sorted copy of the collected analytics grouped by
// route.
func (c *Collector) Snapshot() Snapshot {
	if c == nil {
		return Snapshot{}
	}

	c.mu.RLock()
	defer c.mu.RUnlock()

	snapshot := Snapshot{Routes: make([]RouteSnapshot, 0, len(c.routes))}
	for route, state := range c.routes {
		snapshot.Routes = append(snapshot.Routes, state.snapshot(route))
	}
	sort.Slice(snapshot.Routes, func(i, j int) bool {
		return snapshot.Routes[i].Route < snapshot.Routes[j].Route
	})
	return snapshot
}

// Middleware records request analytics for routeName around next. Passing a
// nil collector leaves the handler unchanged.
func Middleware(routeName string, collector *Collector) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		if collector == nil {
			return next
		}
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			start := time.Now()
			rec := &responseRecorder{
				ResponseWriter: w,
				status:         http.StatusOK,
			}
			defer func() {
				collector.Record(Record{
					Route:    routeName,
					Endpoint: requestEndpoint(r),
					Method:   requestMethod(r),
					Status:   rec.status,
					Duration: time.Since(start),
					Bytes:    rec.bytes,
				})
			}()

			next.ServeHTTP(rec, r)
		})
	}
}

type routeState struct {
	requests      uint64
	totalDuration time.Duration
	minDuration   time.Duration
	maxDuration   time.Duration
	bytes         int64
	endpoints     map[string]uint64
	methods       map[string]uint64
	statuses      map[int]uint64
}

func newRouteState() *routeState {
	return &routeState{
		endpoints: make(map[string]uint64),
		methods:   make(map[string]uint64),
		statuses:  make(map[int]uint64),
	}
}

func (s *routeState) add(record Record) {
	if s.requests == 0 || record.Duration < s.minDuration {
		s.minDuration = record.Duration
	}
	if record.Duration > s.maxDuration {
		s.maxDuration = record.Duration
	}

	s.requests++
	s.totalDuration += record.Duration
	s.bytes += record.Bytes
	s.endpoints[record.Endpoint]++
	s.methods[record.Method]++
	s.statuses[record.Status]++
}

func (s *routeState) snapshot(route string) RouteSnapshot {
	return RouteSnapshot{
		Route:           route,
		Requests:        s.requests,
		TotalDuration:   s.totalDuration,
		AverageDuration: averageDuration(s.totalDuration, s.requests),
		MinDuration:     s.minDuration,
		MaxDuration:     s.maxDuration,
		Bytes:           s.bytes,
		Endpoints:       endpointSnapshots(s.endpoints),
		Methods:         methodSnapshots(s.methods),
		Statuses:        statusSnapshots(s.statuses),
	}
}

type responseRecorder struct {
	http.ResponseWriter
	status      int
	bytes       int64
	wroteHeader bool
}

func (w *responseRecorder) WriteHeader(code int) {
	if code >= 100 && code < 200 && code != http.StatusSwitchingProtocols {
		w.ResponseWriter.WriteHeader(code)
		return
	}
	if w.wroteHeader {
		return
	}
	w.wroteHeader = true
	w.status = code
	w.ResponseWriter.WriteHeader(code)
}

func (w *responseRecorder) Write(p []byte) (int, error) {
	if !w.wroteHeader {
		w.WriteHeader(http.StatusOK)
	}
	n, err := w.ResponseWriter.Write(p)
	w.bytes += int64(n)
	return n, err
}

func (w *responseRecorder) Unwrap() http.ResponseWriter {
	return w.ResponseWriter
}

func normalizeRecord(record Record) Record {
	if record.Endpoint == "" {
		record.Endpoint = "/"
	}
	if record.Route == "" {
		record.Route = record.Endpoint
	}
	if record.Method == "" {
		record.Method = unknownMethod
	}
	if record.Status == 0 {
		record.Status = http.StatusOK
	}
	if record.Duration < 0 {
		record.Duration = 0
	}
	if record.Bytes < 0 {
		record.Bytes = 0
	}
	return record
}

func requestEndpoint(r *http.Request) string {
	if r == nil || r.URL == nil {
		return ""
	}
	if r.URL.Path != "" {
		return r.URL.Path
	}
	return r.RequestURI
}

func requestMethod(r *http.Request) string {
	if r == nil {
		return ""
	}
	return r.Method
}

func averageDuration(total time.Duration, count uint64) time.Duration {
	if count == 0 {
		return 0
	}
	return time.Duration(int64(total) / int64(count))
}

func endpointSnapshots(counts map[string]uint64) []EndpointSnapshot {
	snapshots := make([]EndpointSnapshot, 0, len(counts))
	for endpoint, requests := range counts {
		snapshots = append(snapshots, EndpointSnapshot{
			Endpoint: endpoint,
			Requests: requests,
		})
	}
	sort.Slice(snapshots, func(i, j int) bool {
		return snapshots[i].Endpoint < snapshots[j].Endpoint
	})
	return snapshots
}

func methodSnapshots(counts map[string]uint64) []MethodSnapshot {
	snapshots := make([]MethodSnapshot, 0, len(counts))
	for method, requests := range counts {
		snapshots = append(snapshots, MethodSnapshot{
			Method:   method,
			Requests: requests,
		})
	}
	sort.Slice(snapshots, func(i, j int) bool {
		return snapshots[i].Method < snapshots[j].Method
	})
	return snapshots
}

func statusSnapshots(counts map[int]uint64) []StatusSnapshot {
	snapshots := make([]StatusSnapshot, 0, len(counts))
	for status, requests := range counts {
		snapshots = append(snapshots, StatusSnapshot{
			Status:   status,
			Requests: requests,
		})
	}
	sort.Slice(snapshots, func(i, j int) bool {
		return snapshots[i].Status < snapshots[j].Status
	})
	return snapshots
}
