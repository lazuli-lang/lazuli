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

// Batch E — strict-positive / non-negative numeric carriers. Pilots were
// hand-writing `> 0` (price/amount) and `>= 0` (count/quantity) validators;
// these carriers fold the bound into the decode boundary so an out-of-range
// value surfaces as a `validation_failed` envelope without any authored
// validator — same posture as `Percentage`'s 0..=100 guard.

// PositiveDecimal is the carrier for `@semantic.PositiveDecimal`. The stored
// value is a float64 constrained to value > 0 (strictly positive — zero and
// negatives are rejected). DDL column is NUMERIC(20, 6), matching Decimal's
// precision.
type PositiveDecimal float64

// Validate reports whether the value is strictly positive (> 0).
func (d PositiveDecimal) Validate() error {
	if d <= 0 {
		return fmt.Errorf("lazuli: value %v must be greater than 0", float64(d))
	}
	return nil
}

// Value implements driver.Valuer so PositiveDecimal binds directly as a
// NUMERIC column value.
func (d PositiveDecimal) Value() (driver.Value, error) { return float64(d), nil }

// Scan implements sql.Scanner so pgx can hydrate PositiveDecimal from a
// NUMERIC column. Scan is trusting (DB is the source of truth); shape
// enforcement happens at the wire decode boundary via UnmarshalJSON.
func (d *PositiveDecimal) Scan(src any) error {
	switch v := src.(type) {
	case float64:
		*d = PositiveDecimal(v)
	case int64:
		*d = PositiveDecimal(v)
	case nil:
		*d = 0
	default:
		return fmt.Errorf("lazuli: cannot scan %T into PositiveDecimal", src)
	}
	return nil
}

// MarshalJSON emits the bare number.
func (d PositiveDecimal) MarshalJSON() ([]byte, error) { return json.Marshal(float64(d)) }

// UnmarshalJSON decodes a JSON number and enforces value > 0. A zero or
// negative value returns an error that lifts to a 400 validation_failed
// envelope through the command decode pipeline.
func (d *PositiveDecimal) UnmarshalJSON(data []byte) error {
	var f float64
	if err := json.Unmarshal(data, &f); err != nil {
		return err
	}
	parsed := PositiveDecimal(f)
	if err := parsed.Validate(); err != nil {
		return err
	}
	*d = parsed
	return nil
}

// NonNegativeInt is the carrier for `@semantic.NonNegativeInt`. The stored
// value is an int64 constrained to value >= 0 (non-negative — negatives are
// rejected). DDL column is BIGINT, matching Integer's storage.
type NonNegativeInt int64

// Validate reports whether the value is non-negative (>= 0).
func (n NonNegativeInt) Validate() error {
	if n < 0 {
		return fmt.Errorf("lazuli: value %d must be greater than or equal to 0", int64(n))
	}
	return nil
}

// Value implements driver.Valuer so NonNegativeInt binds directly as a
// BIGINT column value.
func (n NonNegativeInt) Value() (driver.Value, error) { return int64(n), nil }

// Scan implements sql.Scanner so pgx can hydrate NonNegativeInt from a
// BIGINT column. Scan is trusting (DB is the source of truth); shape
// enforcement happens at the wire decode boundary via UnmarshalJSON.
func (n *NonNegativeInt) Scan(src any) error {
	switch v := src.(type) {
	case int64:
		*n = NonNegativeInt(v)
	case float64:
		*n = NonNegativeInt(v)
	case nil:
		*n = 0
	default:
		return fmt.Errorf("lazuli: cannot scan %T into NonNegativeInt", src)
	}
	return nil
}

// MarshalJSON emits the bare integer.
func (n NonNegativeInt) MarshalJSON() ([]byte, error) { return json.Marshal(int64(n)) }

// UnmarshalJSON decodes a JSON number and enforces value >= 0. A negative
// value returns an error that lifts to a 400 validation_failed envelope
// through the command decode pipeline.
func (n *NonNegativeInt) UnmarshalJSON(data []byte) error {
	var i int64
	if err := json.Unmarshal(data, &i); err != nil {
		return err
	}
	parsed := NonNegativeInt(i)
	if err := parsed.Validate(); err != nil {
		return err
	}
	*n = parsed
	return nil
}
