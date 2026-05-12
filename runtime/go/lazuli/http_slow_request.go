package lazuli

import (
	"context"
	"log/slog"
	"net/http"
	"time"
)

// SlowRequestEvent describes a completed HTTP request whose duration exceeded
// the configured slow-request threshold.
type SlowRequestEvent struct {
	Method    string
	Path      string
	Status    int
	RequestID string
	Duration  time.Duration
	Threshold time.Duration
}

// SlowRequestObserver receives slow HTTP request observations.
type SlowRequestObserver interface {
	ObserveSlowRequest(context.Context, SlowRequestEvent)
}

// SlowRequestObserverFunc adapts a function to SlowRequestObserver.
type SlowRequestObserverFunc func(context.Context, SlowRequestEvent)

// ObserveSlowRequest calls f when f is non-nil.
func (f SlowRequestObserverFunc) ObserveSlowRequest(ctx context.Context, event SlowRequestEvent) {
	if f != nil {
		f(ctx, event)
	}
}

// SlowRequestLogger returns an observer that writes slow requests through slog.
// A nil logger uses slog.Default().
func SlowRequestLogger(logger *slog.Logger) SlowRequestObserver {
	return SlowRequestObserverFunc(func(ctx context.Context, event SlowRequestEvent) {
		LogSlowRequest(ctx, logger, event)
	})
}

// LogSlowRequest writes a structured warning for a slow HTTP request.
func LogSlowRequest(ctx context.Context, logger *slog.Logger, event SlowRequestEvent) {
	if ctx == nil {
		ctx = context.Background()
	}
	if logger == nil {
		logger = slog.Default()
	}

	attrs := []any{
		"method", event.Method,
		"path", event.Path,
		"status", event.Status,
		"duration_ms", event.Duration.Milliseconds(),
		"threshold_ms", event.Threshold.Milliseconds(),
	}
	if event.RequestID != "" {
		attrs = append(attrs, "request_id", event.RequestID)
	}

	logger.WarnContext(ctx, "lazuli slow http request", attrs...)
}

// SlowRequestMiddleware measures completed requests and notifies observer when
// the elapsed handler time meets or exceeds threshold. A non-positive threshold
// or nil observer disables the middleware and leaves requests untouched.
func SlowRequestMiddleware(threshold time.Duration, observer SlowRequestObserver) Middleware {
	return func(next http.Handler) http.Handler {
		if threshold <= 0 || observer == nil {
			return next
		}

		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			start := time.Now()
			rec := &slowRequestResponseWriter{
				ResponseWriter: w,
				status:         http.StatusOK,
			}

			next.ServeHTTP(rec, r)

			duration := time.Since(start)
			if duration < threshold {
				return
			}

			observer.ObserveSlowRequest(slowRequestContext(r), SlowRequestEvent{
				Method:    slowRequestMethod(r),
				Path:      slowRequestPath(r),
				Status:    rec.status,
				RequestID: slowRequestID(r, rec.Header()),
				Duration:  duration,
				Threshold: threshold,
			})
		})
	}
}

type slowRequestResponseWriter struct {
	http.ResponseWriter
	status      int
	wroteHeader bool
}

func (w *slowRequestResponseWriter) WriteHeader(code int) {
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

func (w *slowRequestResponseWriter) Write(p []byte) (int, error) {
	if !w.wroteHeader {
		w.WriteHeader(http.StatusOK)
	}
	return w.ResponseWriter.Write(p)
}

func (w *slowRequestResponseWriter) Unwrap() http.ResponseWriter {
	return w.ResponseWriter
}

func slowRequestContext(r *http.Request) context.Context {
	if r == nil || r.Context() == nil {
		return context.Background()
	}
	return r.Context()
}

func slowRequestMethod(r *http.Request) string {
	if r == nil {
		return ""
	}
	return r.Method
}

func slowRequestPath(r *http.Request) string {
	if r == nil || r.URL == nil {
		return ""
	}
	if r.URL.Path != "" {
		return r.URL.Path
	}
	return r.RequestURI
}

func slowRequestID(r *http.Request, header http.Header) string {
	if r != nil {
		if id := RequestID(r.Context()); id != "" {
			return id
		}
		if id := r.Header.Get(requestIDHeader); id != "" {
			return id
		}
	}
	return header.Get(requestIDHeader)
}
