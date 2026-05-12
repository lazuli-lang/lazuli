package diagnostics_test

import (
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/diagnostics"
)

func TestNPlusOneDetectorReportsFingerprintsByRequestAndSpan(t *testing.T) {
	t.Parallel()

	detector := diagnostics.NewNPlusOneDetector(3)
	observeNPlusOne(detector, diagnostics.NPlusOneObservation{
		RequestID:   "req-2",
		SpanID:      "handler",
		Fingerprint: "select posts where author_id = ?",
	}, 3)
	observeNPlusOne(detector, diagnostics.NPlusOneObservation{
		RequestID:   "req-1",
		SpanID:      "handler",
		Fingerprint: "select posts where author_id = ?",
	}, 3)
	observeNPlusOne(detector, diagnostics.NPlusOneObservation{
		RequestID:   "req-1",
		SpanID:      "loader",
		Fingerprint: "select posts where author_id = ?",
	}, 2)
	observeNPlusOne(detector, diagnostics.NPlusOneObservation{
		RequestID:   "req-1",
		SpanID:      "handler",
		Fingerprint: "select customers where id = ?",
	}, 1)

	want := diagnostics.NPlusOneReport{
		Threshold: 3,
		Detected:  true,
		Findings: []diagnostics.NPlusOneFinding{
			{
				RequestID:   "req-1",
				SpanID:      "handler",
				Fingerprint: "select posts where author_id = ?",
				Count:       3,
			},
			{
				RequestID:   "req-2",
				SpanID:      "handler",
				Fingerprint: "select posts where author_id = ?",
				Count:       3,
			},
		},
	}
	if got := detector.Snapshot(); !reflect.DeepEqual(got, want) {
		t.Fatalf("Snapshot() = %#v, want %#v", got, want)
	}
}

func TestNPlusOneDetectorSnapshotScopeAndResetScope(t *testing.T) {
	t.Parallel()

	detector := diagnostics.NewNPlusOneDetector(2)
	observeNPlusOne(detector, diagnostics.NPlusOneObservation{
		RequestID:   "req-1",
		SpanID:      "handler",
		Fingerprint: "select line_items where order_id = ?",
	}, 2)
	observeNPlusOne(detector, diagnostics.NPlusOneObservation{
		RequestID:   "req-1",
		SpanID:      "loader",
		Fingerprint: "select line_items where order_id = ?",
	}, 2)

	wantScope := diagnostics.NPlusOneReport{
		Threshold: 2,
		Detected:  true,
		Findings: []diagnostics.NPlusOneFinding{
			{
				RequestID:   "req-1",
				SpanID:      "handler",
				Fingerprint: "select line_items where order_id = ?",
				Count:       2,
			},
		},
	}
	if got := detector.SnapshotScope("req-1", "handler"); !reflect.DeepEqual(got, wantScope) {
		t.Fatalf("SnapshotScope() = %#v, want %#v", got, wantScope)
	}

	detector.ResetScope("req-1", "handler")

	wantRemaining := diagnostics.NPlusOneReport{
		Threshold: 2,
		Detected:  true,
		Findings: []diagnostics.NPlusOneFinding{
			{
				RequestID:   "req-1",
				SpanID:      "loader",
				Fingerprint: "select line_items where order_id = ?",
				Count:       2,
			},
		},
	}
	if got := detector.Snapshot(); !reflect.DeepEqual(got, wantRemaining) {
		t.Fatalf("Snapshot() after ResetScope = %#v, want %#v", got, wantRemaining)
	}

	detector.Reset()
	wantEmpty := diagnostics.NPlusOneReport{
		Threshold: 2,
		Findings:  []diagnostics.NPlusOneFinding{},
	}
	if got := detector.Snapshot(); !reflect.DeepEqual(got, wantEmpty) {
		t.Fatalf("Snapshot() after Reset = %#v, want %#v", got, wantEmpty)
	}
}

func TestNPlusOneDetectorSnapshotReturnsCopy(t *testing.T) {
	t.Parallel()

	detector := diagnostics.NewNPlusOneDetector(2)
	observeNPlusOne(detector, diagnostics.NPlusOneObservation{
		RequestID:   "req-1",
		SpanID:      "handler",
		Fingerprint: "select users where organization_id = ?",
	}, 2)

	snapshot := detector.Snapshot()
	snapshot.Findings[0].Count = 99
	snapshot.Findings[0].Fingerprint = "changed"

	want := diagnostics.NPlusOneFinding{
		RequestID:   "req-1",
		SpanID:      "handler",
		Fingerprint: "select users where organization_id = ?",
		Count:       2,
	}
	if got := detector.Snapshot().Findings[0]; got != want {
		t.Fatalf("Snapshot() finding after caller mutation = %#v, want %#v", got, want)
	}
}

func TestNPlusOneDetectorZeroValueUsesDefaultAndIgnoresIncompleteObservations(t *testing.T) {
	t.Parallel()

	var detector diagnostics.NPlusOneDetector
	if got := detector.Threshold(); got != diagnostics.DefaultNPlusOneThreshold {
		t.Fatalf("Threshold() = %d, want %d", got, diagnostics.DefaultNPlusOneThreshold)
	}

	observeNPlusOne(&detector, diagnostics.NPlusOneObservation{
		RequestID:   "req-1",
		Fingerprint: "select products where id = ?",
	}, diagnostics.DefaultNPlusOneThreshold)
	observeNPlusOne(&detector, diagnostics.NPlusOneObservation{
		RequestID: "req-1",
	}, diagnostics.DefaultNPlusOneThreshold)
	observeNPlusOne(&detector, diagnostics.NPlusOneObservation{
		Fingerprint: "select orphaned without scope",
	}, diagnostics.DefaultNPlusOneThreshold)

	want := diagnostics.NPlusOneReport{
		Threshold: diagnostics.DefaultNPlusOneThreshold,
		Detected:  true,
		Findings: []diagnostics.NPlusOneFinding{
			{
				RequestID:   "req-1",
				Fingerprint: "select products where id = ?",
				Count:       diagnostics.DefaultNPlusOneThreshold,
			},
		},
	}
	if got := detector.Snapshot(); !reflect.DeepEqual(got, want) {
		t.Fatalf("Snapshot() = %#v, want %#v", got, want)
	}
}

func observeNPlusOne(detector *diagnostics.NPlusOneDetector, observation diagnostics.NPlusOneObservation, times uint64) {
	for range times {
		detector.Observe(observation)
	}
}
