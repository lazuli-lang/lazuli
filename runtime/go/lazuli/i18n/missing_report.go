package i18n

import (
	"sort"
	"strings"
)

// TranslationReportRequest describes the required locale/key coverage and the
// loaded flat catalogs to compare against it.
type TranslationReportRequest struct {
	RequiredLocales []string                     `json:"required_locales"`
	RequiredKeys    []string                     `json:"required_keys"`
	Catalogs        map[string]map[string]string `json:"catalogs"`
}

// TranslationReport is a deterministic coverage diff for flat translation
// catalogs. Translation entries are sorted by locale, then key.
type TranslationReport struct {
	MissingLocales      []string                 `json:"missing_locales"`
	MissingTranslations []TranslationReportEntry `json:"missing_translations"`
	UnusedLocales       []string                 `json:"unused_locales"`
	UnusedTranslations  []TranslationReportEntry `json:"unused_translations"`
}

// OK reports whether the catalog coverage exactly matches the required
// locales and keys.
func (r TranslationReport) OK() bool {
	return len(r.MissingLocales) == 0 &&
		len(r.MissingTranslations) == 0 &&
		len(r.UnusedLocales) == 0 &&
		len(r.UnusedTranslations) == 0
}

// TranslationReportEntry identifies one locale/key catalog entry.
type TranslationReportEntry struct {
	Locale string `json:"locale"`
	Key    string `json:"key"`
}

// BuildTranslationReport compares required locale/key coverage against flat
// locale catalogs. Blank required locales and keys are ignored after trimming;
// duplicates are collapsed. Empty translation strings count as present.
func BuildTranslationReport(request TranslationReportRequest) TranslationReport {
	requiredLocales := sortedUniqueTrimmedStrings(request.RequiredLocales)
	requiredKeys := sortedUniqueTrimmedStrings(request.RequiredKeys)
	catalogLocales := sortedCatalogLocales(request.Catalogs)

	requiredLocaleSet := stringSet(requiredLocales)
	requiredKeySet := stringSet(requiredKeys)

	report := TranslationReport{
		MissingLocales:      make([]string, 0),
		MissingTranslations: make([]TranslationReportEntry, 0),
		UnusedLocales:       make([]string, 0),
		UnusedTranslations:  make([]TranslationReportEntry, 0),
	}

	for _, locale := range requiredLocales {
		entries, localeFound := request.Catalogs[locale]
		if !localeFound {
			report.MissingLocales = append(report.MissingLocales, locale)
		}
		for _, key := range requiredKeys {
			if !localeFound {
				report.MissingTranslations = append(report.MissingTranslations, TranslationReportEntry{
					Locale: locale,
					Key:    key,
				})
				continue
			}
			if _, keyFound := entries[key]; !keyFound {
				report.MissingTranslations = append(report.MissingTranslations, TranslationReportEntry{
					Locale: locale,
					Key:    key,
				})
			}
		}
	}

	for _, locale := range catalogLocales {
		_, localeRequired := requiredLocaleSet[locale]
		if !localeRequired {
			report.UnusedLocales = append(report.UnusedLocales, locale)
		}

		for _, key := range sortedCatalogKeys(request.Catalogs[locale]) {
			_, keyRequired := requiredKeySet[key]
			if !localeRequired || !keyRequired {
				report.UnusedTranslations = append(report.UnusedTranslations, TranslationReportEntry{
					Locale: locale,
					Key:    key,
				})
			}
		}
	}

	return report
}

// BuildTranslationReport reports coverage for this catalog using the
// MessageCatalog locale contract. LocaleContract.Default is included even when
// a malformed contract omits it from Supported.
func (c MessageCatalog) BuildTranslationReport(requiredKeys []string) TranslationReport {
	return BuildTranslationReport(TranslationReportRequest{
		RequiredLocales: messageCatalogReportLocales(c.contract),
		RequiredKeys:    requiredKeys,
		Catalogs:        c.messages,
	})
}

func messageCatalogReportLocales(contract LocaleContract) []string {
	locales := make([]string, 0, len(contract.Supported)+1)
	locales = append(locales, contract.Supported...)
	if contract.Default != "" {
		locales = append(locales, contract.Default)
	}
	return locales
}

func sortedUniqueTrimmedStrings(values []string) []string {
	seen := make(map[string]struct{}, len(values))
	out := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out
}

func sortedCatalogLocales(catalogs map[string]map[string]string) []string {
	locales := make([]string, 0, len(catalogs))
	for locale := range catalogs {
		locales = append(locales, locale)
	}
	sort.Strings(locales)
	return locales
}

func sortedCatalogKeys(entries map[string]string) []string {
	keys := make([]string, 0, len(entries))
	for key := range entries {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func stringSet(values []string) map[string]struct{} {
	set := make(map[string]struct{}, len(values))
	for _, value := range values {
		set[value] = struct{}{}
	}
	return set
}
