package search

import (
	"reflect"
	"testing"
)

func TestNormalizeTerms(t *testing.T) {
	got := NormalizeTerms([]string{"  Beach", "pool deck", "BEACH", "\tCafé  POOL\t"})
	want := []string{"beach", "pool deck", "café pool"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("NormalizeTerms() = %#v, want %#v", got, want)
	}
}

func TestPlanSynonymExpansion(t *testing.T) {
	got := PlanSynonymExpansion(
		[]string{"Home", "villa"},
		map[string][]string{
			"HOME":  {"House", "villa", "residence", "house"},
			"villa": {"casa"},
		},
	)
	want := SynonymExpansionPlan{
		Terms: []string{"home", "villa", "house", "residence", "casa"},
		Expansions: []SynonymExpansion{
			{Term: "home", Synonyms: []string{"house", "residence"}},
			{Term: "villa", Synonyms: []string{"casa"}},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("PlanSynonymExpansion() = %#v, want %#v", got, want)
	}
}

func TestHighlightTermsEscapesHTML(t *testing.T) {
	got := HighlightTerms(`<script>Pool & spa</script>`, []string{"pool", "spa"}, HighlightOptions{})
	want := `&lt;script&gt;<mark>Pool</mark> &amp; <mark>spa</mark>&lt;/script&gt;`
	if got != want {
		t.Fatalf("HighlightTerms() = %q, want %q", got, want)
	}
}

func TestHighlightTermsUnicodeAndCaseInsensitive(t *testing.T) {
	got := HighlightTerms("Casa com CAFÉ e varanda", []string{"café", "CASA"}, HighlightOptions{
		PreTag:  "<strong>",
		PostTag: "</strong>",
	})
	want := "<strong>Casa</strong> com <strong>CAFÉ</strong> e varanda"
	if got != want {
		t.Fatalf("HighlightTerms() = %q, want %q", got, want)
	}
}

func TestHighlightTermsPrefersLongestOverlap(t *testing.T) {
	got := HighlightTerms("pool house", []string{"pool", "pool house"}, HighlightOptions{})
	want := "<mark>pool house</mark>"
	if got != want {
		t.Fatalf("HighlightTerms() = %q, want %q", got, want)
	}
}

func TestHighlightTermsFragmentLimit(t *testing.T) {
	got := HighlightTerms("alpha beta gamma delta epsilon", []string{"gamma"}, HighlightOptions{
		FragmentLimit: 16,
	})
	want := "...beta <mark>gamma</mark> delt..."
	if got != want {
		t.Fatalf("HighlightTerms() = %q, want %q", got, want)
	}
}

func TestHighlightTermsFragmentWithoutMatchEscapesPrefix(t *testing.T) {
	got := HighlightTerms("<alpha> beta gamma", []string{"delta"}, HighlightOptions{
		FragmentLimit: 7,
	})
	want := "&lt;alpha&gt;..."
	if got != want {
		t.Fatalf("HighlightTerms() = %q, want %q", got, want)
	}
}
