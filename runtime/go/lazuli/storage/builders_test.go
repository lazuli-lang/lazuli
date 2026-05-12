package storage_test

import (
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestVisibilityBuilders(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name           string
		got            storage.FileContract
		wantResource   string
		wantField      string
		wantMaxSize    int64
		wantVisibility storage.FileVisibility
	}{
		{
			name:           "public",
			got:            storage.Public("Profile", "avatar", 5<<20, storage.ImageAny()),
			wantResource:   "Profile",
			wantField:      "avatar",
			wantMaxSize:    5 << 20,
			wantVisibility: storage.VisibilityPublic,
		},
		{
			name:           "private",
			got:            storage.Private("Import", "file", 10<<20, storage.TextMime("csv")),
			wantResource:   "Import",
			wantField:      "file",
			wantMaxSize:    10 << 20,
			wantVisibility: storage.VisibilityPrivate,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			if tc.got.Resource != tc.wantResource {
				t.Fatalf("Resource = %q, want %q", tc.got.Resource, tc.wantResource)
			}
			if tc.got.Field != tc.wantField {
				t.Fatalf("Field = %q, want %q", tc.got.Field, tc.wantField)
			}
			if tc.got.MaxSize != tc.wantMaxSize {
				t.Fatalf("MaxSize = %d, want %d", tc.got.MaxSize, tc.wantMaxSize)
			}
			if tc.got.Visibility != tc.wantVisibility {
				t.Fatalf("Visibility = %v, want %v", tc.got.Visibility, tc.wantVisibility)
			}
			if len(tc.got.Accept) != 1 {
				t.Fatalf("Accept len = %d, want 1", len(tc.got.Accept))
			}
		})
	}
}

func TestSignedBuilderSetsTTL(t *testing.T) {
	t.Parallel()

	contract := storage.Signed("Export", "archive", 100<<20, 15*time.Minute, storage.App("zip"))

	if contract.Visibility != storage.VisibilitySigned {
		t.Fatalf("Visibility = %v, want %v", contract.Visibility, storage.VisibilitySigned)
	}
	if contract.SignedTTL != 15*time.Minute {
		t.Fatalf("SignedTTL = %v, want %v", contract.SignedTTL, 15*time.Minute)
	}
	if got := contract.Accept[0]; got != (storage.MimeType{Family: "application", Subtype: "zip"}) {
		t.Fatalf("Accept[0] = %v", got)
	}
}

func TestMimeBuilders(t *testing.T) {
	t.Parallel()

	cases := []struct {
		got, want storage.MimeType
	}{
		{storage.ImageMime("png"), storage.MimeType{Family: "image", Subtype: "png"}},
		{storage.ImageAny(), storage.MimeType{Family: "image", Subtype: "*"}},
		{storage.TextMime("plain"), storage.MimeType{Family: "text", Subtype: "plain"}},
		{storage.App("json"), storage.MimeType{Family: "application", Subtype: "json"}},
	}

	for _, tc := range cases {
		if tc.got != tc.want {
			t.Fatalf("Mime builder = %v, want %v", tc.got, tc.want)
		}
	}
}
