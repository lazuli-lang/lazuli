package observability

import (
	"reflect"
	"testing"
	"time"
)

func TestProfileOpIdentityFromLabelsExtractsCanonicalLabels(t *testing.T) {
	identity, ok := ProfileOpIdentityFromLabels(map[string]string{
		ProfileLabelFeature:        " customer ",
		ProfileLabelKind:           " command ",
		ProfileLabelOp:             " create_customer ",
		ProfileLabelSource:         " features/customer.lzi:42:8 ",
		ProfileLabelPatternID:      " command_pgx_insert ",
		ProfileLabelPatternVersion: " v1 ",
	})

	if !ok {
		t.Fatal("ProfileOpIdentityFromLabels(...) ok = false, want true")
	}

	want := ProfileOpIdentity{
		Feature:        "customer",
		Kind:           "command",
		Op:             "create_customer",
		Source:         "features/customer.lzi:42:8",
		PatternID:      "command_pgx_insert",
		PatternVersion: "v1",
	}
	if identity != want {
		t.Fatalf("ProfileOpIdentityFromLabels(...) = %#v, want %#v", identity, want)
	}
}

func TestProfileOpIdentityFromLabelsAcceptsStartOpNameLabel(t *testing.T) {
	identity, ok := ProfileOpIdentityFromLabels(map[string]string{
		ProfileLabelFeature: "customer",
		ProfileLabelKind:    "command",
		opLabelNameKey:      "create_customer",
	})

	if !ok {
		t.Fatal("ProfileOpIdentityFromLabels(...) ok = false, want true")
	}
	if identity.Op != "create_customer" {
		t.Fatalf("identity.Op = %q, want %q", identity.Op, "create_customer")
	}
}

func TestProfileOpIdentityFromLabelsPrefersCanonicalOpLabel(t *testing.T) {
	identity, ok := ProfileOpIdentityFromLabels(map[string]string{
		ProfileLabelFeature: "customer",
		ProfileLabelKind:    "command",
		ProfileLabelOp:      "create_customer",
		opLabelNameKey:      "legacy_create_customer",
	})

	if !ok {
		t.Fatal("ProfileOpIdentityFromLabels(...) ok = false, want true")
	}
	if identity.Op != "create_customer" {
		t.Fatalf("identity.Op = %q, want canonical op label", identity.Op)
	}
}

func TestProfileOpIdentityFromLabelsHandlesMissingLabels(t *testing.T) {
	for _, labels := range []map[string]string{
		nil,
		{},
		{ProfileLabelFeature: "customer"},
		{
			ProfileLabelFeature: "customer",
			ProfileLabelKind:    "command",
			ProfileLabelOp:      " ",
		},
	} {
		if identity, ok := ProfileOpIdentityFromLabels(labels); ok {
			t.Fatalf("ProfileOpIdentityFromLabels(%v) = %#v, true; want false", labels, identity)
		}
	}
}

func TestNormalizeProfileLabelsCanonicalizesAndCopies(t *testing.T) {
	labels := map[string]string{
		" feature ":        " customer ",
		ProfileLabelKind:   " command ",
		ProfileLabelOp:     " create_customer ",
		opLabelNameKey:     " legacy_create_customer ",
		ProfileLabelSource: " features/customer.lzi:42:8 ",
		"tenant":           " acme ",
		"empty":            " ",
		" ":                "ignored",
	}

	got := NormalizeProfileLabels(labels)

	want := map[string]string{
		ProfileLabelFeature: "customer",
		ProfileLabelKind:    "command",
		ProfileLabelOp:      "create_customer",
		ProfileLabelSource:  "features/customer.lzi:42:8",
		"tenant":            "acme",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("NormalizeProfileLabels(...) = %#v, want %#v", got, want)
	}

	labels[ProfileLabelOp] = "mutated"
	if got[ProfileLabelOp] != "create_customer" {
		t.Fatalf("normalized labels changed after input mutation: %#v", got)
	}
}

func TestNormalizeProfileLabelsUsesStartOpNameWhenOpMissing(t *testing.T) {
	got := NormalizeProfileLabels(map[string]string{
		ProfileLabelFeature: "customer",
		ProfileLabelKind:    "command",
		opLabelNameKey:      "create_customer",
	})

	if got[ProfileLabelOp] != "create_customer" {
		t.Fatalf("normalized op = %q, want %q", got[ProfileLabelOp], "create_customer")
	}
	if _, ok := got[opLabelNameKey]; ok {
		t.Fatalf("normalized labels kept %q alias: %#v", opLabelNameKey, got)
	}
}

func TestBuildProfileReportAttributesRawStartOpLabels(t *testing.T) {
	report := BuildProfileReport([]ProfileSample{
		{
			Labels: map[string]string{
				ProfileLabelFeature:        "customer",
				ProfileLabelKind:           "command",
				opLabelNameKey:             "create_customer",
				ProfileLabelPatternID:      "command_pgx_insert",
				ProfileLabelPatternVersion: "v1",
			},
			CPUDuration: time.Millisecond,
		},
	}, 1)

	profileLabelsAssertRows(t, report.Ops, []ProfileOpReport{
		{
			Feature:        "customer",
			Kind:           "command",
			Op:             "create_customer",
			PatternID:      "command_pgx_insert",
			PatternVersion: "v1",
			SampleCount:    1,
			CPUDuration:    time.Millisecond,
		},
	})
	if report.Unattributed.SampleCount != 0 {
		t.Fatalf("unattributed sample count = %d, want 0", report.Unattributed.SampleCount)
	}
}

func profileLabelsAssertRows(t *testing.T, got, want []ProfileOpReport) {
	t.Helper()

	if !reflect.DeepEqual(got, want) {
		t.Fatalf("rows = %+v, want %+v", got, want)
	}
}
