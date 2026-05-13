package views

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestNewLayoutManifestNormalizesOrdersAndCopies(t *testing.T) {
	layouts := []Layout{
		{Name: "app", Slots: []string{"head", " content "}},
		{Name: "admin.shell", Slots: []string{"title", "body"}},
	}
	partials := []Partial{
		{Name: "card", Dependencies: []string{"currency", " avatar "}},
		{Name: "currency", Dependencies: []string{"format"}},
		{Name: "format"},
		{Name: "avatar"},
	}

	manifest, err := NewLayoutManifest(layouts, partials)
	if err != nil {
		t.Fatalf("NewLayoutManifest() error = %v", err)
	}

	if got := manifest.LayoutNames(); !reflect.DeepEqual(got, []string{"admin.shell", "app"}) {
		t.Fatalf("LayoutNames() = %#v, want sorted layout names", got)
	}
	if got := manifest.Layouts[1].Slots; !reflect.DeepEqual(got, []string{"head", "content"}) {
		t.Fatalf("manifest slots = %#v, want normalized slots", got)
	}
	if got := partialNames(manifest.Partials); !reflect.DeepEqual(got, []string{"avatar", "format", "currency", "card"}) {
		t.Fatalf("manifest partial order = %#v, want dependency-first order", got)
	}
	if got := manifest.Partials[3].Dependencies; !reflect.DeepEqual(got, []string{"avatar", "currency"}) {
		t.Fatalf("card dependencies = %#v, want sorted normalized dependencies", got)
	}

	layouts[0].Slots[0] = "changed"
	partials[0].Dependencies[0] = "changed"
	if manifest.Layouts[1].Slots[0] != "head" {
		t.Fatal("NewLayoutManifest() did not copy layout slots")
	}
	if manifest.Partials[3].Dependencies[1] != "currency" {
		t.Fatal("NewLayoutManifest() did not copy partial dependencies")
	}
}

func TestLayoutManifestLookupHelpersReturnCopies(t *testing.T) {
	manifest, err := NewLayoutManifest(
		[]Layout{
			{Name: "app", Slots: []string{"head", "content"}},
		},
		[]Partial{
			{Name: "card", Dependencies: []string{"avatar"}},
			{Name: "avatar"},
		},
	)
	if err != nil {
		t.Fatalf("NewLayoutManifest() error = %v", err)
	}

	slots, ok := manifest.Slots(" app ")
	if !ok {
		t.Fatal("Slots() did not find app layout")
	}
	if !reflect.DeepEqual(slots, []string{"head", "content"}) {
		t.Fatalf("Slots() = %#v, want app slots", slots)
	}
	slots[0] = "changed"
	if manifest.Layouts[0].Slots[0] != "head" {
		t.Fatal("Slots() returned manifest backing slice")
	}

	dependencies, ok := manifest.PartialDependencies(" card ")
	if !ok {
		t.Fatal("PartialDependencies() did not find card partial")
	}
	if !reflect.DeepEqual(dependencies, []string{"avatar"}) {
		t.Fatalf("PartialDependencies() = %#v, want card dependencies", dependencies)
	}
	dependencies[0] = "changed"
	dependencies, _ = manifest.PartialDependencies("card")
	if !reflect.DeepEqual(dependencies, []string{"avatar"}) {
		t.Fatalf("PartialDependencies() returned manifest backing slice: %#v", dependencies)
	}

	if _, ok := manifest.Slots("missing"); ok {
		t.Fatal("Slots() found a missing layout")
	}
	if _, ok := manifest.PartialDependencies("missing"); ok {
		t.Fatal("PartialDependencies() found a missing partial")
	}
}

func TestTopologicalPartialsOrdersDependenciesFirst(t *testing.T) {
	partials := []Partial{
		{Name: "page", Dependencies: []string{"toolbar", "card"}},
		{Name: "toolbar", Dependencies: []string{"icon"}},
		{Name: "card", Dependencies: []string{"avatar"}},
		{Name: "icon"},
		{Name: "avatar"},
	}

	ordered, err := TopologicalPartials(partials)
	if err != nil {
		t.Fatalf("TopologicalPartials() error = %v", err)
	}

	if got := partialNames(ordered); !reflect.DeepEqual(got, []string{"avatar", "card", "icon", "toolbar", "page"}) {
		t.Fatalf("TopologicalPartials() names = %#v, want dependency-first order", got)
	}
	if partials[0].Dependencies[0] != "toolbar" {
		t.Fatal("TopologicalPartials() mutated input dependencies")
	}
}

