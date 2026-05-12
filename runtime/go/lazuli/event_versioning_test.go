package lazuli

import (
	"context"
	"errors"
	"reflect"
	"testing"
)

func TestParseEventVersion(t *testing.T) {
	tests := []struct {
		input string
		want  EventVersion
	}{
		{
			input: "1.2.3",
			want:  EventVersion{Major: 1, Minor: 2, Patch: 3},
		},
		{
			input: " v2.0.0-alpha.1+build.5 ",
			want:  EventVersion{Major: 2, Minor: 0, Patch: 0, PreRelease: "alpha.1", Build: "build.5"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got, err := ParseEventVersion(tt.input)
			if err != nil {
				t.Fatalf("ParseEventVersion(%q) error = %v", tt.input, err)
			}
			if got != tt.want {
				t.Fatalf("ParseEventVersion(%q) = %#v, want %#v", tt.input, got, tt.want)
			}
			if roundTrip := MustParseEventVersion(got.String()); roundTrip != got {
				t.Fatalf("MustParseEventVersion(%q) = %#v, want %#v", got.String(), roundTrip, got)
			}
		})
	}
}

func TestParseEventVersionRejectsInvalidVersions(t *testing.T) {
	tests := []string{
		"",
		"1",
		"1.2",
		"1.2.3.4",
		"01.2.3",
		"1.02.3",
		"1.2.03",
		"1.2.x",
		"1.2.3-",
		"1.2.3-alpha..1",
		"1.2.3-01",
		"1.2.3+",
		"1.2.3-é",
	}

	for _, input := range tests {
		t.Run(input, func(t *testing.T) {
			_, err := ParseEventVersion(input)
			if !errors.Is(err, ErrInvalidEventVersion) {
				t.Fatalf("ParseEventVersion(%q) error = %v, want ErrInvalidEventVersion", input, err)
			}
		})
	}
}

func TestEventVersionCompare(t *testing.T) {
	ordered := []string{
		"1.0.0-alpha",
		"1.0.0-alpha.1",
		"1.0.0-alpha.beta",
		"1.0.0-beta",
		"1.0.0-beta.2",
		"1.0.0-beta.11",
		"1.0.0-rc.1",
		"1.0.0",
		"1.0.1",
		"1.1.0",
		"2.0.0",
	}

	for i := 0; i < len(ordered)-1; i++ {
		left := MustParseEventVersion(ordered[i])
		right := MustParseEventVersion(ordered[i+1])
		if got := left.Compare(right); got != -1 {
			t.Fatalf("%s.Compare(%s) = %d, want -1", left, right, got)
		}
		if got := right.Compare(left); got != 1 {
			t.Fatalf("%s.Compare(%s) = %d, want 1", right, left, got)
		}
	}

	if got := MustParseEventVersion("1.0.0+build.1").Compare(MustParseEventVersion("1.0.0+build.2")); got != 0 {
		t.Fatalf("Compare() with build metadata = %d, want 0", got)
	}
	if got := CompareEventVersions(MustParseEventVersion("1.0.0"), MustParseEventVersion("1.0.0")); got != 0 {
		t.Fatalf("CompareEventVersions() = %d, want 0", got)
	}
}

func TestEventUpcasterRegistryAppliesChain(t *testing.T) {
	registry := NewEventUpcasterRegistry()
	one := MustParseEventVersion("1.0.0")
	oneOne := MustParseEventVersion("1.1.0")
	two := MustParseEventVersion("2.0.0")
	steps := []string{}

	err := registry.Register("customer.updated", one, oneOne, func(_ context.Context, event Event) (Event, error) {
		steps = append(steps, "1.1.0")
		event.Payload["full_name"] = event.Payload["first_name"].(string) + " " + event.Payload["last_name"].(string)
		return event, nil
	})
	if err != nil {
		t.Fatalf("Register() first error = %v", err)
	}
	err = registry.Register("customer.updated", oneOne, two, func(_ context.Context, event Event) (Event, error) {
		steps = append(steps, "2.0.0")
		event.Payload["customer_id"] = event.Payload["id"]
		delete(event.Payload, "id")
		return event, nil
	})
	if err != nil {
		t.Fatalf("Register() second error = %v", err)
	}

	original := Event{
		Name: "customer.updated",
		Payload: map[string]any{
			"id":         ID(42),
			"first_name": "Ada",
			"last_name":  "Lovelace",
		},
	}
	upcasted, err := registry.Upcast(context.Background(), original, one, two)
	if err != nil {
		t.Fatalf("Upcast() error = %v", err)
	}

	if !reflect.DeepEqual(steps, []string{"1.1.0", "2.0.0"}) {
		t.Fatalf("upcast steps = %v, want [1.1.0 2.0.0]", steps)
	}
	wantPayload := map[string]any{
		"customer_id": ID(42),
		"first_name":  "Ada",
		"last_name":   "Lovelace",
		"full_name":   "Ada Lovelace",
	}
	if !reflect.DeepEqual(upcasted.Payload, wantPayload) {
		t.Fatalf("upcasted payload = %#v, want %#v", upcasted.Payload, wantPayload)
	}
	if _, ok := original.Payload["full_name"]; ok {
		t.Fatalf("original payload was mutated: %#v", original.Payload)
	}
	if _, ok := original.Payload["id"]; !ok {
		t.Fatalf("original payload lost id: %#v", original.Payload)
	}
}

