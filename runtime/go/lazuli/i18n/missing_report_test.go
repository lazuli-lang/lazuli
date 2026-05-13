package i18n_test

import (
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/i18n"
)

func TestBuildTranslationReportReportsMissingAndUnusedTranslationsDeterministically(t *testing.T) {
	t.Parallel()

	request := i18n.TranslationReportRequest{
		RequiredLocales: []string{"pt-BR", "en-US", "en-US"},
		RequiredKeys:    []string{"settings.title", "account.locked", "account.locked"},
		Catalogs: map[string]map[string]string{
			"pt-BR": {
				"settings.title": "Configuracoes",
				"legacy.key":     "Antigo",
			},
			"en-US": {
				"account.locked": "Locked",
				"legacy.key":     "Legacy",
			},
			"es-AR": {
				"account.locked": "Bloqueado",
				"legacy.key":     "Anterior",
			},
		},
	}

	got := i18n.BuildTranslationReport(request)
	gotAgain := i18n.BuildTranslationReport(request)
	if !reflect.DeepEqual(got, gotAgain) {
		t.Fatalf("BuildTranslationReport is not deterministic:\n%#v\n%#v", got, gotAgain)
	}

	wantMissing := []i18n.TranslationReportEntry{
		{Locale: "en-US", Key: "settings.title"},
		{Locale: "pt-BR", Key: "account.locked"},
	}
	if !reflect.DeepEqual(got.MissingTranslations, wantMissing) {
		t.Fatalf("missing translations = %#v, want %#v", got.MissingTranslations, wantMissing)
	}

	wantUnusedLocales := []string{"es-AR"}
	if !reflect.DeepEqual(got.UnusedLocales, wantUnusedLocales) {
		t.Fatalf("unused locales = %#v, want %#v", got.UnusedLocales, wantUnusedLocales)
	}

	wantUnused := []i18n.TranslationReportEntry{
		{Locale: "en-US", Key: "legacy.key"},
		{Locale: "es-AR", Key: "account.locked"},
		{Locale: "es-AR", Key: "legacy.key"},
		{Locale: "pt-BR", Key: "legacy.key"},
	}
	if !reflect.DeepEqual(got.UnusedTranslations, wantUnused) {
		t.Fatalf("unused translations = %#v, want %#v", got.UnusedTranslations, wantUnused)
	}
	if got.OK() {
		t.Fatal("report OK = true, want false")
	}
}

func TestBuildTranslationReportReportsLocaleOnlyGaps(t *testing.T) {
	t.Parallel()

	got := i18n.BuildTranslationReport(i18n.TranslationReportRequest{
		RequiredLocales: []string{"pt-BR", "en-US"},
		Catalogs: map[string]map[string]string{
			"en-US": {},
			"fr-FR": {},
		},
	})

	if want := []string{"pt-BR"}; !reflect.DeepEqual(got.MissingLocales, want) {
		t.Fatalf("missing locales = %#v, want %#v", got.MissingLocales, want)
	}
	if want := []string{"fr-FR"}; !reflect.DeepEqual(got.UnusedLocales, want) {
		t.Fatalf("unused locales = %#v, want %#v", got.UnusedLocales, want)
	}
	if len(got.MissingTranslations) != 0 {
		t.Fatalf("missing translations = %#v, want none", got.MissingTranslations)
	}
	if len(got.UnusedTranslations) != 0 {
		t.Fatalf("unused translations = %#v, want none", got.UnusedTranslations)
	}
}

func TestBuildTranslationReportOKWhenCatalogMatchesRequirements(t *testing.T) {
	t.Parallel()

	got := i18n.BuildTranslationReport(i18n.TranslationReportRequest{
		RequiredLocales: []string{" en-US ", "", "en-US"},
		RequiredKeys:    []string{"welcome", "", "welcome"},
		Catalogs: map[string]map[string]string{
			"en-US": {
				"welcome": "",
			},
		},
	})

	if !got.OK() {
		t.Fatalf("report OK = false, report = %#v", got)
	}
}

func TestMessageCatalogBuildTranslationReportUsesContractLocales(t *testing.T) {
	t.Parallel()

	catalog := i18n.NewMessageCatalog(i18n.LocaleContract{
		Default:   "en",
		Supported: []string{"pt"},
	}, map[string]map[string]string{
		"en": {
			"welcome": "Welcome",
		},
		"pt": {},
	})

	got := catalog.BuildTranslationReport([]string{"welcome"})
	want := []i18n.TranslationReportEntry{{Locale: "pt", Key: "welcome"}}
	if !reflect.DeepEqual(got.MissingTranslations, want) {
		t.Fatalf("missing translations = %#v, want %#v", got.MissingTranslations, want)
	}
	if len(got.MissingLocales) != 0 {
		t.Fatalf("missing locales = %#v, want none", got.MissingLocales)
	}
}
