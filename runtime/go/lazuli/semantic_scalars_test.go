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

func TestPositiveDecimalUnmarshalJSON(t *testing.T) {
	cases := []struct {
		in    string
		valid bool
	}{
		{`0.01`, true},
		{`1`, true},
		{`9999.99`, true},
		{`0`, false},    // zero is not strictly positive
		{`-0.01`, false}, // negatives rejected
		{`-5`, false},
	}
	for _, c := range cases {
		var got PositiveDecimal
		err := json.Unmarshal([]byte(c.in), &got)
		if c.valid && err != nil {
			t.Errorf("PositiveDecimal %s: unexpected error %v", c.in, err)
		}
		if !c.valid && err == nil {
			t.Errorf("PositiveDecimal %s: expected validation error, got nil", c.in)
		}
	}
}

func TestNonNegativeIntUnmarshalJSON(t *testing.T) {
	cases := []struct {
		in    string
		valid bool
	}{
		{`0`, true}, // zero is admissible (>= 0)
		{`1`, true},
		{`1000000`, true},
		{`-1`, false}, // negatives rejected
		{`-42`, false},
	}
	for _, c := range cases {
		var got NonNegativeInt
		err := json.Unmarshal([]byte(c.in), &got)
		if c.valid && err != nil {
			t.Errorf("NonNegativeInt %s: unexpected error %v", c.in, err)
		}
		if !c.valid && err == nil {
			t.Errorf("NonNegativeInt %s: expected validation error, got nil", c.in)
		}
	}
}

func TestPositiveDecimalRoundTrip(t *testing.T) {
	original := PositiveDecimal(12.34)
	raw, err := json.Marshal(original)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var back PositiveDecimal
	if err := json.Unmarshal(raw, &back); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if back != original {
		t.Fatalf("round-trip = %v, want %v", back, original)
	}
}

func TestNonNegativeIntRoundTrip(t *testing.T) {
	original := NonNegativeInt(7)
	raw, err := json.Marshal(original)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var back NonNegativeInt
	if err := json.Unmarshal(raw, &back); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if back != original {
		t.Fatalf("round-trip = %v, want %v", back, original)
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
