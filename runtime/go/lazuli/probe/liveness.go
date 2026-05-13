package probe

import (
	"encoding/json"
	"net/http"
)

// Liveness returns 200 + minimal JSON whenever the goroutine can
// respond. No deps. k8s livenessProbe pings this; failure = restart.
type Liveness struct{}

func NewLiveness() *Liveness { return &Liveness{} }

func (l *Liveness) Handler() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"status": "alive"})
	})
}
