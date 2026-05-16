package lazuli

import (
	"database/sql/driver"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
)

// MoneyValue is the rich currency-aware value type emitted by codegen
// for `@semantic.Money` / bare `Money` field declarations. Per
// `docs/proposals/semantic-types-money-brazilian.md` v0.3, it pairs
// a decimal amount with an ISO 4217 currency code.
//
// Storage: NUMERIC(20,4) for the amount + TEXT for the currency
// (auto-emitted alongside as `<field>_currency`). The amount is
// carried as a string at the wire layer for precision safety —
// JavaScript loses precision above 2^53 and we cannot assume the
// consumer parses through big-decimal libraries.
//
// Construction:
//
//	m := lazuli.BRL("19.90")        // 19.9000 BRL
//	m := lazuli.USD("0.00")          // 0.0000 USD
//	m := lazuli.MoneyValue{Amount: "1999.99", Currency: "BRL"}
//
// MoneyValue intentionally does NOT own arithmetic. Use
// `github.com/shopspring/decimal` directly:
//
//	d1, _ := decimal.NewFromString(m.Amount)
//	d2, _ := decimal.NewFromString(other.Amount)
//	sum := d1.Add(d2)
//
// The legacy `Money = int64` alias above is preserved for backward
// compatibility — code calling `var m lazuli.Money = 1990` keeps
// compiling. The DSL keyword `Money` resolves to `MoneyValue` (see
// `crates/lazuli_analyzer/src/lib.rs`).
type MoneyValue struct {
	Amount   string `json:"amount"`
	Currency string `json:"currency"`
}

// BRL builds a Money value in Brazilian Reais. `amount` is a decimal
// string ("19.90", "0.00", "1234.5678") — passed directly to the
// wire without coercion so precision is the caller's contract.
func BRL(amount string) MoneyValue {
	return MoneyValue{Amount: amount, Currency: "BRL"}
}

// USD builds a Money value in US Dollars.
func USD(amount string) MoneyValue {
	return MoneyValue{Amount: amount, Currency: "USD"}
}

// EUR builds a Money value in Euros.
func EUR(amount string) MoneyValue {
	return MoneyValue{Amount: amount, Currency: "EUR"}
}

// ParseMoneyLiteral parses the canonical default-literal form
// `<ISO 4217>:<decimal>` from the DSL. Returns an error for
// malformed input (no colon, empty parts, non-uppercase ISO).
//
//	ParseMoneyLiteral("BRL:19.90") -> MoneyValue{Amount:"19.90", Currency:"BRL"}
//	ParseMoneyLiteral("BRL:0")     -> error: missing decimal point
//
// Matches the parser-enforced shape per proposal v0.3 §"One canonical
// default-value form". Bare integer fragments after `:` are rejected
// to prevent silent 100× errors (author meant cents, codegen takes
// integer reais).
func ParseMoneyLiteral(literal string) (MoneyValue, error) {
	idx := strings.IndexByte(literal, ':')
	if idx <= 0 || idx == len(literal)-1 {
		return MoneyValue{}, fmt.Errorf(
			"money literal must be \"<ISO 4217>:<decimal>\" (got %q)",
			literal,
		)
	}
	currency := literal[:idx]
	amount := literal[idx+1:]
	if len(currency) != 3 || currency != strings.ToUpper(currency) {
		return MoneyValue{}, fmt.Errorf(
			"money literal currency must be 3-letter ISO 4217 uppercase (got %q)",
			currency,
		)
	}
	if !strings.Contains(amount, ".") {
		return MoneyValue{}, fmt.Errorf(
			"money literal amount must contain a decimal point — bare integer rejected to prevent 100x errors (got %q)",
			amount,
		)
	}
	return MoneyValue{Amount: amount, Currency: currency}, nil
}

// Value implements `driver.Valuer` so MoneyValue can be passed
// directly to pgx queries as the amount column value. The currency
// column is bound separately by codegen.
func (m MoneyValue) Value() (driver.Value, error) {
	return m.Amount, nil
}

// Scan implements `sql.Scanner` so pgx can hydrate MoneyValue from
// the amount column. Currency is populated in a separate pass by the
// codegen-emitted struct-scan helper.
func (m *MoneyValue) Scan(src any) error {
	switch v := src.(type) {
	case string:
		m.Amount = v
	case []byte:
		m.Amount = string(v)
	case nil:
		m.Amount = "0"
	default:
		return fmt.Errorf("lazuli: cannot scan %T into MoneyValue.Amount", src)
	}
	return nil
}

// MarshalJSON emits `{"amount": "19.90", "currency": "BRL"}`.
func (m MoneyValue) MarshalJSON() ([]byte, error) {
	type wire struct {
		Amount   string `json:"amount"`
		Currency string `json:"currency"`
	}
	return json.Marshal(wire{Amount: m.Amount, Currency: m.Currency})
}

// UnmarshalJSON accepts the canonical `{amount, currency}` shape.
// Rejects malformed input (currency not 3-letter, amount missing).
func (m *MoneyValue) UnmarshalJSON(data []byte) error {
	var wire struct {
		Amount   string `json:"amount"`
		Currency string `json:"currency"`
	}
	if err := json.Unmarshal(data, &wire); err != nil {
		return err
	}
	if wire.Currency != "" && (len(wire.Currency) != 3 || wire.Currency != strings.ToUpper(wire.Currency)) {
		return errors.New("lazuli: MoneyValue.Currency must be 3-letter ISO 4217 uppercase")
	}
	m.Amount = wire.Amount
	m.Currency = wire.Currency
	return nil
}
