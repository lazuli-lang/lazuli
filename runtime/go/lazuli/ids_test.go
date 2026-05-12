package lazuli

import (
	"strings"
	"testing"
	"time"
)

func TestNewSortableIDWithEntropyIsDeterministic(t *testing.T) {
	now := time.UnixMilli(1469918176385).UTC()
	entropy := [SortableIDEntropyBytes]byte{
		0xff, 0xff, 0xff, 0xff, 0xff,
		0xff, 0xff, 0xff, 0xff, 0xff,
	}

	id, err := NewSortableIDWithEntropy(now, entropy)
	if err != nil {
		t.Fatalf("NewSortableIDWithEntropy error = %v", err)
	}

	const want = SortableID("01ARYZ6S41ZZZZZZZZZZZZZZZZ")
	if id != want {
		t.Fatalf("NewSortableIDWithEntropy = %q, want %q", id, want)
	}
	if len(id.String()) != SortableIDLength {
		t.Fatalf("SortableID length = %d, want %d", len(id.String()), SortableIDLength)
	}
}

func TestSortableIDTimestamp(t *testing.T) {
	now := time.Date(2026, 5, 12, 22, 17, 33, 987654321, time.FixedZone("BRT", -3*60*60))
	id, err := NewSortableIDWithEntropy(now, [SortableIDEntropyBytes]byte{})
	if err != nil {
		t.Fatalf("NewSortableIDWithEntropy error = %v", err)
	}

	got, err := id.Timestamp()
	if err != nil {
		t.Fatalf("Timestamp error = %v", err)
	}

	want := time.UnixMilli(now.UnixMilli()).UTC()
	if !got.Equal(want) {
		t.Fatalf("Timestamp = %s, want %s", got, want)
	}

	parsed, err := ParseSortableIDTimestamp(id.String())
	if err != nil {
		t.Fatalf("ParseSortableIDTimestamp error = %v", err)
	}
	if !parsed.Equal(want) {
		t.Fatalf("ParseSortableIDTimestamp = %s, want %s", parsed, want)
	}
}

func TestSortableIDSortsByTimestamp(t *testing.T) {
	now := time.UnixMilli(1469918176385).UTC()
	earlyEntropy := [SortableIDEntropyBytes]byte{
		0xff, 0xff, 0xff, 0xff, 0xff,
		0xff, 0xff, 0xff, 0xff, 0xff,
	}

	early, err := NewSortableIDWithEntropy(now, earlyEntropy)
	if err != nil {
		t.Fatalf("NewSortableIDWithEntropy early error = %v", err)
	}
	late, err := NewSortableIDWithEntropy(now.Add(time.Millisecond), [SortableIDEntropyBytes]byte{})
	if err != nil {
		t.Fatalf("NewSortableIDWithEntropy late error = %v", err)
	}

	if string(early) >= string(late) {
		t.Fatalf("early sortable id %q should sort before late sortable id %q", early, late)
	}
}

func TestParseSortableIDTimestampRejectsInvalidIDs(t *testing.T) {
	tests := []struct {
		name string
		id   string
	}{
		{name: "short", id: "01ARYZ6S41"},
		{name: "invalid character", id: "01ARYZ6S41I000000000000000"},
		{name: "timestamp overflow", id: "80000000000000000000000000"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, err := ParseSortableIDTimestamp(tt.id); err == nil {
				t.Fatalf("ParseSortableIDTimestamp(%q) error = nil, want error", tt.id)
			}
		})
	}
}

func TestNewSortableIDWithEntropyRejectsOutOfRangeTimestamp(t *testing.T) {
	tests := []struct {
		name string
		at   time.Time
	}{
		{name: "before epoch", at: time.UnixMilli(-1)},
		{name: "after max", at: time.UnixMilli(int64(sortableIDMaxMillis) + 1)},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if _, err := NewSortableIDWithEntropy(tt.at, [SortableIDEntropyBytes]byte{}); err == nil {
				t.Fatalf("NewSortableIDWithEntropy(%s) error = nil, want error", tt.at)
			}
		})
	}
}

func TestNewSortableIDAtUsesCrockfordAlphabet(t *testing.T) {
	id, err := NewSortableIDAt(time.UnixMilli(1469918176385).UTC())
	if err != nil {
		t.Fatalf("NewSortableIDAt error = %v", err)
	}
	if len(id.String()) != SortableIDLength {
		t.Fatalf("NewSortableIDAt length = %d, want %d", len(id.String()), SortableIDLength)
	}

	for i, c := range id.String() {
		if !strings.ContainsRune(sortableIDAlphabet, c) {
			t.Fatalf("NewSortableIDAt character at offset %d = %q, want Crockford base32", i, c)
		}
	}
}
