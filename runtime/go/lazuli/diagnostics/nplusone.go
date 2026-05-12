package diagnostics

import (
	"sort"
	"sync"
)

const (
	// DefaultNPlusOneThreshold is the number of identical query fingerprints in
	// one request/span scope required before the detector reports a finding.
	DefaultNPlusOneThreshold uint64 = 5
)

// NPlusOneObservation records one query fingerprint within a request/span
// scope. Fingerprints are supplied by callers so this helper remains
// independent from any database layer.
type NPlusOneObservation struct {
	RequestID   string `json:"request_id"`
	SpanID      string `json:"span_id,omitempty"`
	Fingerprint string `json:"fingerprint"`
}

// NPlusOneReport is a stable point-in-time report of detected repeated query
// fingerprints.
type NPlusOneReport struct {
	Threshold uint64            `json:"threshold"`
	Detected  bool              `json:"detected"`
	Findings  []NPlusOneFinding `json:"findings"`
}

// NPlusOneFinding is one fingerprint whose observation count met or exceeded
// the detector threshold within a single request/span scope.
type NPlusOneFinding struct {
	RequestID   string `json:"request_id"`
	SpanID      string `json:"span_id,omitempty"`
	Fingerprint string `json:"fingerprint"`
	Count       uint64 `json:"count"`
}

// NPlusOneDetector stores query fingerprint counts grouped by request/span. It
// is safe for concurrent use, and its zero value is ready to use with
// DefaultNPlusOneThreshold.
type NPlusOneDetector struct {
	mu        sync.RWMutex
	threshold uint64
	scopes    map[nPlusOneScopeKey]map[string]uint64
}

// NewNPlusOneDetector returns an empty detector. A threshold of zero uses
// DefaultNPlusOneThreshold.
func NewNPlusOneDetector(threshold uint64) *NPlusOneDetector {
	return &NPlusOneDetector{threshold: normalizeNPlusOneThreshold(threshold)}
}

// Observe records one query fingerprint. Observations with no request/span
// scope or no fingerprint are ignored.
func (d *NPlusOneDetector) Observe(observation NPlusOneObservation) {
	if d == nil || observation.Fingerprint == "" || !observation.hasScope() {
		return
	}

	key := nPlusOneScopeKey{
		requestID: observation.RequestID,
		spanID:    observation.SpanID,
	}

	d.mu.Lock()
	defer d.mu.Unlock()

	if d.scopes == nil {
		d.scopes = make(map[nPlusOneScopeKey]map[string]uint64)
	}
	counts := d.scopes[key]
	if counts == nil {
		counts = make(map[string]uint64)
		d.scopes[key] = counts
	}
	counts[observation.Fingerprint]++
}

// Snapshot returns a stable, sorted report of all request/span scopes.
func (d *NPlusOneDetector) Snapshot() NPlusOneReport {
	if d == nil {
		return NPlusOneReport{}
	}

	d.mu.RLock()
	defer d.mu.RUnlock()

	return d.snapshotLocked(func(nPlusOneScopeKey) bool {
		return true
	})
}

// SnapshotScope returns a stable, sorted report for one request/span scope.
func (d *NPlusOneDetector) SnapshotScope(requestID, spanID string) NPlusOneReport {
	if d == nil {
		return NPlusOneReport{}
	}

	d.mu.RLock()
	defer d.mu.RUnlock()

	want := nPlusOneScopeKey{requestID: requestID, spanID: spanID}
	return d.snapshotLocked(func(key nPlusOneScopeKey) bool {
		return key == want
	})
}

// Reset clears all recorded observations.
func (d *NPlusOneDetector) Reset() {
	if d == nil {
		return
	}

	d.mu.Lock()
	defer d.mu.Unlock()

	d.scopes = nil
}

// ResetScope clears observations for one request/span scope.
func (d *NPlusOneDetector) ResetScope(requestID, spanID string) {
	if d == nil {
		return
	}

	d.mu.Lock()
	defer d.mu.Unlock()

	delete(d.scopes, nPlusOneScopeKey{requestID: requestID, spanID: spanID})
}

// Threshold returns the detector threshold after applying the zero-value
// default.
func (d *NPlusOneDetector) Threshold() uint64 {
	if d == nil {
		return 0
	}

	d.mu.RLock()
	defer d.mu.RUnlock()

	return normalizeNPlusOneThreshold(d.threshold)
}

type nPlusOneScopeKey struct {
	requestID string
	spanID    string
}

func (d *NPlusOneDetector) snapshotLocked(includeScope func(nPlusOneScopeKey) bool) NPlusOneReport {
	threshold := normalizeNPlusOneThreshold(d.threshold)
	report := NPlusOneReport{
		Threshold: threshold,
		Findings:  make([]NPlusOneFinding, 0),
	}

	for key, counts := range d.scopes {
		if !includeScope(key) {
			continue
		}
		for fingerprint, count := range counts {
			if count < threshold {
				continue
			}
			report.Findings = append(report.Findings, NPlusOneFinding{
				RequestID:   key.requestID,
				SpanID:      key.spanID,
				Fingerprint: fingerprint,
				Count:       count,
			})
		}
	}

	sort.Slice(report.Findings, func(i, j int) bool {
		return nPlusOneFindingLess(report.Findings[i], report.Findings[j])
	})
	report.Detected = len(report.Findings) > 0
	return report
}

func (o NPlusOneObservation) hasScope() bool {
	return o.RequestID != "" || o.SpanID != ""
}

func normalizeNPlusOneThreshold(threshold uint64) uint64 {
	if threshold == 0 {
		return DefaultNPlusOneThreshold
	}
	return threshold
}

func nPlusOneFindingLess(a, b NPlusOneFinding) bool {
	if a.RequestID != b.RequestID {
		return a.RequestID < b.RequestID
	}
	if a.SpanID != b.SpanID {
		return a.SpanID < b.SpanID
	}
	return a.Fingerprint < b.Fingerprint
}
