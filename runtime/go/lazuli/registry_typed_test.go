package lazuli

import (
	"errors"
	"testing"
)

func TestRegistry_LookupQuery(t *testing.T) {
	r := &Registry{}
	r.RegisterQuery(queryRegistration{Name: "foo.bar", Feature: "foo"})

	got, err := r.LookupQuery("foo.bar")
	if err != nil || got == nil || got.Name != "foo.bar" || got.Feature != "foo" {
		t.Fatalf("LookupQuery: got %v, err %v", got, err)
	}

	_, err = r.LookupQuery("missing")
	if !errors.Is(err, ErrQueryNotFound) {
		t.Fatalf("LookupQuery missing: want ErrQueryNotFound, got %v", err)
	}
}
