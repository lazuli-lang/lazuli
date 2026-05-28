package lazuli

import (
	"database/sql/driver"
	"encoding/json"
	"fmt"
	"regexp"
)

// W1 GAP-04/05 — named runtime carriers for the `@semantic.HexColor` and
// `@semantic.Percentage` builtin scalars. Unlike the alias-based text
// semantics (`Email = string`, `URL = string`) which defer enforcement to
// the validator pipeline, these two carry their validation inline: the
// `UnmarshalJSON` hook runs at the decode boundary so a malformed value
// surfaces as a `validation_failed` envelope (see `typed_decode.go`)
// without any authored validator. Founding principle: each method is a
// thin call into stdlib (`regexp`, a numeric comparison), not homegrown
// parsing.

// hexColorRe matches a CSS-style hex colour literal: `#RRGGBB` (6 hex
// digits) or `#RGB` (3 hex digits), case-insensitive. Compiled once at
// package init. Mirror of the analyzer/codegen-ts regex so server and
// client agree on the accepted shape.
var hexColorRe = regexp.MustCompile(`^#([0-9a-fA-F]{6}|[0-9a-fA-F]{3})$`)

// HexColor is the carrier for `@semantic.HexColor`. The stored value is a
// validated hex colour string (`#RRGGBB` / `#RGB`). DDL column is TEXT.
type HexColor string

// Validate reports whether the colour matches `#RRGGBB` or `#RGB`.
func (c HexColor) Validate() error {
	if !hexColorRe.MatchString(string(c)) {
		return fmt.Errorf("lazuli: invalid hex colour %q (want #RRGGBB or #RGB)", string(c))
	}
	return nil
}

// String returns the raw colour literal.
func (c HexColor) String() string { return string(c) }

// Value implements driver.Valuer so HexColor binds directly as a TEXT
// column value.
func (c HexColor) Value() (driver.Value, error) { return string(c), nil }

// Scan implements sql.Scanner so pgx can hydrate HexColor from a TEXT
// column. Scan is trusting (DB is the source of truth); shape enforcement
// happens at the wire decode boundary via UnmarshalJSON.
func (c *HexColor) Scan(src any) error {
	switch v := src.(type) {
	case string:
		*c = HexColor(v)
	case []byte:
		*c = HexColor(v)
	case nil:
		*c = ""
	default:
		return fmt.Errorf("lazuli: cannot scan %T into HexColor", src)
	}
	return nil
}

// MarshalJSON emits the bare colour string.
func (c HexColor) MarshalJSON() ([]byte, error) { return json.Marshal(string(c)) }

// UnmarshalJSON decodes a JSON string and enforces the `#RRGGBB`/`#RGB`
// regex. A malformed literal returns an error that lifts to a 400
// validation_failed envelope through the command decode pipeline.
func (c *HexColor) UnmarshalJSON(data []byte) error {
	var s string
	if err := json.Unmarshal(data, &s); err != nil {
		return err
	}
	parsed := HexColor(s)
	if err := parsed.Validate(); err != nil {
		return err
	}
	*c = parsed
	return nil
}

// Percentage is the carrier for `@semantic.Percentage`. The stored value
// is a float64 bounded to 0 <= value <= 100. DDL column is NUMERIC(20, 6),
// matching Decimal's precision.
type Percentage float64

// Validate reports whether the value falls within the 0..=100 range.
func (p Percentage) Validate() error {
	if p < 0 || p > 100 {
		return fmt.Errorf("lazuli: percentage %v out of range (want 0 <= value <= 100)", float64(p))
	}
	return nil
}

// Value implements driver.Valuer so Percentage binds directly as a
// NUMERIC column value.
func (p Percentage) Value() (driver.Value, error) { return float64(p), nil }

// Scan implements sql.Scanner so pgx can hydrate Percentage from a
// NUMERIC column.
func (p *Percentage) Scan(src any) error {
	switch v := src.(type) {
	case float64:
		*p = Percentage(v)
	case int64:
		*p = Percentage(v)
	case nil:
		*p = 0
	default:
		return fmt.Errorf("lazuli: cannot scan %T into Percentage", src)
	}
	return nil
}

// MarshalJSON emits the bare number.
func (p Percentage) MarshalJSON() ([]byte, error) { return json.Marshal(float64(p)) }

// UnmarshalJSON decodes a JSON number and enforces the 0..=100 range. An
// out-of-range value returns an error that lifts to a 400
// validation_failed envelope through the command decode pipeline.
func (p *Percentage) UnmarshalJSON(data []byte) error {
	var f float64
	if err := json.Unmarshal(data, &f); err != nil {
		return err
	}
	parsed := Percentage(f)
	if err := parsed.Validate(); err != nil {
		return err
	}
	*p = parsed
	return nil
}
