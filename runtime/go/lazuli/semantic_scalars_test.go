package lazuli

import (
	"encoding/json"
	"testing"
)

func TestHexColorUnmarshalJSON(t *testing.T) {
	cases := []struct {
		in    string
		valid bool
	}{
		{`"#ffffff"`, true},
		{`"#FFF"`, true},
		{`"#0a1B2c"`, true},
		{`"#fff"`, true},
		{`"ffffff"`, false},   // missing leading #
		{`"#ggg"`, false},     // non-hex digits
		{`"#ffff"`, false},    // 4 digits — neither 3 nor 6
		{`"#fffffff"`, false}, // 7 digits
		{`""`, false},
	}
	for _, c := range cases {
		var got HexColor
		err := json.Unmarshal([]byte(c.in), &got)
		if c.valid && err != nil {
			t.Errorf("HexColor %s: unexpected error %v", c.in, err)
		}
		if !c.valid && err == nil {
			t.Errorf("HexColor %s: expected validation error, got nil", c.in)
		}
	}
}

func TestPercentageUnmarshalJSON(t *testing.T) {
	cases := []struct {
		in    string
		valid bool
	}{
		{`0`, true},
		{`100`, true},
		{`42.5`, true},
		{`-0.1`, false},
		{`100.1`, false},
		{`250`, false},
	}
	for _, c := range cases {
		var got Percentage
		err := json.Unmarshal([]byte(c.in), &got)
		if c.valid && err != nil {
			t.Errorf("Percentage %s: unexpected error %v", c.in, err)
		}
		if !c.valid && err == nil {
			t.Errorf("Percentage %s: expected validation error, got nil", c.in)
		}
	}
}

func TestHexColorRoundTrip(t *testing.T) {
	original := HexColor("#abcdef")
	raw, err := json.Marshal(original)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var back HexColor
	if err := json.Unmarshal(raw, &back); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if back != original {
		t.Fatalf("round-trip = %q, want %q", back, original)
	}
}

func TestPercentageRoundTrip(t *testing.T) {
	original := Percentage(33.3)
	raw, err := json.Marshal(original)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var back Percentage
	if err := json.Unmarshal(raw, &back); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if back != original {
		t.Fatalf("round-trip = %v, want %v", back, original)
	}
}
