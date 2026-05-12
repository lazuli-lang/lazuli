package apianalytics

import (
	"net/http"
	"net/http/httptest"
	"reflect"
	"sync"
	"testing"
	"time"
)

func TestCollectorSnapshotGroupsRecordsByRoute(t *testing.T) {
	collector := NewCollector()
	collector.Record(Record{
		Route:    "users.show",
		Endpoint: "/users/2",
		Method:   http.MethodGet,
		Status:   http.StatusOK,
		Duration: 10 * time.Millisecond,
		Bytes:    100,
	})
	collector.Record(Record{
		Route:    "users.show",
		Endpoint: "/users/1",
		Method:   http.MethodPost,
		Status:   http.StatusCreated,
		Duration: 30 * time.Millisecond,
		Bytes:    200,
	})
	collector.Record(Record{
		Route:    "orders.list",
		Endpoint: "/orders",
		Method:   http.MethodGet,
		Status:   http.StatusAccepted,
		Duration: 5 * time.Millisecond,
		Bytes:    50,
	})

	snapshot := collector.Snapshot()
	if got := len(snapshot.Routes); got != 2 {
		t.Fatalf("routes = %d, want 2: %#v", got, snapshot.Routes)
	}

	if snapshot.Routes[0].Route != "orders.list" {
		t.Fatalf("first route = %q, want orders.list", snapshot.Routes[0].Route)
	}

	users := snapshot.Routes[1]
	if users.Route != "users.show" {
		t.Fatalf("second route = %q, want users.show", users.Route)
	}
	if users.Requests != 2 {
		t.Fatalf("users requests = %d, want 2", users.Requests)
	}
	if users.TotalDuration != 40*time.Millisecond {
		t.Fatalf("users total duration = %s, want 40ms", users.TotalDuration)
	}
	if users.AverageDuration != 20*time.Millisecond {
		t.Fatalf("users average duration = %s, want 20ms", users.AverageDuration)
	}
	if users.MinDuration != 10*time.Millisecond {
		t.Fatalf("users min duration = %s, want 10ms", users.MinDuration)
	}
	if users.MaxDuration != 30*time.Millisecond {
		t.Fatalf("users max duration = %s, want 30ms", users.MaxDuration)
	}
	if users.Bytes != 300 {
		t.Fatalf("users bytes = %d, want 300", users.Bytes)
	}

	wantEndpoints := []EndpointSnapshot{
		{Endpoint: "/users/1", Requests: 1},
		{Endpoint: "/users/2", Requests: 1},
	}
	if !reflect.DeepEqual(users.Endpoints, wantEndpoints) {
		t.Fatalf("users endpoints = %#v, want %#v", users.Endpoints, wantEndpoints)
	}

	wantMethods := []MethodSnapshot{
		{Method: http.MethodGet, Requests: 1},
		{Method: http.MethodPost, Requests: 1},
	}
	if !reflect.DeepEqual(users.Methods, wantMethods) {
		t.Fatalf("users methods = %#v, want %#v", users.Methods, wantMethods)
	}

	wantStatuses := []StatusSnapshot{
		{Status: http.StatusOK, Requests: 1},
		{Status: http.StatusCreated, Requests: 1},
	}
	if !reflect.DeepEqual(users.Statuses, wantStatuses) {
		t.Fatalf("users statuses = %#v, want %#v", users.Statuses, wantStatuses)
	}
}

func TestCollectorNormalizesEmptyAndNegativeRecordValues(t *testing.T) {
	collector := NewCollector()
	collector.Record(Record{
		Duration: -time.Second,
		Bytes:    -42,
	})

	snapshot := collector.Snapshot()
	if got := len(snapshot.Routes); got != 1 {
		t.Fatalf("routes = %d, want 1", got)
	}

	route := snapshot.Routes[0]
	if route.Route != "/" {
		t.Fatalf("route = %q, want /", route.Route)
	}
	if route.TotalDuration != 0 {
		t.Fatalf("total duration = %s, want 0", route.TotalDuration)
	}
	if route.Bytes != 0 {
		t.Fatalf("bytes = %d, want 0", route.Bytes)
	}
	if !reflect.DeepEqual(route.Endpoints, []EndpointSnapshot{{Endpoint: "/", Requests: 1}}) {
		t.Fatalf("endpoints = %#v, want default endpoint", route.Endpoints)
	}
	if !reflect.DeepEqual(route.Methods, []MethodSnapshot{{Method: unknownMethod, Requests: 1}}) {
		t.Fatalf("methods = %#v, want default method", route.Methods)
	}
	if !reflect.DeepEqual(route.Statuses, []StatusSnapshot{{Status: http.StatusOK, Requests: 1}}) {
		t.Fatalf("statuses = %#v, want default status", route.Statuses)
	}
}

