package email

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"strings"
	texttemplate "text/template"

	"lazuli.dev/runtime/lazuli/i18n"
)

// TemplateRenderer renders localized email subject and body templates.
type TemplateRenderer interface {
	Render(ctx context.Context, tmpl LocalizedTemplate, locale string, data map[string]any) (RenderedTemplate, error)
}

// LocalizedTemplate contains the locale-specific source for one email.
type LocalizedTemplate struct {
	// Name is used in parse and execution errors.
	Name string
	// DefaultLocale is the final fallback locale for this template. When empty,
	// the renderer's LocaleContract.Default is used.
	DefaultLocale string
	// Locales maps locale tags to their subject and body templates.
	Locales map[string]TemplateSource
}

// TemplateSource contains the source text for a single locale.
type TemplateSource struct {
	Subject string
	Body    string
}

// RenderedTemplate is the preview-friendly output of rendering an email
// template.
type RenderedTemplate struct {
	Name            string
	RequestedLocale string
	Locale          string
	SearchedLocales []string
	Subject         string
	Body            string
}

// PreviewMessage returns the rendered email in the in-memory preview shape.
func (r RenderedTemplate) PreviewMessage(from, to string) PreviewMessage {
	return PreviewMessage{
		From:     from,
		To:       to,
		Subject:  r.Subject,
		TextBody: r.Body,
	}
}

// MissingKeyMode controls how LocalizedTemplateRenderer handles missing map
// keys during interpolation.
type MissingKeyMode string

const (
	// MissingKeyDefault uses text/template's default missing-key behavior.
	MissingKeyDefault MissingKeyMode = "default"
	// MissingKeyZero uses the zero value for missing map keys.
	MissingKeyZero MissingKeyMode = "zero"
	// MissingKeyError returns an execution error for missing map keys.
	MissingKeyError MissingKeyMode = "error"
)

var (
	// ErrTemplateLocaleNotFound is wrapped when no locale source is available
	// across the requested locale, fallback chain, and default locale.
	ErrTemplateLocaleNotFound = errors.New("email: template locale not found")
)

// MissingTemplateLocaleError describes a localized email template that could
// not be found in any searched locale.
type MissingTemplateLocaleError struct {
	Name     string
	Locale   string
	Searched []string
}

func (e *MissingTemplateLocaleError) Error() string {
	name := e.Name
	if name == "" {
		name = "template"
	}
	if len(e.Searched) == 0 {
		return fmt.Sprintf("%s: %s for locale %q", ErrTemplateLocaleNotFound, name, e.Locale)
	}
	return fmt.Sprintf("%s: %s for locale %q (searched %s)", ErrTemplateLocaleNotFound, name, e.Locale, strings.Join(e.Searched, ", "))
}

// Unwrap returns the sentinel error for errors.Is checks.
func (e *MissingTemplateLocaleError) Unwrap() error {
	return ErrTemplateLocaleNotFound
}

// LocalizedTemplateRenderer renders localized email templates with the
// standard library text/template engine. Its zero value is ready to use and
// treats missing keys as errors.
type LocalizedTemplateRenderer struct {
	LocaleContract i18n.LocaleContract
	MissingKey     MissingKeyMode
}

// NewLocalizedTemplateRenderer returns a renderer configured to fail when a
// template references a missing key.
func NewLocalizedTemplateRenderer(contract i18n.LocaleContract) LocalizedTemplateRenderer {
	return LocalizedTemplateRenderer{
		LocaleContract: contract,
		MissingKey:     MissingKeyError,
	}
}

