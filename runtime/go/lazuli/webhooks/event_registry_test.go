package webhooks

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestWebhookEventRegistryRegisterLookupAndDocSummaries(t *testing.T) {
	var registry WebhookEventRegistry

	descriptors := []WebhookEventDescriptor{
		{
			Feature:          " crm ",
			Name:             " customer.upserted ",
			PayloadSchemaRef: " crm.CustomerUpsertedPayload ",
			Version: WebhookEventVersion{
				Version:      " 1.0.0 ",
				IntroducedIn: " 0.13.0 ",
			},
			Summary: " Customer was upserted ",
		},
		{
			Feature:          "billing",
			Name:             "invoice.paid",
			PayloadSchemaRef: "billing.InvoicePaidPayload",
			Version: WebhookEventVersion{
				Version:      "1.1.0",
				IntroducedIn: "0.12.0",
				DeprecatedIn: "0.14.0",
				ReplacedBy:   "invoice.settled",
			},
			Summary: "Invoice paid",
		},
	}
	for _, descriptor := range descriptors {
		if err := registry.Register(descriptor); err != nil {
			t.Fatalf("Register(%q) error = %v", descriptor.Name, err)
		}
	}

	customer, ok := registry.Lookup(" customer.upserted ")
	if !ok {
		t.Fatal("Lookup() ok = false, want true")
	}
	if customer.Feature != "crm" || customer.PayloadSchemaRef.String() != "crm.CustomerUpsertedPayload" ||
		customer.Version.Version != "1.0.0" || customer.Version.IntroducedIn != "0.13.0" ||
		customer.Summary != "Customer was upserted" {
		t.Fatalf("Lookup() descriptor was not normalized: %#v", customer)
	}

	if descriptors[0].Feature != " crm " || descriptors[0].PayloadSchemaRef != " crm.CustomerUpsertedPayload " {
		t.Fatal("Register() mutated input descriptor")
	}

	events := registry.Events()
	if got, want := webhookEventNames(events), []string{"invoice.paid", "customer.upserted"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Events() names = %v, want %v", got, want)
	}

	summaries := registry.DocSummaries()
	want := []WebhookEventDocSummary{
		{
			Feature:          "billing",
			Name:             "invoice.paid",
			PayloadSchemaRef: "billing.InvoicePaidPayload",
			Version:          "1.1.0",
			IntroducedIn:     "0.12.0",
			Deprecated:       true,
			DeprecatedIn:     "0.14.0",
			ReplacedBy:       "invoice.settled",
			Summary:          "Invoice paid",
		},
		{
			Feature:          "crm",
			Name:             "customer.upserted",
			PayloadSchemaRef: "crm.CustomerUpsertedPayload",
			Version:          "1.0.0",
			IntroducedIn:     "0.13.0",
			Summary:          "Customer was upserted",
		},
	}
	if !reflect.DeepEqual(summaries, want) {
		t.Fatalf("DocSummaries() = %#v, want %#v", summaries, want)
	}
}

func TestValidateWebhookEventDescriptorsRejectsInvalidAndDuplicateMetadata(t *testing.T) {
	err := ValidateWebhookEventDescriptors([]WebhookEventDescriptor{
		{
			Name:             "invoice.paid",
			PayloadSchemaRef: "billing.InvoicePaidPayload",
			Version:          WebhookEventVersion{Version: "1.0.0"},
		},
		{
			Name:             " invoice.paid ",
			PayloadSchemaRef: "billing.InvoicePaidPayloadV2",
			Version:          WebhookEventVersion{Version: "2.0.0"},
		},
		{
			Name:             "customer\ncreated",
			PayloadSchemaRef: "crm.Customer Created",
			Version: WebhookEventVersion{
				Version:      "",
				IntroducedIn: "0.13.0 beta",
				ReplacedBy:   "customer.created",
			},
			Summary: "bad\x00summary",
		},
	})

	for _, wantErr := range []error{
		ErrDuplicateWebhookEventDescriptor,
		ErrInvalidWebhookEventDescriptor,
	} {
		if !errors.Is(err, wantErr) {
			t.Fatalf("ValidateWebhookEventDescriptors() error = %v, want %v", err, wantErr)
		}
	}
	for _, want := range []string{
		`event[1] "invoice.paid" also appears at event[0]`,
		"event[2].name",
		"event[2].payload_schema_ref",
		"event[2].version.version",
		"event[2].version.introduced_in",
		"event[2].summary",
	} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("ValidateWebhookEventDescriptors() error = %q, want substring %q", err.Error(), want)
		}
	}
}