func TestEventUpcasterRegistryNoopReturnsCopy(t *testing.T) {
	registry := NewEventUpcasterRegistry()
	version := MustParseEventVersion("1.0.0")
	original := Event{
		Name:    "customer.updated",
		Payload: map[string]any{"meta": map[string]any{"tier": "gold"}},
	}

	got, err := registry.Upcast(context.Background(), original, version, version)
	if err != nil {
		t.Fatalf("Upcast() noop error = %v", err)
	}
	got.Payload["meta"].(map[string]any)["tier"] = "silver"

	if original.Payload["meta"].(map[string]any)["tier"] != "gold" {
		t.Fatalf("original payload was mutated: %#v", original.Payload)
	}
}

func TestEventUpcasterRegistryFindsCompletePath(t *testing.T) {
	registry := NewEventUpcasterRegistry()
	one := MustParseEventVersion("1.0.0")
	oneOne := MustParseEventVersion("1.1.0")
	two := MustParseEventVersion("2.0.0")

	if err := registry.Register("event", one, oneOne, func(context.Context, Event) (Event, error) {
		t.Fatal("incomplete intermediate path should not be applied")
		return Event{}, nil
	}); err != nil {
		t.Fatalf("Register() intermediate error = %v", err)
	}
	if err := registry.Register("event", one, two, func(_ context.Context, event Event) (Event, error) {
		event.Payload["version"] = "2.0.0"
		return event, nil
	}); err != nil {
		t.Fatalf("Register() direct error = %v", err)
	}

	got, err := registry.Upcast(context.Background(), Event{
		Name:    "event",
		Payload: map[string]any{},
	}, one, two)
	if err != nil {
		t.Fatalf("Upcast() error = %v", err)
	}
	if got.Payload["version"] != "2.0.0" {
		t.Fatalf("upcasted payload = %#v, want version 2.0.0", got.Payload)
	}
}

func TestEventUpcasterRegistryRejectsInvalidRegistration(t *testing.T) {
	registry := NewEventUpcasterRegistry()
	one := MustParseEventVersion("1.0.0")
	two := MustParseEventVersion("2.0.0")
	upcaster := func(context.Context, Event) (Event, error) { return Event{}, nil }

	if err := registry.Register("", one, two, upcaster); !errors.Is(err, ErrEventUpcasterNameRequired) {
		t.Fatalf("Register() empty name error = %v, want ErrEventUpcasterNameRequired", err)
	}
	if err := registry.Register("event", one, two, nil); !errors.Is(err, ErrNilEventUpcaster) {
		t.Fatalf("Register() nil upcaster error = %v, want ErrNilEventUpcaster", err)
	}
	if err := registry.Register("event", two, one, upcaster); !errors.Is(err, ErrEventUpcasterVersionOrder) {
		t.Fatalf("Register() downgrade error = %v, want ErrEventUpcasterVersionOrder", err)
	}
	if err := registry.Register("event", one, two, upcaster); err != nil {
		t.Fatalf("Register() valid error = %v", err)
	}
	if err := registry.Register("event", one, two, upcaster); !errors.Is(err, ErrEventUpcasterDuplicate) {
		t.Fatalf("Register() duplicate error = %v, want ErrEventUpcasterDuplicate", err)
	}
}

func TestEventUpcasterRegistryReportsMissingPathAndDowngrade(t *testing.T) {
	registry := NewEventUpcasterRegistry()
	one := MustParseEventVersion("1.0.0")
	two := MustParseEventVersion("2.0.0")

	if _, err := registry.Upcast(context.Background(), Event{Name: "event"}, one, two); !errors.Is(err, ErrEventUpcasterPathMissing) {
		t.Fatalf("Upcast() missing path error = %v, want ErrEventUpcasterPathMissing", err)
	}
	if _, err := registry.Upcast(context.Background(), Event{Name: "event"}, two, one); !errors.Is(err, ErrEventVersionDowngrade) {
		t.Fatalf("Upcast() downgrade error = %v, want ErrEventVersionDowngrade", err)
	}
}
