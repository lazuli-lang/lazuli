package cache

import (
	"errors"
	"reflect"
	"testing"
	"time"
)

func TestBuildFragmentKeyNormalizesInputs(t *testing.T) {
	vary := map[string]any{"locale": "en", "id": 7}
	varyHash, err := HashArgs(vary)
	if err != nil {
		t.Fatalf("HashArgs(vary) error = %v", err)
	}

	key, err := BuildFragmentKey(FragmentKeyParts{
		Spec: FragmentSpec{
			Key:       " Product/Card ",
			Namespace: " Shop Front ",
			Version:   " V2 ",
			Tags:      []string{"product", "customer", "Product"},
		},
		Vary: vary,
	})
	if err != nil {
		t.Fatalf("BuildFragmentKey() error = %v", err)
	}

	want := "fragment|shop-front|product-card|v2|customer,product|" + varyHash
	if key != want {
		t.Fatalf("BuildFragmentKey() = %q, want %q", key, want)
	}
}

func TestBuildFragmentKeyUsesOverridesAndUnscopedNamespace(t *testing.T) {
	vary := struct {
		ID string `json:"id"`
	}{ID: "42"}
	varyHash, err := HashArgs(vary)
	if err != nil {
		t.Fatalf("HashArgs(vary) error = %v", err)
	}

	key, err := BuildFragmentKey(FragmentKeyParts{
		Spec: FragmentSpec{
			Key:     "from-spec",
			Version: "v1",
			Tags:    []string{"from-spec"},
		},
		Key:     "override-key",
		Version: "v2",
		Tags:    []string{},
		Vary:    vary,
	})
	if err != nil {
		t.Fatalf("BuildFragmentKey() error = %v", err)
	}

	want := "fragment|-|override-key|v2|-|" + varyHash
	if key != want {
		t.Fatalf("BuildFragmentKey() = %q, want %q", key, want)
	}
}

func TestBuildFragmentKeyReturnsValidationErrors(t *testing.T) {
	if _, err := BuildFragmentKey(FragmentKeyParts{Version: "v1"}); !errors.Is(err, ErrFragmentKeyRequired) {
		t.Fatalf("BuildFragmentKey(missing key) error = %v, want %v", err, ErrFragmentKeyRequired)
	}
	if _, err := BuildFragmentKey(FragmentKeyParts{Key: "hero"}); !errors.Is(err, ErrFragmentVersionRequired) {
		t.Fatalf("BuildFragmentKey(missing version) error = %v, want %v", err, ErrFragmentVersionRequired)
	}
	if _, err := BuildFragmentKey(FragmentKeyParts{Key: "hero", Version: "v1", Tags: []string{"!!!"}}); !errors.Is(err, ErrFragmentTagInvalid) {
		t.Fatalf("BuildFragmentKey(invalid tag) error = %v, want %v", err, ErrFragmentTagInvalid)
	}
	if _, err := BuildFragmentKey(FragmentKeyParts{Key: "hero", Version: "v1", Vary: make(chan int)}); err == nil {
		t.Fatal("BuildFragmentKey(channel vary) error = nil, want error")
	}
}

func TestValidateFragmentSpec(t *testing.T) {
	err := ValidateFragmentSpec(FragmentSpec{
		Tags:                 []string{"!!!"},
		TTL:                  -time.Minute,
		StaleWhileRevalidate: time.Second,
	})
	for _, want := range []error{
		ErrFragmentKeyRequired,
		ErrFragmentVersionRequired,
		ErrFragmentTagInvalid,
		ErrFragmentStaleWhileRevalidateInvalid,
	} {
		if !errors.Is(err, want) {
			t.Fatalf("ValidateFragmentSpec() error = %v, want errors.Is(%v)", err, want)
		}
	}

	err = ValidateFragmentSpec(FragmentSpec{
		Key:                  "hero",
		Version:              "v1",
		TTL:                  time.Minute,
		StaleWhileRevalidate: 30 * time.Second,
		Tags:                 []string{"hero", "shared"},
	})
	if err != nil {
		t.Fatalf("ValidateFragmentSpec(valid) error = %v", err)
	}
}

