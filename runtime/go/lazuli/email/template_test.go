package email

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/i18n"
)

func TestLocalizedTemplateRendererFallsBackAndInterpolates(t *testing.T) {
	t.Parallel()

	renderer := NewLocalizedTemplateRenderer(testLocaleContract())
	rendered, err := renderer.Render(context.Background(), localizedWelcomeTemplate(), "pt-BR", map[string]any{
		"Name":      "Ada",
		"BookingID": "HP-42",
	})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}

	if rendered.Name != "booking_confirmation" {
		t.Fatalf("Name = %q", rendered.Name)
	}
	if rendered.RequestedLocale != "pt-BR" {
		t.Fatalf("RequestedLocale = %q", rendered.RequestedLocale)
	}
	if rendered.Locale != "pt" {
		t.Fatalf("Locale = %q, want pt", rendered.Locale)
	}
	if rendered.Subject != "Reserva HP-42 confirmada" {
		t.Fatalf("Subject = %q", rendered.Subject)
	}
	if rendered.Body != "Ola Ada, sua reserva HP-42 foi confirmada." {
		t.Fatalf("Body = %q", rendered.Body)
	}
	wantSearched := []string{"pt-BR", "pt", "en"}
	if !reflect.DeepEqual(rendered.SearchedLocales, wantSearched) {
		t.Fatalf("SearchedLocales = %#v, want %#v", rendered.SearchedLocales, wantSearched)
	}
}

func TestLocalizedTemplateRendererFallsBackToDefaultLocale(t *testing.T) {
	t.Parallel()

	renderer := NewLocalizedTemplateRenderer(testLocaleContract())
	rendered, err := renderer.Render(context.Background(), localizedWelcomeTemplate(), "fr-CA", map[string]any{
		"Name":      "Ada",
		"BookingID": "HP-42",
	})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}

	if rendered.Locale != "en" {
		t.Fatalf("Locale = %q, want en", rendered.Locale)
	}
	if rendered.Subject != "Booking HP-42 confirmed" {
		t.Fatalf("Subject = %q", rendered.Subject)
	}
	wantSearched := []string{"fr-CA", "fr", "en"}
	if !reflect.DeepEqual(rendered.SearchedLocales, wantSearched) {
		t.Fatalf("SearchedLocales = %#v, want %#v", rendered.SearchedLocales, wantSearched)
	}
}

func TestLocalizedTemplateRendererMissingLocaleErrorIncludesFallbacks(t *testing.T) {
	t.Parallel()

	renderer := NewLocalizedTemplateRenderer(testLocaleContract())
	_, err := renderer.Render(context.Background(), LocalizedTemplate{
		Name: "empty",
		Locales: map[string]TemplateSource{
			"es": {Subject: "Hola", Body: "Hola"},
		},
	}, "pt-BR", nil)
	if !errors.Is(err, ErrTemplateLocaleNotFound) {
		t.Fatalf("Render error = %v, want ErrTemplateLocaleNotFound", err)
	}

	var missing *MissingTemplateLocaleError
	if !errors.As(err, &missing) {
		t.Fatalf("Render error = %T, want MissingTemplateLocaleError", err)
	}
	wantSearched := []string{"pt-BR", "pt", "en"}
	if !reflect.DeepEqual(missing.Searched, wantSearched) {
		t.Fatalf("searched locales = %#v, want %#v", missing.Searched, wantSearched)
	}
	if !strings.Contains(err.Error(), "empty") {
		t.Fatalf("error = %q, want template name", err)
	}
}

func TestLocalizedTemplateRendererMissingKeyErrorMode(t *testing.T) {
	t.Parallel()

	renderer := LocalizedTemplateRenderer{
		LocaleContract: testLocaleContract(),
	}
	_, err := renderer.Render(context.Background(), LocalizedTemplate{
		Name: "welcome",
		Locales: map[string]TemplateSource{
			"en": {Subject: "Welcome", Body: "Hi {{.Missing}}"},
		},
	}, "en", map[string]any{})
	if err == nil {
		t.Fatal("expected missing-key error")
	}
	if !strings.Contains(err.Error(), "execute welcome.en.body template") {
		t.Fatalf("error = %q", err)
	}
}