func TestLayoutManifestPartialOrder(t *testing.T) {
	manifest := LayoutManifest{
		Partials: []Partial{
			{Name: "page", Dependencies: []string{"card"}},
			{Name: "card"},
		},
	}

	order, err := manifest.PartialOrder()
	if err != nil {
		t.Fatalf("PartialOrder() error = %v", err)
	}
	if !reflect.DeepEqual(order, []string{"card", "page"}) {
		t.Fatalf("PartialOrder() = %#v, want dependency-first names", order)
	}
}

func TestNewLayoutManifestRejectsDuplicateMetadata(t *testing.T) {
	tests := []struct {
		name     string
		layouts  []Layout
		partials []Partial
		wantErr  error
	}{
		{
			name: "duplicate layout",
			layouts: []Layout{
				{Name: "app"},
				{Name: " app "},
			},
			wantErr: ErrDuplicateLayout,
		},
		{
			name: "duplicate layout slot",
			layouts: []Layout{
				{Name: "app", Slots: []string{"content", " content "}},
			},
			wantErr: ErrDuplicateLayoutSlot,
		},
		{
			name: "duplicate partial",
			partials: []Partial{
				{Name: "nav"},
				{Name: " nav "},
			},
			wantErr: ErrDuplicatePartial,
		},
		{
			name: "duplicate partial dependency",
			partials: []Partial{
				{Name: "card", Dependencies: []string{"avatar", " avatar "}},
				{Name: "avatar"},
			},
			wantErr: ErrDuplicatePartialDependency,
		},
		{
			name: "unknown partial dependency",
			partials: []Partial{
				{Name: "card", Dependencies: []string{"avatar"}},
			},
			wantErr: ErrUnknownPartialDependency,
		},
		{
			name: "partial dependency cycle",
			partials: []Partial{
				{Name: "a", Dependencies: []string{"b"}},
				{Name: "b", Dependencies: []string{"a"}},
			},
			wantErr: ErrPartialDependencyCycle,
		},
		{
			name: "invalid names",
			layouts: []Layout{
				{Name: "bad layout", Slots: []string{""}},
			},
			partials: []Partial{
				{Name: "bad partial"},
			},
			wantErr: ErrInvalidLayoutManifest,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewLayoutManifest(tt.layouts, tt.partials)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("NewLayoutManifest() error = %v, want %v", err, tt.wantErr)
			}
			if !errors.Is(err, ErrInvalidLayoutManifest) {
				t.Fatalf("NewLayoutManifest() error = %v, want ErrInvalidLayoutManifest", err)
			}
		})
	}
}

func TestPartialDependencyCycleReportsPath(t *testing.T) {
	_, err := TopologicalPartials([]Partial{
		{Name: "a", Dependencies: []string{"b"}},
		{Name: "b", Dependencies: []string{"c"}},
		{Name: "c", Dependencies: []string{"a"}},
	})
	if !errors.Is(err, ErrPartialDependencyCycle) {
		t.Fatalf("TopologicalPartials() error = %v, want ErrPartialDependencyCycle", err)
	}
	if !strings.Contains(err.Error(), "a -> b -> c -> a") {
		t.Fatalf("TopologicalPartials() error = %q, want cycle path", err.Error())
	}
}

func TestLayoutManifestValidateDoesNotMutate(t *testing.T) {
	manifest := LayoutManifest{
		Layouts: []Layout{
			{Name: " app ", Slots: []string{" content "}},
		},
		Partials: []Partial{
			{Name: "card", Dependencies: []string{" avatar "}},
			{Name: "avatar"},
		},
	}

	if err := manifest.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if manifest.Layouts[0].Name != " app " || manifest.Layouts[0].Slots[0] != " content " {
		t.Fatalf("Validate() mutated layout metadata: %#v", manifest.Layouts[0])
	}
	if manifest.Partials[0].Dependencies[0] != " avatar " {
		t.Fatalf("Validate() mutated partial metadata: %#v", manifest.Partials[0])
	}
}

func partialNames(partials []Partial) []string {
	names := make([]string, 0, len(partials))
	for _, partial := range partials {
		names = append(names, partial.Name)
	}
	return names
}
