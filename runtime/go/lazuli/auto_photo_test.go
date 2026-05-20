package lazuli

import (
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestAutoPhotoKey(t *testing.T) {
	spec := AutoPhotoSpec{Table: "host", Field: "profile_photo"}
	got := autoPhotoKey(spec, ID(7), ID(42))
	want := "host/profile_photo/org-7/user-42"
	if got != want {
		t.Errorf("autoPhotoKey = %q, want %q", got, want)
	}
}

func TestAutoPhotoKeySeparatesOrgs(t *testing.T) {
	spec := AutoPhotoSpec{Table: "host", Field: "profile_photo"}
	userID := ID(42)

	orgOne := autoPhotoKey(spec, ID(1), userID)
	orgTwo := autoPhotoKey(spec, ID(2), userID)
	if orgOne == orgTwo {
		t.Fatalf("same user across orgs produced colliding key %q", orgOne)
	}
}

func TestCacheKeyIncludesActorAndUser(t *testing.T) {
	args := map[string]string{"status": "open"}
	anonymous := &Ctx{Actor: ActorAnonymous, Tenant: &Tenant{OrgID: 7}}
	system := &Ctx{Actor: ActorSystem, Tenant: &Tenant{OrgID: 7}}
	userOne := &Ctx{Actor: ActorUser, Tenant: &Tenant{OrgID: 7}, User: &User{ID: 1}}
	userTwo := &Ctx{Actor: ActorUser, Tenant: &Tenant{OrgID: 7}, User: &User{ID: 2}}

	anonymousKey, err := cacheKeyFor(anonymous, "ticket.list", args)
	if err != nil {
		t.Fatalf("anonymous cache key: %v", err)
	}
	systemKey, err := cacheKeyFor(system, "ticket.list", args)
	if err != nil {
		t.Fatalf("system cache key: %v", err)
	}
	userOneKey, err := cacheKeyFor(userOne, "ticket.list", args)
	if err != nil {
		t.Fatalf("user one cache key: %v", err)
	}
	userTwoKey, err := cacheKeyFor(userTwo, "ticket.list", args)
	if err != nil {
		t.Fatalf("user two cache key: %v", err)
	}

	if anonymousKey == systemKey {
		t.Fatal("cache key must separate anonymous and system actors")
	}
	if userOneKey == userTwoKey {
		t.Fatal("cache key must separate user identities")
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