func TestMiddlewareRecordsStatusDurationAndBytes(t *testing.T) {
	collector := NewCollector()
	handler := Middleware("users.create", collector)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		time.Sleep(time.Millisecond)
		w.WriteHeader(http.StatusCreated)
		_, _ = w.Write([]byte("created"))
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/users", nil))

	if rec.Code != http.StatusCreated {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusCreated)
	}
	if rec.Body.String() != "created" {
		t.Fatalf("body = %q, want created", rec.Body.String())
	}

	snapshot := collector.Snapshot()
	if got := len(snapshot.Routes); got != 1 {
		t.Fatalf("routes = %d, want 1: %#v", got, snapshot.Routes)
	}

	route := snapshot.Routes[0]
	if route.Route != "users.create" {
		t.Fatalf("route = %q, want users.create", route.Route)
	}
	if route.Requests != 1 {
		t.Fatalf("requests = %d, want 1", route.Requests)
	}
	if route.TotalDuration <= 0 {
		t.Fatalf("total duration = %s, want positive duration", route.TotalDuration)
	}
	if route.Bytes != int64(len("created")) {
		t.Fatalf("bytes = %d, want %d", route.Bytes, len("created"))
	}
	if !reflect.DeepEqual(route.Endpoints, []EndpointSnapshot{{Endpoint: "/users", Requests: 1}}) {
		t.Fatalf("endpoints = %#v, want /users", route.Endpoints)
	}
	if !reflect.DeepEqual(route.Methods, []MethodSnapshot{{Method: http.MethodPost, Requests: 1}}) {
		t.Fatalf("methods = %#v, want POST", route.Methods)
	}
	if !reflect.DeepEqual(route.Statuses, []StatusSnapshot{{Status: http.StatusCreated, Requests: 1}}) {
		t.Fatalf("statuses = %#v, want 201", route.Statuses)
	}
}

func TestMiddlewareRecordsImplicitOK(t *testing.T) {
	collector := NewCollector()
	handler := Middleware("health", collector)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte("ok"))
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/healthz", nil))

	route := collector.Snapshot().Routes[0]
	if !reflect.DeepEqual(route.Statuses, []StatusSnapshot{{Status: http.StatusOK, Requests: 1}}) {
		t.Fatalf("statuses = %#v, want implicit 200", route.Statuses)
	}
	if route.Bytes != int64(len("ok")) {
		t.Fatalf("bytes = %d, want %d", route.Bytes, len("ok"))
	}
}

func TestMiddlewareWithNilCollectorLeavesHandlerUnchanged(t *testing.T) {
	handler := Middleware("ignored", nil)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/", nil))

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
}

func TestCollectorIsSafeForConcurrentUse(t *testing.T) {
	const workers = 32
	const recordsPerWorker = 100

	collector := NewCollector()
	var wg sync.WaitGroup
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < recordsPerWorker; j++ {
				collector.Record(Record{
					Route:    "search",
					Endpoint: "/search",
					Method:   http.MethodGet,
					Status:   http.StatusOK,
					Duration: time.Millisecond,
					Bytes:    3,
				})
				_ = collector.Snapshot()
			}
		}()
	}
	wg.Wait()

	snapshot := collector.Snapshot()
	if got := len(snapshot.Routes); got != 1 {
		t.Fatalf("routes = %d, want 1", got)
	}

	route := snapshot.Routes[0]
	wantRequests := uint64(workers * recordsPerWorker)
	if route.Requests != wantRequests {
		t.Fatalf("requests = %d, want %d", route.Requests, wantRequests)
	}
	if route.Bytes != int64(wantRequests*3) {
		t.Fatalf("bytes = %d, want %d", route.Bytes, wantRequests*3)
	}
}