func TestWebhookEventDocSummariesAreDeterministic(t *testing.T) {
	descriptors := []WebhookEventDescriptor{
		{
			Feature:          "crm",
			Name:             "customer.deleted",
			PayloadSchemaRef: "crm.CustomerDeletedPayload",
			Version:          WebhookEventVersion{Version: "1.0.0"},
			Summary:          "Customer deleted",
		},
		{
			Feature:          "billing",
			Name:             "invoice.paid",
			PayloadSchemaRef: "billing.InvoicePaidPayload",
			Version:          WebhookEventVersion{Version: "1.0.0"},
			Summary:          "Invoice paid",
		},
		{
			Feature:          "crm",
			Name:             "customer.created",
			PayloadSchemaRef: "crm.CustomerCreatedPayload",
			Version:          WebhookEventVersion{Version: "1.0.0"},
			Summary:          "Customer | created",
		},
	}

	first, err := WebhookEventDocSummaries(descriptors)
	if err != nil {
		t.Fatalf("WebhookEventDocSummaries() error = %v", err)
	}

	reversed := []WebhookEventDescriptor{descriptors[2], descriptors[1], descriptors[0]}
	second, err := WebhookEventDocSummaries(reversed)
	if err != nil {
		t.Fatalf("WebhookEventDocSummaries() reversed error = %v", err)
	}

	if !reflect.DeepEqual(first, second) {
		t.Fatalf("WebhookEventDocSummaries() changed after input reorder\nfirst:  %#v\nsecond: %#v", first, second)
	}
	if got, want := webhookEventSummaryNames(first), []string{"invoice.paid", "customer.created", "customer.deleted"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("summary names = %v, want %v", got, want)
	}
}

func TestWebhookEventRegistryRejectsDuplicateAndNilRegister(t *testing.T) {
	var nilRegistry *WebhookEventRegistry
	if err := nilRegistry.Register(WebhookEventDescriptor{}); !errors.Is(err, ErrNilWebhookEventRegistry) {
		t.Fatalf("nil Register() error = %v, want ErrNilWebhookEventRegistry", err)
	}

	registry := NewWebhookEventRegistry()
	descriptor := WebhookEventDescriptor{
		Name:             "invoice.paid",
		PayloadSchemaRef: "billing.InvoicePaidPayload",
		Version:          WebhookEventVersion{Version: "1.0.0"},
	}
	if err := registry.Register(descriptor); err != nil {
		t.Fatalf("first Register() error = %v", err)
	}
	if err := registry.Register(descriptor); !errors.Is(err, ErrDuplicateWebhookEventDescriptor) {
		t.Fatalf("duplicate Register() error = %v, want ErrDuplicateWebhookEventDescriptor", err)
	}

	if _, ok := registry.Lookup("missing"); ok {
		t.Fatal("Lookup() missing ok = true, want false")
	}
}

func webhookEventNames(descriptors []WebhookEventDescriptor) []string {
	names := make([]string, 0, len(descriptors))
	for _, descriptor := range descriptors {
		names = append(names, descriptor.Name)
	}
	return names
}

func webhookEventSummaryNames(summaries []WebhookEventDocSummary) []string {
	names := make([]string, 0, len(summaries))
	for _, summary := range summaries {
		names = append(names, summary.Name)
	}
	return names
}
