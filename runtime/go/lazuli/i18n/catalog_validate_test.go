package i18n_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/i18n"
)

func TestValidateLocaleTag(t *testing.T) {
	t.Parallel()

	valid := []string{
		"en",
		"pt-BR",
		"zh-Hant-TW",
		"en-US-u-ca-gregory",
	}
	for _, tag := range valid {
		tag := tag
		t.Run("valid "+tag, func(t *testing.T) {
			t.Parallel()

			if err := i18n.ValidateLocaleTag(tag); err != nil {
				t.Fatalf("ValidateLocaleTag(%q) error = %v", tag, err)
			}
		})
	}

	invalid := []string{
		"",
		" pt-BR",
		"pt_BR",
		"en--US",
		"1n",
		"en-US-u",
	}
	for _, tag := range invalid {
		tag := tag
		t.Run("invalid "+tag, func(t *testing.T) {
			t.Parallel()

			err := i18n.ValidateLocaleTag(tag)
			if !errors.Is(err, i18n.ErrInvalidLocaleTag) {
				t.Fatalf("ValidateLocaleTag(%q) error = %v, want ErrInvalidLocaleTag", tag, err)
			}
		})
	}
}

func TestValidateCatalogMessagesAcceptsConsistentPlaceholders(t *testing.T) {
	t.Parallel()

	err := i18n.ValidateCatalogMessages([]i18n.CatalogMessage{
		{Locale: "en", Key: "welcome", Message: "Hello, {name}. You have {count} alerts."},
		{Locale: "pt-BR", Key: "welcome", Message: "{count} alertas para { name }."},
		{Locale: "en", Key: "literal", Message: "Use {{name}} literally."},
		{Locale: "pt-BR", Key: "literal", Message: "Use {{nome}} literalmente."},
	})
	if err != nil {
		t.Fatalf("ValidateCatalogMessages: %v", err)
	}
}

func TestValidateCatalogMessagesReportsDuplicateKeysAndPlaceholderMismatch(t *testing.T) {
	t.Parallel()

	err := i18n.ValidateCatalogMessages([]i18n.CatalogMessage{
		{Locale: "en", Key: "welcome", Message: "Hello, {name}. You have {count} alerts."},
		{Locale: "en", Key: "welcome", Message: "Duplicate {name}."},
		{Locale: "pt-BR", Key: "welcome", Message: "Ola, {name}."},
	})
	if !errors.Is(err, i18n.ErrDuplicateCatalogKey) {
		t.Fatalf("ValidateCatalogMessages error = %v, want ErrDuplicateCatalogKey", err)
	}
	if !errors.Is(err, i18n.ErrCatalogPlaceholderMismatch) {
		t.Fatalf("ValidateCatalogMessages error = %v, want ErrCatalogPlaceholderMismatch", err)
	}
	for _, fragment := range []string{
		`message[1] locale "en" key "welcome" also appears at message[0]`,
		`locale "pt-BR" placeholders "name" differ from locale "en" placeholders "count", "name"`,
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateCatalogMessages error = %q, want fragment %q", err, fragment)
		}
	}
}

func TestValidateCatalogMessagesReportsInvalidFormat(t *testing.T) {
	t.Parallel()

	err := i18n.ValidateCatalogMessages([]i18n.CatalogMessage{
		{Locale: "en", Key: "welcome", Message: "Hello, {name"},
	})
	if !errors.Is(err, i18n.ErrMessageFormatInvalid) {
		t.Fatalf("ValidateCatalogMessages error = %v, want ErrMessageFormatInvalid", err)
	}

	var invalid *i18n.InvalidMessageFormatError
	if !errors.As(err, &invalid) {
		t.Fatalf("ValidateCatalogMessages error = %T, want InvalidMessageFormatError", err)
	}
	if invalid.Key != "welcome" || invalid.Offset != 7 {
		t.Fatalf("invalid format = %+v", invalid)
	}
}

func TestValidateCatalogPluralMessagesAcceptsCompleteArms(t *testing.T) {
	t.Parallel()

	err := i18n.ValidateCatalogPluralMessages([]i18n.CatalogPluralMessage{
		{
			Locale: "en-US",
			Key:    "cart.items",
			Arms: []i18n.CatalogPluralArm{
				{Category: i18n.PluralOne, Message: "{count} item"},
				{Category: i18n.PluralOther, Message: "{count} items"},
			},
		},
		{
			Locale: "pt-BR",
			Key:    "cart.items",
			Arms: []i18n.CatalogPluralArm{
				{Category: i18n.PluralOne, Message: "{count} item"},
				{Category: i18n.PluralOther, Message: "{count} items"},
			},
		},
	})
	if err != nil {
		t.Fatalf("ValidateCatalogPluralMessages: %v", err)
	}
}

func TestValidateCatalogPluralMessagesReportsArmProblems(t *testing.T) {
	t.Parallel()

	err := i18n.ValidateCatalogPluralMessages([]i18n.CatalogPluralMessage{
		{
			Locale: "en-US",
			Key:    "cart.items",
			Arms: []i18n.CatalogPluralArm{
				{Category: i18n.PluralOne, Message: "{count} item"},
				{Category: i18n.PluralOne, Message: "{count} duplicate"},
				{Category: i18n.PluralCategory("wat"), Message: "{count} invalid"},
			},
		},
		{
			Locale: "en-us",
			Key:    "cart.items",
			Arms: []i18n.CatalogPluralArm{
				{Category: i18n.PluralOne, Message: "{count} item"},
				{Category: i18n.PluralOther, Message: "items"},
			},
		},
		{
			Locale: "pt-BR",
			Key:    "cart.items",
			Arms: []i18n.CatalogPluralArm{
				{Category: i18n.PluralOne, Message: "{count} item"},
				{Category: i18n.PluralOther, Message: "{count} items"},
			},
		},
	})
	for _, want := range []error{
		i18n.ErrInvalidPluralArm,
		i18n.ErrPluralCategoryMissing,
		i18n.ErrDuplicateCatalogKey,
		i18n.ErrCatalogPlaceholderMismatch,
	} {
		if !errors.Is(err, want) {
			t.Fatalf("ValidateCatalogPluralMessages error = %v, want %v", err, want)
		}
	}

	var missing *i18n.MissingPluralCategoryError
	if !errors.As(err, &missing) {
		t.Fatalf("ValidateCatalogPluralMessages error = %T, want MissingPluralCategoryError", err)
	}
	if !reflect.DeepEqual(missing.Categories, []i18n.PluralCategory{i18n.PluralOther}) {
		t.Fatalf("missing categories = %#v", missing.Categories)
	}
	for _, fragment := range []string{
		`plural[0].arms[1].category "one" also appears at plural[0].arms[0]`,
		`plural[0].arms[2].category "wat" is unknown`,
		`plural[1] locale "en-us" key "cart.items" also appears at plural[0]`,
		`key "cart.items#other" locale "pt-BR" placeholders "count" differ from locale "en-us" placeholders none`,
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateCatalogPluralMessages error = %q, want fragment %q", err, fragment)
		}
	}
}
