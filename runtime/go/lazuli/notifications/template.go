package notifications

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	texttemplate "text/template"
)

// TemplateRenderer renders notification template text into channel-neutral
// subject and body strings.
type TemplateRenderer interface {
	Render(ctx context.Context, tmpl Template, data map[string]any) (RenderedTemplate, error)
}

// Template contains the template source for a notification message.
type Template struct {
	Name    string
	Subject string
	Body    string
}

// RenderedTemplate is the channel-neutral output of rendering a Template.
type RenderedTemplate struct {
	Subject string
	Body    string
}

// MissingKeyMode controls how TextTemplateRenderer handles missing map keys.
type MissingKeyMode string

const (
	// MissingKeyDefault uses text/template's default missing-key behavior.
	MissingKeyDefault MissingKeyMode = "default"
	// MissingKeyZero uses the zero value for missing map keys.
	MissingKeyZero MissingKeyMode = "zero"
	// MissingKeyError returns an execution error for missing map keys.
	MissingKeyError MissingKeyMode = "error"
)

// DigestTemplateDataKey is the top-level data key used by digest helpers for
// provider-neutral digest metadata. Append helpers populate
// `.Digest.Items`; all digest helpers populate `.Digest.Count`.
const DigestTemplateDataKey = "Digest"

const (
	digestItemsKey = "Items"
	digestCountKey = "Count"
)

var (
	// ErrDigestStrategyUnsupported is returned when a digest helper receives a
	// strategy outside the closed merge/append catalog.
	ErrDigestStrategyUnsupported = errors.New("notifications: unsupported digest template strategy")
	errTemplateRendererNil       = errors.New("notifications: template renderer is nil")
)

// TextTemplateRenderer renders notification templates with the standard
// library text/template engine. Its zero value is ready to use and treats
// missing keys as errors.
type TextTemplateRenderer struct {
	MissingKey MissingKeyMode
}

// NewTextTemplateRenderer returns a stdlib renderer configured to fail when a
// template references a missing key.
func NewTextTemplateRenderer() TextTemplateRenderer {
	return TextTemplateRenderer{MissingKey: MissingKeyError}
}

// Render implements TemplateRenderer.
func (r TextTemplateRenderer) Render(
	ctx context.Context,
	tmpl Template,
	data map[string]any,
) (RenderedTemplate, error) {
	if err := ctx.Err(); err != nil {
		return RenderedTemplate{}, err
	}

	subject, err := r.renderPart(templatePartName(tmpl.Name, "subject"), tmpl.Subject, data)
	if err != nil {
		return RenderedTemplate{}, err
	}
	if err := ctx.Err(); err != nil {
		return RenderedTemplate{}, err
	}

	body, err := r.renderPart(templatePartName(tmpl.Name, "body"), tmpl.Body, data)
	if err != nil {
		return RenderedTemplate{}, err
	}
	if err := ctx.Err(); err != nil {
		return RenderedTemplate{}, err
	}

	return RenderedTemplate{
		Subject: subject,
		Body:    body,
	}, nil
}

func (r TextTemplateRenderer) renderPart(name, source string, data map[string]any) (string, error) {
	option, err := r.missingKeyOption()
	if err != nil {
		return "", err
	}
	parsed, err := texttemplate.New(name).Option(option).Parse(source)
	if err != nil {
		return "", fmt.Errorf("notifications: parse %s template: %w", name, err)
	}

	var rendered bytes.Buffer
	if err := parsed.Execute(&rendered, data); err != nil {
		return "", fmt.Errorf("notifications: execute %s template: %w", name, err)
	}
	return rendered.String(), nil
}

func (r TextTemplateRenderer) missingKeyOption() (string, error) {
	switch r.MissingKey {
	case "", MissingKeyError:
		return "missingkey=error", nil
	case MissingKeyDefault:
		return "missingkey=default", nil
	case MissingKeyZero:
		return "missingkey=zero", nil
	default:
		return "", fmt.Errorf("notifications: unsupported missing-key mode: %q", r.MissingKey)
	}
}

func templatePartName(name, part string) string {
	if name == "" {
		return part
	}
	return name + "." + part
}

// DigestTemplateData builds template data for the requested digest strategy.
// Empty strategy defaults to DigestStrategyMerge, matching the notification
// contract default.
func DigestTemplateData(
	strategy DigestStrategy,
	base map[string]any,
	payloads []map[string]any,
) (map[string]any, error) {
	switch strategy {
	case "", DigestStrategyMerge:
		return MergeDigestTemplateData(base, payloads), nil
	case DigestStrategyAppend:
		return AppendDigestTemplateData(base, payloads), nil
	default:
		return nil, fmt.Errorf("%w: %s", ErrDigestStrategyUnsupported, strategy)
	}
}

// MergeDigestTemplateData builds digest template data by shallow-copying base
// and then applying every payload in order. Later payloads win on duplicate
// keys. The returned map includes `.Digest.Count`.
func MergeDigestTemplateData(base map[string]any, payloads []map[string]any) map[string]any {
	out := cloneTemplateData(base)
	for _, payload := range payloads {
		for key, value := range payload {
			out[key] = value
		}
	}
	out[DigestTemplateDataKey] = map[string]any{
		digestCountKey: len(payloads),
	}
	return out
}

// AppendDigestTemplateData builds digest template data by shallow-copying base
// and exposing copied payloads under `.Digest.Items`. The returned map also
// includes `.Digest.Count`.
func AppendDigestTemplateData(base map[string]any, payloads []map[string]any) map[string]any {
	out := cloneTemplateData(base)
	out[DigestTemplateDataKey] = map[string]any{
		digestItemsKey: cloneDigestPayloads(payloads),
		digestCountKey: len(payloads),
	}
	return out
}

// RenderDigest renders a template after first applying the requested digest
// strategy to the supplied base data and payloads.
func RenderDigest(
	ctx context.Context,
	renderer TemplateRenderer,
	tmpl Template,
	strategy DigestStrategy,
	base map[string]any,
	payloads []map[string]any,
) (RenderedTemplate, error) {
	if renderer == nil {
		return RenderedTemplate{}, errTemplateRendererNil
	}
	data, err := DigestTemplateData(strategy, base, payloads)
	if err != nil {
		return RenderedTemplate{}, err
	}
	return renderer.Render(ctx, tmpl, data)
}

func cloneTemplateData(data map[string]any) map[string]any {
	out := make(map[string]any, len(data))
	for key, value := range data {
		out[key] = value
	}
	return out
}

func cloneDigestPayloads(payloads []map[string]any) []map[string]any {
	items := make([]map[string]any, 0, len(payloads))
	for _, payload := range payloads {
		items = append(items, cloneTemplateData(payload))
	}
	return items
}
