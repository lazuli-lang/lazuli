package i18n

import (
	"errors"
	"fmt"
	"strings"
)

const defaultPluralLocale = "en-US"

// PluralCategory is one of the CLDR plural arm names accepted by Lazuli
// translation declarations.
type PluralCategory string

const (
	PluralZero  PluralCategory = "zero"
	PluralOne   PluralCategory = "one"
	PluralTwo   PluralCategory = "two"
	PluralFew   PluralCategory = "few"
	PluralMany  PluralCategory = "many"
	PluralOther PluralCategory = "other"
)

var (
	// ErrPluralCategoryMissing is wrapped when plural messages do not include
	// a category required by the selected locale profile.
	ErrPluralCategoryMissing = errors.New("lazuli/i18n: plural category missing")
)

type pluralProfile struct {
	tag      string
	required []PluralCategory
	selectFn func(count int) PluralCategory
}

var fallbackPluralProfiles = []pluralProfile{
	{
		tag:      "en-US",
		required: []PluralCategory{PluralOne, PluralOther},
		selectFn: englishPluralCategory,
	},
	{
		tag:      "pt-BR",
		required: []PluralCategory{PluralOne, PluralOther},
		selectFn: portuguesePluralCategory,
	},
}

// MissingPluralCategoryError describes plural message categories that are
// required for selection or validation but were not provided.
type MissingPluralCategoryError struct {
	Locale     string
	Categories []PluralCategory
}

func (e *MissingPluralCategoryError) Error() string {
	if len(e.Categories) == 0 {
		return fmt.Sprintf("%s: locale %q", ErrPluralCategoryMissing, e.Locale)
	}
	return fmt.Sprintf("%s: locale %q categories %s", ErrPluralCategoryMissing, e.Locale, pluralCategoryList(e.Categories))
}

// Unwrap returns the sentinel error for errors.Is checks.
func (e *MissingPluralCategoryError) Unwrap() error {
	return ErrPluralCategoryMissing
}

// SupportedPluralLocales returns the locale tags backed by Lazuli's tiny
// fallback plural profile table.
//
// These helpers intentionally do not implement full ICU MessageFormat or the
// full CLDR plural rule set. They provide deterministic cardinal selection for
// generated apps that only need bundled English and Portuguese fallbacks.
func SupportedPluralLocales() []string {
	locales := make([]string, 0, len(fallbackPluralProfiles))
	for _, profile := range fallbackPluralProfiles {
		locales = append(locales, profile.tag)
	}
	return locales
}

// PluralCategoryFor returns the plural category for count using Lazuli's
// fallback locale profiles. Unknown locales fall back to en-US, while
// language-only tags such as "pt" use the matching bundled profile when
// available.
func PluralCategoryFor(tag string, count int) PluralCategory {
	return resolvePluralProfile(tag).selectFn(count)
}

// RequiredPluralCategories returns the plural categories that should be
// present for tag's fallback profile. The returned slice is a copy.
func RequiredPluralCategories(tag string) []PluralCategory {
	return clonePluralCategories(resolvePluralProfile(tag).required)
}

// ValidatePluralMessages checks that messages includes every category required
// by tag's fallback plural profile. Empty strings are accepted when the
// category key is present.
func ValidatePluralMessages(tag string, messages map[PluralCategory]string) error {
	required := RequiredPluralCategories(tag)
	missing := make([]PluralCategory, 0, len(required))
	for _, category := range required {
		if _, ok := messages[category]; !ok {
			missing = append(missing, category)
		}
	}
	if len(missing) > 0 {
		return &MissingPluralCategoryError{
			Locale:     tag,
			Categories: missing,
		}
	}
	return nil
}

// SelectPluralMessage selects the message for count and tag. When the exact
// selected category is absent, it falls back to the "other" message if one was
// provided.
func SelectPluralMessage(tag string, count int, messages map[PluralCategory]string) (string, error) {
	category := PluralCategoryFor(tag, count)
	if message, ok := messages[category]; ok {
		return message, nil
	}
	if category != PluralOther {
		if message, ok := messages[PluralOther]; ok {
			return message, nil
		}
	}

	missing := []PluralCategory{category}
	if category != PluralOther {
		missing = append(missing, PluralOther)
	}
	return "", &MissingPluralCategoryError{
		Locale:     tag,
		Categories: missing,
	}
}

func resolvePluralProfile(tag string) pluralProfile {
	tag = strings.TrimSpace(tag)
	if tag != "" {
		for _, profile := range fallbackPluralProfiles {
			if strings.EqualFold(profile.tag, tag) {
				return profile
			}
		}

		language := strings.ToLower(strings.SplitN(tag, "-", 2)[0])
		for _, profile := range fallbackPluralProfiles {
			if strings.EqualFold(strings.SplitN(profile.tag, "-", 2)[0], language) {
				return profile
			}
		}
	}

	for _, profile := range fallbackPluralProfiles {
		if profile.tag == defaultPluralLocale {
			return profile
		}
	}
	return fallbackPluralProfiles[0]
}

func englishPluralCategory(count int) PluralCategory {
	if count == 1 {
		return PluralOne
	}
	return PluralOther
}

func portuguesePluralCategory(count int) PluralCategory {
	if count == 0 || count == 1 {
		return PluralOne
	}
	return PluralOther
}

func clonePluralCategories(categories []PluralCategory) []PluralCategory {
	return append([]PluralCategory(nil), categories...)
}

func pluralCategoryList(categories []PluralCategory) string {
	parts := make([]string, 0, len(categories))
	for _, category := range categories {
		parts = append(parts, string(category))
	}
	return strings.Join(parts, ", ")
}
