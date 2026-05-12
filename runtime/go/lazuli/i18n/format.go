package i18n

import (
	"math"
	"strconv"
	"strings"
	"time"
)

const (
	defaultFormatLocale = "en-US"
	maxFractionDigits   = 18
)

type formatProfile struct {
	tag             string
	dateLayout      string
	timeLayout      string
	decimal         string
	group           string
	currencySpace   bool
	currencySymbols map[string]string
}

var fallbackFormatProfiles = []formatProfile{
	{
		tag:           "en-US",
		dateLayout:    "01/02/2006",
		timeLayout:    "3:04 PM",
		decimal:       ".",
		group:         ",",
		currencySpace: false,
		currencySymbols: map[string]string{
			"BRL": "R$",
			"EUR": "\u20ac",
			"USD": "$",
		},
	},
	{
		tag:           "pt-BR",
		dateLayout:    "02/01/2006",
		timeLayout:    "15:04",
		decimal:       ",",
		group:         ".",
		currencySpace: true,
		currencySymbols: map[string]string{
			"BRL": "R$",
			"EUR": "\u20ac",
			"USD": "US$",
		},
	},
}

// SupportedFormatLocales returns the locale tags backed by Lazuli's tiny
// fallback formatter profile table.
//
// The format helpers in this file are deterministic stdlib-only fallbacks for
// generated apps that do not wire a CLDR/ICU formatter. They intentionally do
// not implement localized digits, calendars, plural rules, accounting currency
// formats, or the full set of CLDR locale preferences.
func SupportedFormatLocales() []string {
	locales := make([]string, 0, len(fallbackFormatProfiles))
	for _, profile := range fallbackFormatProfiles {
		locales = append(locales, profile.tag)
	}
	return locales
}

// FormatDate formats t as a short date for tag using Lazuli's fallback locale
// profiles. Unknown locales fall back to en-US, while language-only tags such
// as "pt" use the matching bundled profile when available.
func FormatDate(tag string, t time.Time) string {
	return t.Format(resolveFormatProfile(tag).dateLayout)
}

// FormatTime formats t as a short time for tag using Lazuli's fallback locale
// profiles. Unknown locales fall back to en-US, while language-only tags such
// as "pt" use the matching bundled profile when available.
func FormatTime(tag string, t time.Time) string {
	return t.Format(resolveFormatProfile(tag).timeLayout)
}

// FormatNumber formats value with grouped integer digits and fractionDigits
// decimal places using Lazuli's fallback locale profiles.
//
// fractionDigits is clamped to [0, 18] to keep accidental large precision
// requests from producing unbounded strings. NaN and infinities are returned in
// Go's stable strconv representation.
func FormatNumber(tag string, value float64, fractionDigits int) string {
	return formatDecimal(resolveFormatProfile(tag), value, fractionDigits)
}

// FormatCurrency formats amount with two fraction digits and a small set of
// ISO 4217 symbols using Lazuli's fallback locale profiles.
//
// Unknown currency codes are uppercased and printed before the number. This is
// a fallback display helper, not an accounting, cash rounding, or CLDR currency
// formatter.
func FormatCurrency(tag string, amount float64, currency string) string {
	profile := resolveFormatProfile(tag)
	code := strings.ToUpper(strings.TrimSpace(currency))
	if code == "" {
		code = "XXX"
	}

	symbol, ok := profile.currencySymbols[code]
	if !ok {
		symbol = code
	}

	number := formatDecimal(profile, amount, 2)
	sign := ""
	if strings.HasPrefix(number, "-") {
		sign = "-"
		number = strings.TrimPrefix(number, "-")
	}

	if profile.currencySpace || !ok {
		return sign + symbol + " " + number
	}
	return sign + symbol + number
}

func resolveFormatProfile(tag string) formatProfile {
	tag = strings.TrimSpace(tag)
	if tag != "" {
		for _, profile := range fallbackFormatProfiles {
			if strings.EqualFold(profile.tag, tag) {
				return profile
			}
		}

		language := strings.ToLower(strings.SplitN(tag, "-", 2)[0])
		for _, profile := range fallbackFormatProfiles {
			if strings.EqualFold(strings.SplitN(profile.tag, "-", 2)[0], language) {
				return profile
			}
		}
	}

	for _, profile := range fallbackFormatProfiles {
		if profile.tag == defaultFormatLocale {
			return profile
		}
	}
	return fallbackFormatProfiles[0]
}

func formatDecimal(profile formatProfile, value float64, fractionDigits int) string {
	if math.IsNaN(value) || math.IsInf(value, 0) {
		return strconv.FormatFloat(value, 'g', -1, 64)
	}

	fractionDigits = clampFractionDigits(fractionDigits)
	value = roundToFractionDigits(value, fractionDigits)
	formatted := strconv.FormatFloat(value, 'f', fractionDigits, 64)
	negative := strings.HasPrefix(formatted, "-")
	if negative {
		formatted = strings.TrimPrefix(formatted, "-")
	}

	whole, fraction, _ := strings.Cut(formatted, ".")
	grouped := groupDigits(whole, profile.group)
	if fractionDigits > 0 {
		formatted = grouped + profile.decimal + fraction
	} else {
		formatted = grouped
	}

	if negative && !isRoundedZero(whole, fraction) {
		return "-" + formatted
	}
	return formatted
}

func roundToFractionDigits(value float64, fractionDigits int) float64 {
	if fractionDigits == 0 {
		return math.Round(value)
	}

	scale := math.Pow10(fractionDigits)
	if math.Abs(value) > math.MaxFloat64/scale {
		return value
	}
	return math.Round(value*scale) / scale
}

func clampFractionDigits(fractionDigits int) int {
	switch {
	case fractionDigits < 0:
		return 0
	case fractionDigits > maxFractionDigits:
		return maxFractionDigits
	default:
		return fractionDigits
	}
}

func groupDigits(digits string, separator string) string {
	if len(digits) <= 3 {
		return digits
	}

	var b strings.Builder
	firstGroup := len(digits) % 3
	if firstGroup == 0 {
		firstGroup = 3
	}

	b.Grow(len(digits) + ((len(digits)-1)/3)*len(separator))
	b.WriteString(digits[:firstGroup])
	for i := firstGroup; i < len(digits); i += 3 {
		b.WriteString(separator)
		b.WriteString(digits[i : i+3])
	}
	return b.String()
}

func isRoundedZero(whole string, fraction string) bool {
	for _, r := range whole {
		if r != '0' {
			return false
		}
	}
	for _, r := range fraction {
		if r != '0' {
			return false
		}
	}
	return true
}
