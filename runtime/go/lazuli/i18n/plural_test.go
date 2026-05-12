package i18n_test

import (
	"errors"
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/i18n"
)

func TestSupportedPluralLocales(t *testing.T) {
	got := i18n.SupportedPluralLocales()
	want := []string{"en-US", "pt-BR"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("SupportedPluralLocales() = %#v, want %#v", got, want)
	}

	got[0] = "changed"
	if got := i18n.SupportedPluralLocales(); !reflect.DeepEqual(got, want) {
		t.Fatalf("SupportedPluralLocales() returned mutable backing storage: %#v", got)
	}
}

func TestPluralCategoryFor(t *testing.T) {
	tests := []struct {
		name   string
		locale string
		count  int
		want   i18n.PluralCategory
	}{
		{
			name:   "en one",
			locale: "en-US",
			count:  1,
			want:   i18n.PluralOne,
		},
		{
			name:   "en zero",
			locale: "en-US",
			count:  0,
			want:   i18n.PluralOther,
		},
		{
			name:   "pt zero",
			locale: "pt-BR",
			count:  0,
			want:   i18n.PluralOne,
		},
		{
			name:   "pt language fallback",
			locale: "pt-PT",
			count:  0,
			want:   i18n.PluralOne,
		},
		{
			name:   "unknown locale fallback",
			locale: "zz-ZZ",
			count:  0,
			want:   i18n.PluralOther,
		},
		{
			name:   "pt other",
			locale: "pt-BR",
			count:  2,
			want:   i18n.PluralOther,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := i18n.PluralCategoryFor(tt.locale, tt.count); got != tt.want {
				t.Fatalf("PluralCategoryFor(%q, %d) = %q, want %q", tt.locale, tt.count, got, tt.want)
			}
		})
	}
}

func TestRequiredPluralCategories(t *testing.T) {
	want := []i18n.PluralCategory{i18n.PluralOne, i18n.PluralOther}
	got := i18n.RequiredPluralCategories("pt-BR")
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("RequiredPluralCategories() = %#v, want %#v", got, want)
	}

	got[0] = i18n.PluralZero
	if got := i18n.RequiredPluralCategories("pt-BR"); !reflect.DeepEqual(got, want) {
		t.Fatalf("RequiredPluralCategories() returned mutable backing storage: %#v", got)
	}
}

func TestValidatePluralMessages(t *testing.T) {
	err := i18n.ValidatePluralMessages("en-US", map[i18n.PluralCategory]string{
		i18n.PluralOne:   "{{count}} item",
		i18n.PluralOther: "{{count}} items",
	})
	if err != nil {
		t.Fatalf("ValidatePluralMessages complete map: %v", err)
	}

	err = i18n.ValidatePluralMessages("en-US", map[i18n.PluralCategory]string{
		i18n.PluralOther: "{{count}} items",
	})
	if !errors.Is(err, i18n.ErrPluralCategoryMissing) {
		t.Fatalf("ValidatePluralMessages error = %v, want ErrPluralCategoryMissing", err)
	}

	var missing *i18n.MissingPluralCategoryError
	if !errors.As(err, &missing) {
		t.Fatalf("ValidatePluralMessages error = %T, want MissingPluralCategoryError", err)
	}
	if missing.Locale != "en-US" {
		t.Fatalf("missing locale = %q, want en-US", missing.Locale)
	}
	wantMissing := []i18n.PluralCategory{i18n.PluralOne}
	if !reflect.DeepEqual(missing.Categories, wantMissing) {
		t.Fatalf("missing categories = %#v, want %#v", missing.Categories, wantMissing)
	}
}

func TestSelectPluralMessage(t *testing.T) {
	messages := map[i18n.PluralCategory]string{
		i18n.PluralOne:   "one",
		i18n.PluralOther: "other",
	}

	tests := []struct {
		name   string
		locale string
		count  int
		want   string
	}{
		{
			name:   "en one",
			locale: "en-US",
			count:  1,
			want:   "one",
		},
		{
			name:   "en other",
			locale: "en-US",
			count:  2,
			want:   "other",
		},
		{
			name:   "pt zero one",
			locale: "pt-BR",
			count:  0,
			want:   "one",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := i18n.SelectPluralMessage(tt.locale, tt.count, messages)
			if err != nil {
				t.Fatalf("SelectPluralMessage: %v", err)
			}
			if got != tt.want {
				t.Fatalf("SelectPluralMessage(%q, %d) = %q, want %q", tt.locale, tt.count, got, tt.want)
			}
		})
	}
}

func TestSelectPluralMessageFallsBackToOther(t *testing.T) {
	got, err := i18n.SelectPluralMessage("en-US", 1, map[i18n.PluralCategory]string{
		i18n.PluralOther: "fallback",
	})
	if err != nil {
		t.Fatalf("SelectPluralMessage: %v", err)
	}
	if got != "fallback" {
		t.Fatalf("SelectPluralMessage fallback = %q, want fallback", got)
	}
}

func TestSelectPluralMessageMissingCategory(t *testing.T) {
	_, err := i18n.SelectPluralMessage("en-US", 2, map[i18n.PluralCategory]string{
		i18n.PluralOne: "one",
	})
	if !errors.Is(err, i18n.ErrPluralCategoryMissing) {
		t.Fatalf("SelectPluralMessage error = %v, want ErrPluralCategoryMissing", err)
	}

	var missing *i18n.MissingPluralCategoryError
	if !errors.As(err, &missing) {
		t.Fatalf("SelectPluralMessage error = %T, want MissingPluralCategoryError", err)
	}
	wantMissing := []i18n.PluralCategory{i18n.PluralOther}
	if !reflect.DeepEqual(missing.Categories, wantMissing) {
		t.Fatalf("missing categories = %#v, want %#v", missing.Categories, wantMissing)
	}
}
