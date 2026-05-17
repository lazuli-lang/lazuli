package lazuli

import (
	"context"
	"strings"
	"testing"
)

func TestInstallConstantManifestRejectsMissingRequired(t *testing.T) {
	defer resetManifestForTest()
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("expected panic on missing required env var")
		}
		msg, ok := r.(string)
		if !ok {
			t.Fatalf("expected string panic, got %T: %v", r, r)
		}
		if !strings.Contains(msg, "constants.MISSING_TEST_VAR") {
			t.Fatalf("panic message should name missing var, got: %s", msg)
		}
	}()
	InstallConstantManifest([]string{"MISSING_TEST_VAR"}, nil)
}

func TestInstallConstantManifestPopulatesFromEnv(t *testing.T) {
	defer resetManifestForTest()
	t.Setenv("TEST_API_URL", "https://api.example.com")
	t.Setenv("TEST_API_KEY", "sk-abc123")

	m := InstallConstantManifest(
		[]string{"TEST_API_URL"},
		[]string{"TEST_API_KEY"},
	)
	if m == nil {
		t.Fatal("expected non-nil manifest")
	}
}

func TestConstantReturnsManifestValue(t *testing.T) {
	defer resetManifestForTest()
	t.Setenv("TEST_API_URL", "https://api.example.com")
	InstallConstantManifest([]string{"TEST_API_URL"}, nil)

	got := ResolveConstant(context.Background(), "TEST_API_URL")
	if got != "https://api.example.com" {
		t.Fatalf("Constant returned %q, want %q", got, "https://api.example.com")
	}
}

func TestConstantPanicsBeforeInstall(t *testing.T) {
	defer resetManifestForTest()
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("expected panic on ResolveConstant call before install")
		}
	}()
	ResolveConstant(context.Background(), "ANYTHING")
}

func TestConstantPanicsOnUnknownName(t *testing.T) {
	defer resetManifestForTest()
	t.Setenv("KNOWN_VAR", "value")
	InstallConstantManifest([]string{"KNOWN_VAR"}, nil)
	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("expected panic on unknown constant name")
		}
		msg, _ := r.(string)
		if !strings.Contains(msg, "codegen drift") {
			t.Fatalf("panic should mention codegen drift, got: %s", msg)
		}
	}()
	ResolveConstant(context.Background(), "UNKNOWN_VAR")
}

func TestSecretSeparateFromConstant(t *testing.T) {
	defer resetManifestForTest()
	t.Setenv("MY_API_KEY", "sk-secret")
	t.Setenv("MY_URL", "https://example.com")
	InstallConstantManifest([]string{"MY_URL"}, []string{"MY_API_KEY"})

	if got := ResolveSecret(context.Background(), "MY_API_KEY"); got != "sk-secret" {
		t.Fatalf("Secret returned %q, want sk-secret", got)
	}
	if got := ResolveConstant(context.Background(), "MY_URL"); got != "https://example.com" {
		t.Fatalf("Constant returned %q", got)
	}

	// Secret name in constants slot must panic — preserves boundary.
	defer func() {
		if r := recover(); r == nil {
			t.Fatal("expected panic — secret name resolved as constant crosses the boundary")
		}
	}()
	ResolveConstant(context.Background(), "MY_API_KEY")
}

func TestSecretPanicsBeforeInstall(t *testing.T) {
	defer resetManifestForTest()
	defer func() {
		if r := recover(); r == nil {
			t.Fatal("expected panic on ResolveSecret call before install")
		}
	}()
	ResolveSecret(context.Background(), "ANYTHING")
}

func TestInstallTwiceIdempotentWithSameRequired(t *testing.T) {
	defer resetManifestForTest()
	t.Setenv("CONST_A", "valueA")
	t.Setenv("SEC_B", "secretB")

	m1 := InstallConstantManifest([]string{"CONST_A"}, []string{"SEC_B"})
	m2 := InstallConstantManifest([]string{"CONST_A"}, []string{"SEC_B"})
	if m1 != m2 {
		t.Fatal("idempotent re-install should return the same manifest pointer")
	}
}

func TestInstallTwicePanicsWithDifferentRequired(t *testing.T) {
	defer resetManifestForTest()
	t.Setenv("CONST_A", "valueA")
	t.Setenv("CONST_C", "valueC")
	InstallConstantManifest([]string{"CONST_A"}, nil)

	defer func() {
		if r := recover(); r == nil {
			t.Fatal("expected panic on differing required set")
		}
	}()
	InstallConstantManifest([]string{"CONST_C"}, nil)
}
