package storage_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestBuildLifecyclePlanOutputsDryRunTransitions(t *testing.T) {
	t.Parallel()

	const day = 24 * time.Hour
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	policy := storage.LifecyclePolicy{Rules: []storage.LifecycleRule{
		storage.ArchiveAfter(30*day, storage.VisibilityPublic).Named("public-archive"),
		storage.DeleteAfter(90*day, storage.VisibilityPrivate).Named("private-delete"),
		storage.RetainAfter(0, storage.VisibilitySigned).Named("signed-retain"),
	}}
	objects := []storage.LifecycleObject{
		{
			Key:        "avatars/alice.png",
			CreatedAt:  now.Add(-45 * day),
			Visibility: storage.VisibilityPublic,
		},
		{
			Key:          "imports/customers.csv",
			LastModified: now.Add(-100 * day),
			Visibility:   storage.VisibilityPrivate,
		},
		{
			Key:        "avatars/bob.png",
			CreatedAt:  now.Add(-10 * day),
			Visibility: storage.VisibilityPublic,
		},
		{
			Key:        "exports/report.zip",
			CreatedAt:  now.Add(-200 * day),
			Visibility: storage.VisibilitySigned,
		},
	}

	plan, err := storage.BuildLifecyclePlan(policy, objects, now)
	if err != nil {
		t.Fatalf("BuildLifecyclePlan() error = %v", err)
	}
	if !plan.DryRun {
		t.Fatal("plan DryRun = false, want true")
	}
	if !plan.GeneratedAt.Equal(now) {
		t.Fatalf("GeneratedAt = %s, want %s", plan.GeneratedAt, now)
	}
	if len(plan.Entries) != len(objects) {
		t.Fatalf("entries len = %d, want %d", len(plan.Entries), len(objects))
	}

	wantTransitions := []storage.LifecycleTransition{
		storage.LifecycleArchive,
		storage.LifecycleDelete,
		storage.LifecycleRetain,
		storage.LifecycleRetain,
	}
	for i, want := range wantTransitions {
		if got := plan.Entries[i].Transition; got != want {
			t.Fatalf("entry %d transition = %s, want %s", i, got, want)
		}
	}
	if got := plan.Entries[0].RuleName; got != "public-archive" {
		t.Fatalf("archive rule = %q, want public-archive", got)
	}
	if got := plan.Entries[1].RuleName; got != "private-delete" {
		t.Fatalf("delete rule = %q, want private-delete", got)
	}
	if got := plan.Entries[2].RuleName; got != "public-archive" {
		t.Fatalf("retained pending rule = %q, want public-archive", got)
	}
	if want := now.Add(20 * day); !plan.Entries[2].EligibleAt.Equal(want) {
		t.Fatalf("pending EligibleAt = %s, want %s", plan.Entries[2].EligibleAt, want)
	}
	if got := plan.Entries[3].RuleName; got != "signed-retain" {
		t.Fatalf("explicit retain rule = %q, want signed-retain", got)
	}
}

func TestLifecyclePredicates(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	object := storage.LifecycleObject{
		Key:        "exports/report.zip",
		CreatedAt:  now.Add(-2 * time.Hour),
		Visibility: storage.VisibilitySigned,
	}

	if age := storage.LifecycleAgeAt(object, now); age != 2*time.Hour {
		t.Fatalf("LifecycleAgeAt() = %s, want 2h", age)
	}
	if !storage.LifecycleAgeAtLeast(object, now, time.Hour) {
		t.Fatal("LifecycleAgeAtLeast() rejected object older than 1h")
	}
	if storage.LifecycleAgeAtLeast(object, now, 3*time.Hour) {
		t.Fatal("LifecycleAgeAtLeast() accepted object younger than 3h")
	}
	if !storage.LifecycleVisibilityMatches(object) {
		t.Fatal("empty visibility predicate did not match")
	}
	if !storage.LifecycleVisibilityMatches(object, storage.VisibilitySigned) {
		t.Fatal("signed visibility predicate did not match")
	}
	if storage.LifecycleVisibilityMatches(object, storage.VisibilityPrivate, storage.VisibilityPublic) {
		t.Fatal("private/public visibility predicate matched signed object")
	}
	object.Visibility = storage.FileVisibility(99)
	if storage.LifecycleVisibilityMatches(object) {
		t.Fatal("empty visibility predicate matched unknown object visibility")
	}
}

func TestValidateLifecyclePolicyRejectsInvalidRules(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name   string
		policy storage.LifecyclePolicy
	}{
		{
			name: "unknown transition",
			policy: storage.LifecyclePolicy{Rules: []storage.LifecycleRule{
				{Transition: storage.LifecycleTransition(99)},
			}},
		},
		{
			name: "negative age",
			policy: storage.LifecyclePolicy{Rules: []storage.LifecycleRule{
				storage.DeleteAfter(-time.Second, storage.VisibilityPrivate),
			}},
		},
		{
			name: "unknown visibility",
			policy: storage.LifecyclePolicy{Rules: []storage.LifecycleRule{
				storage.ArchiveAfter(time.Hour, storage.FileVisibility(99)),
			}},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			err := storage.ValidateLifecyclePolicy(tc.policy)
			if !errors.Is(err, storage.ErrLifecyclePolicyInvalid) {
				t.Fatalf("ValidateLifecyclePolicy() error = %v, want ErrLifecyclePolicyInvalid", err)
			}
		})
	}
}

func TestBuildLifecyclePlanValidatesObjects(t *testing.T) {
	t.Parallel()

	policy := storage.LifecyclePolicy{Rules: []storage.LifecycleRule{
		storage.DeleteAfter(0, storage.VisibilityPrivate),
	}}
	_, err := storage.BuildLifecyclePlan(policy, []storage.LifecycleObject{
		{Visibility: storage.VisibilityPrivate},
	}, time.Now())
	if !errors.Is(err, storage.ErrLifecycleObjectInvalid) {
		t.Fatalf("empty key error = %v, want ErrLifecycleObjectInvalid", err)
	}

	_, err = storage.BuildLifecyclePlan(policy, []storage.LifecycleObject{
		{Key: "objects/one", Visibility: storage.FileVisibility(99)},
	}, time.Now())
	if !errors.Is(err, storage.ErrLifecycleObjectInvalid) {
		t.Fatalf("unknown visibility error = %v, want ErrLifecycleObjectInvalid", err)
	}
}

func TestLifecycleTransitionString(t *testing.T) {
	t.Parallel()

	cases := map[storage.LifecycleTransition]string{
		storage.LifecycleRetain:         "retain",
		storage.LifecycleDelete:         "delete",
		storage.LifecycleArchive:        "archive",
		storage.LifecycleTransition(99): "unknown",
	}
	for transition, want := range cases {
		if got := transition.String(); got != want {
			t.Fatalf("LifecycleTransition(%d).String() = %q, want %q", transition, got, want)
		}
	}
}
