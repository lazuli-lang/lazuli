package encryption

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/base64"
	"errors"
	"testing"
)

func setupTenantKey(t *testing.T, tenantID string) string {
	t.Helper()
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		t.Fatalf("rand.Read: %v", err)
	}
	encoded := base64.StdEncoding.EncodeToString(key)
	envName := "CRYPT_KEY_TENANT_" + tenantID
	t.Setenv(envName, encoded)
	return encoded
}

func TestForReturnsCipherForTenantBinding(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	setupTenantKey(t, "org-42")
	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})

	c, err := For("@key.tenant", CtxAxes{TenantID: "org-42"})
	if err != nil {
		t.Fatalf("For: %v", err)
	}
	if c == nil {
		t.Fatal("expected cipher, got nil")
	}

	// Round-trip works.
	encrypted, err := c.Encrypt([]byte("hello"))
	if err != nil {
		t.Fatalf("Encrypt: %v", err)
	}
	plain, err := c.Decrypt(encrypted)
	if err != nil {
		t.Fatalf("Decrypt: %v", err)
	}
	if !bytes.Equal(plain, []byte("hello")) {
		t.Fatalf("round-trip mismatch: %q", plain)
	}
}

func TestForPerTenantIsolation(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	setupTenantKey(t, "org-1")
	setupTenantKey(t, "org-2")
	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})

	c1, err := For("@key.tenant", CtxAxes{TenantID: "org-1"})
	if err != nil {
		t.Fatalf("For org-1: %v", err)
	}
	c2, err := For("@key.tenant", CtxAxes{TenantID: "org-2"})
	if err != nil {
		t.Fatalf("For org-2: %v", err)
	}

	encryptedByOne, err := c1.Encrypt([]byte("sensitive"))
	if err != nil {
		t.Fatalf("Encrypt: %v", err)
	}
	// org-2 cannot decrypt org-1's ciphertext — independent keys.
	if _, err := c2.Decrypt(encryptedByOne); err == nil {
		t.Fatal("Decrypt cross-tenant succeeded; want failure")
	}
}

func TestForBindingMissingFires(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	if _, err := For("@key.tenant", CtxAxes{TenantID: "org-1"}); !errors.Is(err, ErrBindingMissing) {
		t.Fatalf("expected ErrBindingMissing, got %v", err)
	}
}

func TestForEnvKeyMissingFires(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})

	// No env var set for org-7.
	if _, err := For("@key.tenant", CtxAxes{TenantID: "org-7"}); !errors.Is(err, ErrEnvKeyMissing) {
		t.Fatalf("expected ErrEnvKeyMissing, got %v", err)
	}
}

func TestForKeyDecodeFailedFires(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	// Wrong-length key (16 bytes instead of 32).
	t.Setenv("CRYPT_KEY_TENANT_org-8", base64.StdEncoding.EncodeToString(make([]byte, 16)))
	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})

	if _, err := For("@key.tenant", CtxAxes{TenantID: "org-8"}); !errors.Is(err, ErrKeyDecodeFailed) {
		t.Fatalf("expected ErrKeyDecodeFailed, got %v", err)
	}
}

func TestForTenantIDMissingFires(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})

	// Empty TenantID.
	if _, err := For("@key.tenant", CtxAxes{}); !errors.Is(err, ErrTenantIDMissing) {
		t.Fatalf("expected ErrTenantIDMissing, got %v", err)
	}
}

func TestForGlobalAppScope(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		t.Fatalf("rand.Read: %v", err)
	}
	t.Setenv("CRYPT_KEY_APP", base64.StdEncoding.EncodeToString(key))
	Register(Binding{
		Scope:     "@key.app",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_APP",
		Axes:      nil,
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})

	c, err := For("@key.app", CtxAxes{})
	if err != nil {
		t.Fatalf("For: %v", err)
	}
	encrypted, err := c.Encrypt([]byte("global secret"))
	if err != nil {
		t.Fatalf("Encrypt: %v", err)
	}
	plain, err := c.Decrypt(encrypted)
	if err != nil {
		t.Fatalf("Decrypt: %v", err)
	}
	if !bytes.Equal(plain, []byte("global secret")) {
		t.Fatalf("mismatch: %q", plain)
	}
}

func TestForSecretsAdapterUnsupportedYet(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceSecrets,
		Template:  "vault_key_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})

	if _, err := For("@key.tenant", CtxAxes{TenantID: "org-1"}); !errors.Is(err, ErrSecretsAdapterMissing) {
		t.Fatalf("expected ErrSecretsAdapterMissing, got %v", err)
	}
}

func TestForCachesCipherPerScopeAndAxis(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	setupTenantKey(t, "org-1")
	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})

	a, _ := For("@key.tenant", CtxAxes{TenantID: "org-1"})
	b, _ := For("@key.tenant", CtxAxes{TenantID: "org-1"})
	if a != b {
		t.Fatal("expected cipher cache hit for same scope + axes")
	}
}

func TestForCtxLiftsTenantViaHook(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	setupTenantKey(t, "ctx-tenant")
	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})

	// Install a hook that lifts "tenant_from_ctx" → "ctx-tenant".
	previous := ctxAxesHook
	t.Cleanup(func() { ctxAxesHook = previous })
	RegisterCtxAxes(func(_ context.Context) (string, string) {
		return "ctx-tenant", ""
	})

	c, err := ForCtx(context.Background(), "@key.tenant", "")
	if err != nil {
		t.Fatalf("ForCtx: %v", err)
	}
	if c == nil {
		t.Fatal("expected cipher")
	}
}

func TestBindingsSnapshot(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	Register(Binding{Scope: "@key.app", Source: SourceEnv, Template: "K1"})
	Register(Binding{Scope: "@key.tenant", Source: SourceEnv, Template: "K2"})

	snapshot := Bindings()
	if len(snapshot) != 2 {
		t.Fatalf("expected 2 bindings, got %d", len(snapshot))
	}
}

func TestRegisterIdempotentReplacesAndInvalidatesCipher(t *testing.T) {
	Reset()
	t.Cleanup(Reset)

	setupTenantKey(t, "org-1")
	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})
	first, _ := For("@key.tenant", CtxAxes{TenantID: "org-1"})

	// Re-register with the same shape — cache invalidated.
	Register(Binding{
		Scope:     "@key.tenant",
		Source:    SourceEnv,
		Template:  "CRYPT_KEY_TENANT_{tenant_id}",
		Axes:      []TemplateAxis{AxisTenantID},
		Algorithm: AlgorithmAES256GCM,
		Rotation:  RotationManual,
	})
	second, _ := For("@key.tenant", CtxAxes{TenantID: "org-1"})

	if first == second {
		t.Fatal("expected Register to invalidate cached cipher")
	}
}
