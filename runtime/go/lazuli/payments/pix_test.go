package payments_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/payments"
)

func TestBuildPIXDescriptorPlanNormalizesDescriptor(t *testing.T) {
	t.Parallel()

	createdAt := time.Date(2026, 5, 13, 12, 0, 0, 0, time.UTC)
	expiresAt := createdAt.Add(30 * time.Minute)
	plan, err := payments.BuildPIXDescriptorPlan(payments.PIXDescriptor{
		KeyType:      " EVP ",
		Key:          " pix-key-placeholder ",
		MerchantName: " lazuli  store ",
		MerchantCity: " sao paulo ",
		TxID:         " Txn123 ",
		Amount:       payments.Money{Amount: 2590, Currency: " brl "},
		Expiration: payments.PIXExpiration{
			CreatedAt: createdAt,
			ExpiresAt: expiresAt,
		},
		Metadata: map[string]string{
			"note":        " order 123 ",
			"callbackURL": "https://example.invalid/pix/callback",
			"token_hint":  "not-a-real-token",
		},
	})
	if err != nil {
		t.Fatalf("BuildPIXDescriptorPlan failed: %v", err)
	}

	desc := plan.Descriptor
	if desc.KeyType != payments.PIXKeyTypeRandom {
		t.Fatalf("key type = %q", desc.KeyType)
	}
	if desc.MerchantName != "LAZULI STORE" || desc.MerchantCity != "SAO PAULO" {
		t.Fatalf("merchant = %q / %q", desc.MerchantName, desc.MerchantCity)
	}
	if desc.TxID != "Txn123" {
		t.Fatalf("txid = %q", desc.TxID)
	}
	if desc.Amount != (payments.Money{Amount: 2590, Currency: "BRL"}) {
		t.Fatalf("amount = %+v", desc.Amount)
	}
	if desc.Expiration.TTL != 30*time.Minute {
		t.Fatalf("ttl = %s", desc.Expiration.TTL)
	}
	if desc.Metadata["note"] != "order 123" {
		t.Fatalf("metadata note = %q", desc.Metadata["note"])
	}
	if plan.Summary.Key == desc.Key {
		t.Fatal("summary key was not redacted")
	}
	if plan.Summary.Metadata["note"] != "order 123" {
		t.Fatalf("summary note = %q", plan.Summary.Metadata["note"])
	}
	if plan.Summary.Metadata["callbackURL"] != "[redacted]" {
		t.Fatalf("summary callbackURL = %q", plan.Summary.Metadata["callbackURL"])
	}
	if plan.Summary.Metadata["token_hint"] != "[redacted]" {
		t.Fatalf("summary token_hint = %q", plan.Summary.Metadata["token_hint"])
	}
}

func TestValidatePIXDescriptorRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	valid := payments.PIXDescriptor{
		KeyType:      payments.PIXKeyTypeEmail,
		Key:          "pix-key-placeholder",
		MerchantName: "Lazuli Store",
		MerchantCity: "Sao Paulo",
		TxID:         "Txn123",
		Amount:       payments.Money{Amount: 2590, Currency: "BRL"},
	}

	tests := []struct {
		name string
		mut  func(*payments.PIXDescriptor)
	}{
		{
			name: "unknown key type",
			mut: func(desc *payments.PIXDescriptor) {
				desc.KeyType = "alias"
			},
		},
		{
			name: "missing key",
			mut: func(desc *payments.PIXDescriptor) {
				desc.Key = " "
			},
		},
		{
			name: "merchant name too long",
			mut: func(desc *payments.PIXDescriptor) {
				desc.MerchantName = "12345678901234567890123456"
			},
		},
		{
			name: "merchant city too long",
			mut: func(desc *payments.PIXDescriptor) {
				desc.MerchantCity = "1234567890123456"
			},
		},
		{
			name: "missing txid",
			mut: func(desc *payments.PIXDescriptor) {
				desc.TxID = ""
			},
		},
		{
			name: "txid too long",
			mut: func(desc *payments.PIXDescriptor) {
				desc.TxID = "12345678901234567890123456"
			},
		},
		{
			name: "txid non portable",
			mut: func(desc *payments.PIXDescriptor) {
				desc.TxID = "txn-123"
			},
		},
		{
			name: "non positive amount",
			mut: func(desc *payments.PIXDescriptor) {
				desc.Amount.Amount = 0
			},
		},
		{
			name: "non brl currency",
			mut: func(desc *payments.PIXDescriptor) {
				desc.Amount.Currency = "USD"
			},
		},
		{
			name: "incoherent expiration",
			mut: func(desc *payments.PIXDescriptor) {
				desc.Expiration = payments.PIXExpiration{
					CreatedAt: time.Date(2026, 5, 13, 13, 0, 0, 0, time.UTC),
					ExpiresAt: time.Date(2026, 5, 13, 12, 0, 0, 0, time.UTC),
				}
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			desc := valid
			tt.mut(&desc)
			err := payments.ValidatePIXDescriptor(desc)
			if !errors.Is(err, payments.ErrPIXDescriptorInvalid) {
				t.Fatalf("ValidatePIXDescriptor error = %v", err)
			}
		})
	}
}

func TestPIXKeyTypeValid(t *testing.T) {
	t.Parallel()

	for _, keyType := range []payments.PIXKeyType{
		payments.PIXKeyTypeCPF,
		payments.PIXKeyTypeCNPJ,
		payments.PIXKeyTypeEmail,
		payments.PIXKeyTypePhone,
		payments.PIXKeyTypeRandom,
	} {
		if !keyType.Valid() {
			t.Fatalf("%q should be valid", keyType)
		}
	}
	if payments.PIXKeyType("unknown").Valid() {
		t.Fatal("unknown key type should be invalid")
	}
}

func TestPIXDescriptorSafeSummaryRedactsShortKeysAndURLValues(t *testing.T) {
	t.Parallel()

	summary := (payments.PIXDescriptor{
		KeyType:      payments.PIXKeyTypeCPF,
		Key:          "1234",
		MerchantName: "Lazuli Store",
		MerchantCity: "Sao Paulo",
		TxID:         "Txn123",
		Amount:       payments.Money{Amount: 100, Currency: "BRL"},
		Metadata: map[string]string{
			"plain": "https://example.invalid/not-public",
			"label": "visible",
		},
	}).SafeSummary()

	if summary.Key != "[redacted]" {
		t.Fatalf("key = %q", summary.Key)
	}
	if summary.Metadata["plain"] != "[redacted]" {
		t.Fatalf("plain metadata = %q", summary.Metadata["plain"])
	}
	if summary.Metadata["label"] != "visible" {
		t.Fatalf("label metadata = %q", summary.Metadata["label"])
	}
}
