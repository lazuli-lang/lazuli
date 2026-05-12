package notifications_test

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/notifications"
)

func TestTextTemplateRendererRendersSubjectAndBody(t *testing.T) {
	t.Parallel()

	renderer := notifications.NewTextTemplateRenderer()
	rendered, err := renderer.Render(context.Background(), notifications.Template{
		Name:    "welcome",
		Subject: "Welcome, {{.Name}}",
		Body:    "Hi {{.Name}}, your org is {{.Org}}.",
	}, map[string]any{
		"Name": "Ada",
		"Org":  "acme",
	})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if rendered.Subject != "Welcome, Ada" {
		t.Fatalf("Subject = %q, want %q", rendered.Subject, "Welcome, Ada")
	}
	if rendered.Body != "Hi Ada, your org is acme." {
		t.Fatalf("Body = %q", rendered.Body)
	}
}

func TestTextTemplateRendererMissingKeyErrorMode(t *testing.T) {
	t.Parallel()

	renderer := notifications.TextTemplateRenderer{}
	_, err := renderer.Render(context.Background(), notifications.Template{
		Name:    "welcome",
		Subject: "Welcome",
		Body:    "Hi {{.Missing}}",
	}, map[string]any{})
	if err == nil {
		t.Fatal("expected missing-key error")
	}
	if !strings.Contains(err.Error(), "execute welcome.body template") {
		t.Fatalf("error = %q", err)
	}
}

func TestTextTemplateRendererDefaultMissingKeyMode(t *testing.T) {
	t.Parallel()

	renderer := notifications.TextTemplateRenderer{
		MissingKey: notifications.MissingKeyDefault,
	}
	rendered, err := renderer.Render(context.Background(), notifications.Template{
		Subject: "Notice",
		Body:    "Hi {{.Missing}}",
	}, map[string]any{})
	if err != nil {
		t.Fatalf("Render: %v", err)
	}
	if !strings.Contains(rendered.Body, "<no value>") {
		t.Fatalf("Body = %q, want missing key placeholder", rendered.Body)
	}
}

func TestTextTemplateRendererHonorsCanceledContext(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := notifications.NewTextTemplateRenderer().Render(ctx, notifications.Template{
		Subject: "Welcome",
		Body:    "Body",
	}, nil)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Render error = %v, want context.Canceled", err)
	}
}

func TestDigestTemplateDataMergeLastWriteWins(t *testing.T) {
	t.Parallel()

	base := map[string]any{
		"Feature": "customer",
		"Status":  "initial",
	}
	payloads := []map[string]any{
		{"Status": "active", "Name": "Ada"},
		{"Status": "archived", "Email": "ada@example.com"},
	}

	got := notifications.MergeDigestTemplateData(base, payloads)

	if got["Feature"] != "customer" {
		t.Fatalf("Feature = %v", got["Feature"])
	}
	if got["Status"] != "archived" {
		t.Fatalf("Status = %v, want last payload value", got["Status"])
	}
	if got["Name"] != "Ada" || got["Email"] != "ada@example.com" {
		t.Fatalf("merged payload fields = %+v", got)
	}
	if base["Status"] != "initial" {
		t.Fatalf("base map was mutated: %+v", base)
	}
	assertDigestCount(t, got, 2)
}

func TestDigestTemplateDataAppendExposesCopiedItems(t *testing.T) {
	t.Parallel()

	payloads := []map[string]any{
		{"Name": "Ada"},
		{"Name": "Grace"},
	}
	got := notifications.AppendDigestTemplateData(map[string]any{"Feature": "customer"}, payloads)
	payloads[0]["Name"] = "mutated"

	digest := digestData(t, got)
	items, ok := digest["Items"].([]map[string]any)
	if !ok {
		t.Fatalf("Digest.Items = %T, want []map[string]any", digest["Items"])
	}
	want := []map[string]any{
		{"Name": "Ada"},
		{"Name": "Grace"},
	}
	if !reflect.DeepEqual(items, want) {
		t.Fatalf("Digest.Items = %#v, want %#v", items, want)
	}
	assertDigestCount(t, got, 2)
}

func TestDigestTemplateDataRejectsUnsupportedStrategy(t *testing.T) {
	t.Parallel()

	_, err := notifications.DigestTemplateData(notifications.DigestStrategy("unknown"), nil, nil)
	if !errors.Is(err, notifications.ErrDigestStrategyUnsupported) {
		t.Fatalf("DigestTemplateData error = %v", err)
	}
}

func TestRenderDigestAppend(t *testing.T) {
	t.Parallel()

	rendered, err := notifications.RenderDigest(
		context.Background(),
		notifications.NewTextTemplateRenderer(),
		notifications.Template{
			Name:    "digest",
			Subject: "{{.Digest.Count}} customer changes",
			Body:    "{{range .Digest.Items}}{{.Name}} {{end}}",
		},
		notifications.DigestStrategyAppend,
		nil,
		[]map[string]any{
			{"Name": "Ada"},
			{"Name": "Grace"},
		},
	)
	if err != nil {
		t.Fatalf("RenderDigest: %v", err)
	}
	if rendered.Subject != "2 customer changes" {
		t.Fatalf("Subject = %q", rendered.Subject)
	}
	if rendered.Body != "Ada Grace " {
		t.Fatalf("Body = %q", rendered.Body)
	}
}

func digestData(t *testing.T, data map[string]any) map[string]any {
	t.Helper()

	digest, ok := data[notifications.DigestTemplateDataKey].(map[string]any)
	if !ok {
		t.Fatalf("Digest = %T, want map[string]any", data[notifications.DigestTemplateDataKey])
	}
	return digest
}

func assertDigestCount(t *testing.T, data map[string]any, want int) {
	t.Helper()

	digest := digestData(t, data)
	if got := digest["Count"]; got != want {
		t.Fatalf("Digest.Count = %v, want %d", got, want)
	}
}
