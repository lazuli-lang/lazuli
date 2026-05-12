package lazuli

import (
	"net/http"
	"strconv"
	"strings"
	"time"
)

// CodeMaintenance is emitted as the problem details "code" extension when
// MaintenanceModeMiddleware rejects a request.
const CodeMaintenance = "maintenance"

const defaultMaintenanceDetail = "service temporarily unavailable for maintenance"

// MaintenanceModeEnabledProvider reports whether maintenance mode is active
// for a request. It is called per request, allowing generated apps to read a
// dynamic flag provider.
type MaintenanceModeEnabledProvider func(*http.Request) bool

// MaintenanceModeConfig configures MaintenanceModeMiddleware.
type MaintenanceModeConfig struct {
	// Enabled statically enables maintenance mode. Ignored when
	// EnabledProvider is set.
	Enabled bool

	// EnabledProvider dynamically reports whether maintenance mode is enabled.
	// When set, it is called for every non-bypassed request and overrides
	// Enabled.
	EnabledProvider MaintenanceModeEnabledProvider

	// RetryAfter sets the Retry-After response header when positive.
	RetryAfter time.Duration

	// BypassPaths lists exact URL paths that should always reach the next
	// handler, such as "/healthz".
	BypassPaths []string

	// Detail overrides the default problem details message.
	Detail string
}

// MaintenanceModeMiddleware returns a middleware that blocks requests with a
// 503 Service Unavailable problem response while maintenance mode is enabled.
func MaintenanceModeMiddleware(config MaintenanceModeConfig) Middleware {
	bypassPaths := append([]string(nil), config.BypassPaths...)

	return func(next http.Handler) http.Handler {
		if config.EnabledProvider == nil && !config.Enabled {
			return next
		}

		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if maintenancePathBypassed(r, bypassPaths) {
				next.ServeHTTP(w, r)
				return
			}

			enabled := config.Enabled
			if config.EnabledProvider != nil {
				enabled = config.EnabledProvider(r)
			}
			if !enabled {
				next.ServeHTTP(w, r)
				return
			}

			writeMaintenanceProblem(w, config)
		})
	}
}

func maintenancePathBypassed(r *http.Request, bypassPaths []string) bool {
	if r == nil || r.URL == nil {
		return false
	}

	path := r.URL.Path
	for _, bypassPath := range bypassPaths {
		if strings.TrimSpace(bypassPath) == path {
			return true
		}
	}
	return false
}

func writeMaintenanceProblem(w http.ResponseWriter, config MaintenanceModeConfig) {
	if header := maintenanceRetryAfter(config.RetryAfter); header != "" {
		w.Header().Set("Retry-After", header)
	}

	detail := config.Detail
	if detail == "" {
		detail = defaultMaintenanceDetail
	}

	WriteProblem(w, Problem{
		Status: http.StatusServiceUnavailable,
		Detail: detail,
		Extensions: map[string]any{
			"code": CodeMaintenance,
		},
	})
}

func maintenanceRetryAfter(d time.Duration) string {
	if d <= 0 {
		return ""
	}

	seconds := int64((d + time.Second - 1) / time.Second)
	if seconds < 1 {
		seconds = 1
	}
	return strconv.FormatInt(seconds, 10)
}