// Render implements TemplateRenderer.
func (r LocalizedTemplateRenderer) Render(
	ctx context.Context,
	tmpl LocalizedTemplate,
	locale string,
	data map[string]any,
) (RenderedTemplate, error) {
	if err := templateContextErr(ctx); err != nil {
		return RenderedTemplate{}, err
	}

	source, resolvedLocale, searched, ok := selectTemplateSource(r.LocaleContract, tmpl, locale)
	if !ok {
		return RenderedTemplate{}, &MissingTemplateLocaleError{
			Name:     tmpl.Name,
			Locale:   locale,
			Searched: append([]string(nil), searched...),
		}
	}

	subject, err := r.renderPart(templatePartName(tmpl.Name, resolvedLocale, "subject"), source.Subject, data)
	if err != nil {
		return RenderedTemplate{}, err
	}
	if err := templateContextErr(ctx); err != nil {
		return RenderedTemplate{}, err
	}

	body, err := r.renderPart(templatePartName(tmpl.Name, resolvedLocale, "body"), source.Body, data)
	if err != nil {
		return RenderedTemplate{}, err
	}
	if err := templateContextErr(ctx); err != nil {
		return RenderedTemplate{}, err
	}

	return RenderedTemplate{
		Name:            tmpl.Name,
		RequestedLocale: locale,
		Locale:          resolvedLocale,
		SearchedLocales: append([]string(nil), searched...),
		Subject:         subject,
		Body:            body,
	}, nil
}

func (r LocalizedTemplateRenderer) renderPart(name, source string, data map[string]any) (string, error) {
	option, err := r.missingKeyOption()
	if err != nil {
		return "", err
	}
	parsed, err := texttemplate.New(name).Option(option).Parse(source)
	if err != nil {
		return "", fmt.Errorf("email: parse %s template: %w", name, err)
	}

	var rendered bytes.Buffer
	if err := parsed.Execute(&rendered, data); err != nil {
		return "", fmt.Errorf("email: execute %s template: %w", name, err)
	}
	return rendered.String(), nil
}

func (r LocalizedTemplateRenderer) missingKeyOption() (string, error) {
	switch r.MissingKey {
	case "", MissingKeyError:
		return "missingkey=error", nil
	case MissingKeyDefault:
		return "missingkey=default", nil
	case MissingKeyZero:
		return "missingkey=zero", nil
	default:
		return "", fmt.Errorf("email: unsupported missing-key mode: %q", r.MissingKey)
	}
}

func selectTemplateSource(
	contract i18n.LocaleContract,
	tmpl LocalizedTemplate,
	locale string,
) (TemplateSource, string, []string, bool) {
	searched := emailTemplateFallbackLocales(contract, tmpl.DefaultLocale, locale)
	for _, candidate := range searched {
		source, ok := tmpl.Locales[candidate]
		if ok {
			return source, candidate, searched, true
		}
	}
	return TemplateSource{}, "", searched, false
}

func emailTemplateFallbackLocales(contract i18n.LocaleContract, defaultLocale, locale string) []string {
	if defaultLocale == "" {
		defaultLocale = contract.Default
	}
	if locale == "" {
		locale = defaultLocale
	}

	seen := make(map[string]struct{})
	locales := make([]string, 0, 3)
	add := func(tag string) {
		if tag == "" {
			return
		}
		if _, ok := seen[tag]; ok {
			return
		}
		seen[tag] = struct{}{}
		locales = append(locales, tag)
	}

	walked := make(map[string]struct{})
	for locale != "" {
		if _, ok := walked[locale]; ok {
			break
		}
		walked[locale] = struct{}{}
		add(locale)

		next := localeFallback(contract, locale)
		if next == "" {
			next = parentLocale(locale)
		}
		if next == "" {
			break
		}
		locale = next
	}
	add(defaultLocale)
	return locales
}

func localeFallback(contract i18n.LocaleContract, locale string) string {
	for _, fallback := range contract.Fallbacks {
		if fallback.From == locale {
			return fallback.To
		}
	}
	return ""
}

func parentLocale(locale string) string {
	if i := strings.LastIndex(locale, "-"); i > 0 {
		return locale[:i]
	}
	return ""
}

func templatePartName(name, locale, part string) string {
	if name == "" {
		name = "template"
	}
	if locale == "" {
		return name + "." + part
	}
	return name + "." + locale + "." + part
}

func templateContextErr(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
		return nil
	}
}
