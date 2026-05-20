package lazuli

import (
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestAutoPhotoKey(t *testing.T) {
	spec := AutoPhotoSpec{Table: "host", Field: "profile_photo"}
	got := autoPhotoKey(spec, ID(42))
	want := "host/profile_photo/42"
	if got != want {
		t.Errorf("autoPhotoKey = %q, want %q", got, want)
	}
}

func TestContractAcceptsMime_ExactMatch(t *testing.T) {
	contract := storage.FileContract{
		Accept: []storage.MimeType{{Family: "image", Subtype: "jpeg"}},
	}
	if !contractAcceptsMime(contract, "image/jpeg") {
		t.Error("expected image/jpeg to match image/jpeg")
	}
}

func TestContractAcceptsMime_WildcardSubtype(t *testing.T) {
	contract := storage.FileContract{
		Accept: []storage.MimeType{{Family: "image", Subtype: "*"}},
	}
	if !contractAcceptsMime(contract, "image/jpeg") {
		t.Error("expected image/* to match image/jpeg")
	}
}

func TestContractAcceptsMime_Reject(t *testing.T) {
	contract := storage.FileContract{
		Accept: []storage.MimeType{{Family: "image", Subtype: "jpeg"}},
	}
	if contractAcceptsMime(contract, "text/csv") {
		t.Error("expected text/csv to reject against image/jpeg-only contract")
	}
}

func TestContractAcceptsMime_EmptyAcceptPasses(t *testing.T) {
	contract := storage.FileContract{}
	if !contractAcceptsMime(contract, "anything/here") {
		t.Error("empty Accept must accept everything (caller's choice)")
	}
}

func TestSplitMime(t *testing.T) {
	f, s, ok := splitMime("image/jpeg")
	if !ok || f != "image" || s != "jpeg" {
		t.Errorf("splitMime(image/jpeg) = (%q, %q, %v)", f, s, ok)
	}
	_, _, ok = splitMime("no-slash")
	if ok {
		t.Error("splitMime(no-slash) should be !ok")
	}
}

func TestQuoteIdent(t *testing.T) {
	if got := quoteIdent("host"); got != `"host"` {
		t.Errorf("quoteIdent(host) = %q", got)
	}
}

// PutTTL / GetTTL default coverage -- exercises the constants live.
func TestAutoPhotoTTLDefaults(t *testing.T) {
	if defaultAutoPhotoPutTTL != 15*time.Minute {
		t.Error("defaultAutoPhotoPutTTL drift")
	}
	if defaultAutoPhotoGetTTL != 24*time.Hour {
		t.Error("defaultAutoPhotoGetTTL drift")
	}
}
