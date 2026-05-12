package i18n_test

import (
	"reflect"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/i18n"
)

func TestSupportedFormatLocales(t *testing.T) {
	got := i18n.SupportedFormatLocales()
	want := []string{"en-US", "pt-BR"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("SupportedFormatLocales() = %#v, want %#v", got, want)
	}

	got[0] = "changed"
	if got := i18n.SupportedFormatLocales(); !reflect.DeepEqual(got, want) {
		t.Fatalf("SupportedFormatLocales() returned mutable backing storage: %#v", got)
	}
}

func TestFormatDateAndTime(t *testing.T) {
	ts := time.Date(2026, 5, 12, 17, 6, 7, 0, time.UTC)

	tests := []struct {
		name     string
		locale   string
		wantDate string
		wantTime string
	}{
		{
			name:     "en-US",
			locale:   "en-US",
			wantDate: "05/12/2026",
			wantTime: "5:06 PM",
		},
		{
			name:     "pt-BR",
			locale:   "pt-BR",
			wantDate: "12/05/2026",
			wantTime: "17:06",
		},
		{
			name:     "language fallback",
			locale:   "pt-PT",
			wantDate: "12/05/2026",
			wantTime: "17:06",
		},
		{
			name:     "unknown fallback",
			locale:   "zz-ZZ",
			wantDate: "05/12/2026",
			wantTime: "5:06 PM",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := i18n.FormatDate(tt.locale, ts); got != tt.wantDate {
				t.Fatalf("FormatDate(%q) = %q, want %q", tt.locale, got, tt.wantDate)
			}
			if got := i18n.FormatTime(tt.locale, ts); got != tt.wantTime {
				t.Fatalf("FormatTime(%q) = %q, want %q", tt.locale, got, tt.wantTime)
			}
		})
	}
}

func TestFormatNumber(t *testing.T) {
	tests := []struct {
		name           string
		locale         string
		value          float64
		fractionDigits int
		want           string
	}{
		{
			name:           "en-US grouped decimal",
			locale:         "en-US",
			value:          1234567.891,
			fractionDigits: 2,
			want:           "1,234,567.89",
		},
		{
			name:           "pt-BR grouped decimal",
			locale:         "pt-BR",
			value:          -1234567.5,
			fractionDigits: 1,
			want:           "-1.234.567,5",
		},
		{
			name:           "rounds whole values",
			locale:         "en-US",
			value:          1234.5,
			fractionDigits: 0,
			want:           "1,235",
		},
		{
			name:           "clamps negative precision",
			locale:         "pt-BR",
			value:          1234.49,
			fractionDigits: -1,
			want:           "1.234",
		},
		{
			name:           "normalizes rounded negative zero",
			locale:         "en-US",
			value:          -0.004,
			fractionDigits: 2,
			want:           "0.00",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := i18n.FormatNumber(tt.locale, tt.value, tt.fractionDigits); got != tt.want {
				t.Fatalf("FormatNumber(%q, %v, %d) = %q, want %q", tt.locale, tt.value, tt.fractionDigits, got, tt.want)
			}
		})
	}
}

func TestFormatCurrency(t *testing.T) {
	tests := []struct {
		name     string
		locale   string
		amount   float64
		currency string
		want     string
	}{
		{
			name:     "en-US USD",
			locale:   "en-US",
			amount:   1234.5,
			currency: "usd",
			want:     "$1,234.50",
		},
		{
			name:     "pt-BR BRL",
			locale:   "pt-BR",
			amount:   1234.5,
			currency: "brl",
			want:     "R$ 1.234,50",
		},
		{
			name:     "pt-BR USD",
			locale:   "pt-BR",
			amount:   1234.5,
			currency: "USD",
			want:     "US$ 1.234,50",
		},
		{
			name:     "negative en-US amount",
			locale:   "en-US",
			amount:   -1234.5,
			currency: "USD",
			want:     "-$1,234.50",
		},
		{
			name:     "unknown currency code",
			locale:   "pt-BR",
			amount:   1234.5,
			currency: "xyz",
			want:     "XYZ 1.234,50",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := i18n.FormatCurrency(tt.locale, tt.amount, tt.currency); got != tt.want {
				t.Fatalf("FormatCurrency(%q, %v, %q) = %q, want %q", tt.locale, tt.amount, tt.currency, got, tt.want)
			}
		})
	}
}
