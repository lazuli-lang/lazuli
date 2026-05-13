package lazuli

import "testing"

func TestParseAcceptParsesQualityWildcardsAndParameters(t *testing.T) {
	got := ParseAccept(`application/json, text/html;level=1;q=0.8, text/*;q=0.7, */*;q=0`)

	if len(got) != 4 {
		t.Fatalf("ParseAccept len = %d, want 4: %#v", len(got), got)
	}
	if got[0].MediaType != "application/json" || got[0].Type != "application" || got[0].Subtype != "json" || got[0].Quality != 1 {
		t.Fatalf("first range = %#v, want application/json q=1", got[0])
	}
	if got[1].MediaType != "text/html" || got[1].Params["level"] != "1" || got[1].Quality != 0.8 {
		t.Fatalf("second range = %#v, want text/html;level=1 q=0.8", got[1])
	}
	if got[2].MediaType != "text/*" || got[2].Quality != 0.7 {
		t.Fatalf("third range = %#v, want text/* q=0.7", got[2])
	}
	if got[3].MediaType != "*/*" || got[3].Quality != 0 {
		t.Fatalf("fourth range = %#v, want */* q=0", got[3])
	}
}

func TestParseAcceptHandlesQuotedCommas(t *testing.T) {
	got := ParseAccept(`application/vnd.example+json;profile="a,b";q=0.5, text/plain`)

	if len(got) != 2 {
		t.Fatalf("ParseAccept len = %d, want 2: %#v", len(got), got)
	}
	if got[0].MediaType != "application/vnd.example+json" || got[0].Params["profile"] != "a,b" || got[0].Quality != 0.5 {
		t.Fatalf("first range = %#v, want vendor json profile q=0.5", got[0])
	}
	if got[1].MediaType != "text/plain" || got[1].Quality != 1 {
		t.Fatalf("second range = %#v, want text/plain q=1", got[1])
	}
}

func TestParseAcceptSkipsInvalidRanges(t *testing.T) {
	got := ParseAccept(`broken, */json, text/plain;q=1.5, application/json;q=0.25`)

	if len(got) != 1 {
		t.Fatalf("ParseAccept len = %d, want 1: %#v", len(got), got)
	}
	if got[0].MediaType != "application/json" || got[0].Quality != 0.25 {
		t.Fatalf("range = %#v, want application/json q=0.25", got[0])
	}
}

func TestNegotiateContentTypeUsesQualityAndWildcards(t *testing.T) {
	got, ok := NegotiateContentType(
		`text/*;q=0.4, application/json;q=0.8, */*;q=0.1`,
		"text/html",
		"application/json",
	)

	if !ok {
		t.Fatal("NegotiateContentType ok = false, want true")
	}
	if got != "application/json" {
		t.Fatalf("NegotiateContentType = %q, want application/json", got)
	}
}

func TestBestMediaTypeUsesSpecificRangeBeforeWildcard(t *testing.T) {
	ranges := ParseAccept(`application/json;q=0, application/*;q=1, */*;q=0.5`)

	got, ok := BestMediaType(ranges, "application/json", "text/plain")

	if !ok {
		t.Fatal("BestMediaType ok = false, want true")
	}
	if got != "text/plain" {
		t.Fatalf("BestMediaType = %q, want text/plain", got)
	}
}

func TestBestMediaTypeUsesParameterSpecificity(t *testing.T) {
	ranges := ParseAccept(`text/html;level=1;q=1, text/html;q=0.5`)

	got, ok := BestMediaType(ranges, "text/html", "text/html;level=1")

	if !ok {
		t.Fatal("BestMediaType ok = false, want true")
	}
	if got != "text/html;level=1" {
		t.Fatalf("BestMediaType = %q, want text/html;level=1", got)
	}
}

func TestBestMediaTypeTieBreaksByAcceptOrderThenOfferOrder(t *testing.T) {
	got, ok := NegotiateContentType(
		`text/html;q=0.8, application/json;q=0.8`,
		"application/json",
		"text/html",
	)

	if !ok {
		t.Fatal("NegotiateContentType ok = false, want true")
	}
	if got != "text/html" {
		t.Fatalf("NegotiateContentType = %q, want text/html", got)
	}

	got, ok = NegotiateContentType(`*/*;q=0.5`, "application/json", "text/html")
	if !ok {
		t.Fatal("NegotiateContentType wildcard ok = false, want true")
	}
	if got != "application/json" {
		t.Fatalf("NegotiateContentType wildcard = %q, want application/json", got)
	}
}

func TestNegotiateContentTypeTreatsEmptyAcceptAsAny(t *testing.T) {
	got, ok := NegotiateContentType("", "application/json", "text/html")

	if !ok {
		t.Fatal("NegotiateContentType ok = false, want true")
	}
	if got != "application/json" {
		t.Fatalf("NegotiateContentType = %q, want application/json", got)
	}
}

func TestBestMediaTypeRejectsOnlyZeroQualityMatches(t *testing.T) {
	got, ok := NegotiateContentType(`application/json;q=0, text/html;q=0`, "application/json", "text/html")

	if ok {
		t.Fatalf("NegotiateContentType ok = true, want false with %q", got)
	}
	if got != "" {
		t.Fatalf("NegotiateContentType = %q, want empty", got)
	}
}
