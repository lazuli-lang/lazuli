package lazuli

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/base64"
	"errors"
	"testing"

	"lazuli.dev/runtime/lazuli/encryption"
)

// setupTenantEncryption installs an AES-256-GCM binding scoped to the
// per-tenant template the canonical fixture authors in `app.lzi`
// (`CRYPT_KEY_TENANT_{tenant_id}`). The cleanup runs `encryption.Reset`
// so a follow-on test starts with an empty registry — registration is
// process-global.
func setupTenantEncryption(t *testing.T, tenantID string) {
	t.Helper()
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		t.Fatalf("rand.Read: %v", err)
	}
	t.Setenv("CRYPT_KEY_TENANT_"+tenantID, base64.StdEncoding.EncodeToString(key))

	encryption.Reset()
	t.Cleanup(encryption.Reset)
	encryption.Register(encryption.Binding{
		Scope:     "@key.tenant",
		Source:    encryption.SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []encryption.TemplateAxis{encryption.AxisTenantID},
		Algorithm: encryption.AlgorithmAES256GCM,
		Rotation:  encryption.RotationManual,
	})

	// Bind the runtime ctx → encryption hook so `encryption.ForCtx`
	// resolves the tenant axis from `*lazuli.Ctx.Tenant.OrgID`. The
	// canonical fixture uses textual tenant IDs in the env var name;
	// for the test we mirror the same shape.
	encryption.RegisterCtxAxes(func(ctx context.Context) (string, string) {
		if c, ok := ctx.(*Ctx); ok && c != nil && c.Tenant != nil {
			return tenantID, ""
		}
		return "", ""
	})
}

// fakeCustomer mirrors the shape that codegen emits for an encrypted
// resource: a single optional `@cap.Encrypted` field. The local type
// keeps the unit test independent of any generated package.
type fakeCustomer struct {
	ID         ID
	ExternalID *EncryptedRef
}

// decryptFakeCustomer mimics the generated `DecryptCustomer` helper —
// in-place pt-substitution for every server-readable encrypted field.
func decryptFakeCustomer(ctx *Ctx, row *fakeCustomer) error {
	if row.ExternalID != nil && len(*row.ExternalID) > 0 {
		cipher, err := encryption.ForCtx(ctx, "@key.tenant", "")
		if err != nil {
			return err
		}
		pt, err := cipher.Decrypt(*row.ExternalID)
		if err != nil {
			return err
		}
		*row.ExternalID = pt
	}
	return nil
}

// TestEncryptColumnValuesRoundTrip walks a synthetic (cols, values)
// pair through `encryptColumnValues` then verifies that `Cipher.Decrypt`
// recovers the plaintext. Mirrors the lifecycle `applyCreates` runs:
// bindings resolve to plaintext, runtime encrypts in place, INSERT
// stores ciphertext, SELECT scans, runtime calls Decrypt callback.
func TestEncryptColumnValuesRoundTrip(t *testing.T) {
	setupTenantEncryption(t, "42")
	ctx := &Ctx{Context: context.Background(), Tenant: &Tenant{OrgID: 42}}

	res := &resourceErased{
		Name:    "customer",
		Tenancy: TenancyOrg,
		EncryptedColumns: map[string]string{
			"external_id": "@key.tenant",
		},
		Decrypt: func(ctx *Ctx, row any) error {
			return decryptFakeCustomer(ctx, row.(*fakeCustomer))
		},
	}

	plaintext := []byte("acct-9001")
	cols := []string{`"external_id"`, `"name"`}
	values := []any{append([]byte(nil), plaintext...), "Alice"}

	if err := encryptColumnValues(ctx, res, cols, values); err != nil {
		t.Fatalf("encryptColumnValues: %v", err)
	}

	encryptedBytes, ok := values[0].([]byte)
	if !ok {
		t.Fatalf("expected []byte at index 0, got %T", values[0])
	}
	if bytes.Equal(encryptedBytes, plaintext) {
		t.Fatalf("expected ciphertext to differ from plaintext; both = %q", plaintext)
	}
	if values[1] != "Alice" {
		t.Fatalf("non-encrypted column was mutated: got %v", values[1])
	}

	// Simulate the SELECT-side decrypt callback. The runtime calls it
	// against `&out` (a `*fakeCustomer` here); the callback substitutes
	// plaintext back into the struct.
	enc := encryptedBytes
	row := &fakeCustomer{ID: 1, ExternalID: &enc}
	if err := decryptScannedRow(ctx, res, row); err != nil {
		t.Fatalf("decryptScannedRow: %v", err)
	}
	if row.ExternalID == nil || !bytes.Equal(*row.ExternalID, plaintext) {
		t.Fatalf("decrypt did not restore plaintext: got %v want %q",
			row.ExternalID, plaintext)
	}
}

// TestEncryptColumnValuesNoEncryptedColumns confirms the fast-path
// when a resource has no encrypted fields — the helper must not call
// `encryption.ForCtx` (which would otherwise return ErrBindingMissing
// since the registry is empty for this test).
func TestEncryptColumnValuesNoEncryptedColumns(t *testing.T) {
	encryption.Reset()
	t.Cleanup(encryption.Reset)
	ctx := &Ctx{Context: context.Background()}

	res := &resourceErased{Name: "no_encryption_resource"}
	cols := []string{`"id"`, `"name"`}
	values := []any{int64(1), "Alice"}

	if err := encryptColumnValues(ctx, res, cols, values); err != nil {
		t.Fatalf("encryptColumnValues should be a no-op: %v", err)
	}
	if values[0] != int64(1) || values[1] != "Alice" {
		t.Fatalf("values were mutated despite empty EncryptedColumns: %v", values)
	}
}

// TestDecryptScannedRowSkipsWhenCallbackNil mirrors the same fast-path
// on the read side — resources without a Decrypt callback must short
// circuit before any reflection / type-assertion runs.
func TestDecryptScannedRowSkipsWhenCallbackNil(t *testing.T) {
	ctx := &Ctx{Context: context.Background()}
	res := &resourceErased{Name: "plain"}

	type other struct {
		ID ID
	}
	if err := decryptScannedRow(ctx, res, &other{ID: 7}); err != nil {
		t.Fatalf("expected nil error on resource without Decrypt callback: %v", err)
	}
}

// TestEncryptColumnValuesPropagatesBindingMissing verifies that a
// resource declaring an encrypted column whose `@key.<scope>` has no
// registered binding fails fast at the SQL boundary (the doctor-
// catchable case slipped through to runtime — should still error
// cleanly rather than silently writing plaintext).
func TestEncryptColumnValuesPropagatesBindingMissing(t *testing.T) {
	encryption.Reset()
	t.Cleanup(encryption.Reset)
	ctx := &Ctx{Context: context.Background()}

	res := &resourceErased{
		Name: "customer",
		EncryptedColumns: map[string]string{
			"external_id": "@key.nope",
		},
	}
	cols := []string{`"external_id"`}
	values := []any{[]byte("plaintext")}

	err := encryptColumnValues(ctx, res, cols, values)
	if err == nil {
		t.Fatal("expected ErrBindingMissing, got nil")
	}
	if !errors.Is(err, encryption.ErrBindingMissing) {
		t.Fatalf("expected ErrBindingMissing, got %v", err)
	}
	// Even on failure, the value should NOT have been replaced with
	// partial ciphertext — fail-closed semantics.
	if got, ok := values[0].([]byte); !ok || !bytes.Equal(got, []byte("plaintext")) {
		t.Fatalf("values[0] mutated despite error: %v", values[0])
	}
}