func TestLocalizedTemplateRendererDefaultMissingKeyMode(t *testing.T) {
	t.Parallel()

	renderer := LocalizedTemplateRenderer{
		LocaleContract: testLocaleContract(),
		MissingKey:     MissingKeyDefault,
	}
	rendered, err := renderer.Render(context.Background(), LocalizedTemplate{
		Name: "welcome",
		Locales: map[string]TemplateSource{
			"en": {Subject: "Welcome", Body: "Hi {{.Missing}}"},
		},
	}, "en", map[string]any{})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(rendered.Body, "<no value>") {
		t.Fatalf("Body = %q, want missing key placeholder", rendered.Body)
	}
}

func TestLocalizedTemplateRendererZeroMissingKeyMode(t *testing.T) {
	t.Parallel()

	renderer := LocalizedTemplateRenderer{
		LocaleContract: testLocaleContract(),
		MissingKey:     MissingKeyZero,
	}
	rendered, err := renderer.Render(context.Background(), LocalizedTemplate{
		Name: "welcome",
		Locales: map[string]TemplateSource{
			"en": {Subject: "Welcome", Body: "Hi {{.User.Name}}"},
		},
	}, "en", map[string]any{
		"User": map[string]string{},
	})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if rendered.Body != "Hi " {
		t.Fatalf("Body = %q, want empty zero value", rendered.Body)
	}
}

func TestLocalizedTemplateRendererRejectsUnsupportedMissingKeyMode(t *testing.T) {
	t.Parallel()

	renderer := LocalizedTemplateRenderer{
		LocaleContract: testLocaleContract(),
		MissingKey:     MissingKeyMode("keep"),
	}
	_, err := renderer.Render(context.Background(), localizedWelcomeTemplate(), "en", map[string]any{
		"Name":      "Ada",
		"BookingID": "HP-42",
	})
	if err == nil {
		t.Fatal("expected unsupported missing-key mode error")
	}
	if !strings.Contains(err.Error(), "unsupported missing-key mode") {
		t.Fatalf("error = %q", err)
	}
}

func TestLocalizedTemplateRendererHonorsCanceledContext(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := NewLocalizedTemplateRenderer(testLocaleContract()).Render(ctx, localizedWelcomeTemplate(), "en", nil)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Render error = %v, want context.Canceled", err)
	}
}

func TestRenderedTemplatePreviewMessage(t *testing.T) {
	t.Parallel()

	rendered := RenderedTemplate{
		Subject: "Welcome",
		Body:    "Hello",
	}
	preview := rendered.PreviewMessage("Hostpoint <noreply@hostpoint.test>", "ada@example.test")

	if preview.From != "Hostpoint <noreply@hostpoint.test>" ||
		preview.To != "ada@example.test" ||
		preview.Subject != "Welcome" ||
		preview.TextBody != "Hello" ||
		preview.HTMLBody != "" {
		t.Fatalf("PreviewMessage = %+v", preview)
	}
}

func localizedWelcomeTemplate() LocalizedTemplate {
	return LocalizedTemplate{
		Name: "booking_confirmation",
		Locales: map[string]TemplateSource{
			"en": {
				Subject: "Booking {{.BookingID}} confirmed",
				Body:    "Hi {{.Name}}, your booking {{.BookingID}} is confirmed.",
			},
			"pt": {
				Subject: "Reserva {{.BookingID}} confirmada",
				Body:    "Ola {{.Name}}, sua reserva {{.BookingID}} foi confirmada.",
			},
		},
	}
}

func testLocaleContract() i18n.LocaleContract {
	return i18n.LocaleContract{
		Default:   "en",
		Supported: []string{"en", "pt", "pt-BR"},
		Fallbacks: []i18n.Fallback{
			{From: "pt-BR", To: "pt"},
			{From: "pt", To: "en"},
		},
	}
}