func TestFragmentMetadataStates(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	meta := NewFragmentMetadata(now, 10*time.Second, 20*time.Second)

	if state := meta.State(now.Add(9 * time.Second)); state != FragmentFresh {
		t.Fatalf("State(before ttl) = %v, want FragmentFresh", state)
	}
	if meta.ShouldRevalidate(now.Add(9 * time.Second)) {
		t.Fatal("ShouldRevalidate(before ttl) = true, want false")
	}
	if state := meta.State(now.Add(10 * time.Second)); state != FragmentStale {
		t.Fatalf("State(at ttl) = %v, want FragmentStale", state)
	}
	if !meta.ShouldRevalidate(now.Add(10 * time.Second)) {
		t.Fatal("ShouldRevalidate(at ttl) = false, want true")
	}
	if !meta.CanServe(now.Add(29 * time.Second)) {
		t.Fatal("CanServe(inside stale window) = false, want true")
	}
	if state := meta.State(now.Add(30 * time.Second)); state != FragmentExpired {
		t.Fatalf("State(at stale end) = %v, want FragmentExpired", state)
	}
	if meta.CanServe(now.Add(30 * time.Second)) {
		t.Fatal("CanServe(at stale end) = true, want false")
	}
}

func TestFragmentMetadataNegativeTTLNeverExpires(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	meta := NewFragmentMetadata(now, -time.Second, time.Minute)

	later := now.Add(365 * 24 * time.Hour)
	if state := meta.State(later); state != FragmentFresh {
		t.Fatalf("State(negative ttl) = %v, want FragmentFresh", state)
	}
	if meta.ShouldRevalidate(later) {
		t.Fatal("ShouldRevalidate(negative ttl) = true, want false")
	}
}

func TestPlanFragmentTagInvalidation(t *testing.T) {
	plan, err := PlanFragmentTagInvalidation([]FragmentIndexEntry{
		{Key: "fragment|shop|hero|v1|product|a", Tags: []string{"product", "detail"}},
		{Key: "fragment|shop|cart|v1|cart|b", Tags: []string{"cart"}},
		{Key: "fragment|shop|hero|v1|product|a", Tags: []string{"Product"}},
		{Key: "fragment|shop|badge|v1|product|c", Tags: []string{" Product "}},
		{Key: " ", Tags: []string{"product"}},
	}, []string{"DETAIL", "product", "product"})
	if err != nil {
		t.Fatalf("PlanFragmentTagInvalidation() error = %v", err)
	}

	want := FragmentInvalidationPlan{
		Keys: []string{
			"fragment|shop|hero|v1|product|a",
			"fragment|shop|badge|v1|product|c",
		},
		Tags: []string{"detail", "product"},
	}
	if !reflect.DeepEqual(plan, want) {
		t.Fatalf("PlanFragmentTagInvalidation() = %#v, want %#v", plan, want)
	}
}

func TestPlanFragmentTagInvalidationValidatesLabels(t *testing.T) {
	if _, err := PlanFragmentTagInvalidation(nil, []string{"!!!"}); !errors.Is(err, ErrFragmentTagInvalid) {
		t.Fatalf("PlanFragmentTagInvalidation(invalid label) error = %v, want %v", err, ErrFragmentTagInvalid)
	}
	if _, err := PlanFragmentTagInvalidation([]FragmentIndexEntry{{Key: "k", Tags: []string{"!!!"}}}, []string{"tag"}); !errors.Is(err, ErrFragmentTagInvalid) {
		t.Fatalf("PlanFragmentTagInvalidation(invalid entry tag) error = %v, want %v", err, ErrFragmentTagInvalid)
	}
}
