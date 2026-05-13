package security

import (
	"errors"
	"testing"
)

func TestXFrameOptionsBuilderRendersHeader(t *testing.T) {
	t.Parallel()

	name, value, err := NewXFrameOptionsBuilder().Deny().Header()
	if err != nil {
		t.Fatalf("Header() error = %v", err)
	}
	if name != HeaderXFrameOptions {
		t.Fatalf("header name = %q, want %q", name, HeaderXFrameOptions)
	}
	if value != XFrameOptionsDeny {
		t.Fatalf("header value = %q, want %q", value, XFrameOptionsDeny)
	}

	value, err = (XFrameOptionsPolicy{Value: " sameorigin "}).HeaderValue()
	if err != nil {
		t.Fatalf("HeaderValue() error = %v", err)
	}
	if value != XFrameOptionsSameOrigin {
		t.Fatalf("header value = %q, want %q", value, XFrameOptionsSameOrigin)
	}
}

func TestXContentTypeOptionsBuilderRendersHeader(t *testing.T) {
	t.Parallel()

	name, value, err := NewXContentTypeOptionsBuilder().NoSniff().Header()
	if err != nil {
		t.Fatalf("Header() error = %v", err)
	}
	if name != HeaderXContentTypeOptions {
		t.Fatalf("header name = %q, want %q", name, HeaderXContentTypeOptions)
	}
	if value != XContentTypeOptionsNoSniff {
		t.Fatalf("header value = %q, want %q", value, XContentTypeOptionsNoSniff)
	}
}

func TestReferrerPolicyBuilderRendersOrderedFallbacks(t *testing.T) {
	t.Parallel()

	name, value, err := NewReferrerPolicyBuilder().
		NoReferrer().
		StrictOriginWhenCrossOrigin().
		Policy("STRICT-ORIGIN-WHEN-CROSS-ORIGIN").
		Header()
	if err != nil {
		t.Fatalf("Header() error = %v", err)
	}
	if name != HeaderReferrerPolicy {
		t.Fatalf("header name = %q, want %q", name, HeaderReferrerPolicy)
	}

	want := "no-referrer, strict-origin-when-cross-origin"
	if value != want {
		t.Fatalf("header value = %q, want %q", value, want)
	}
}

func TestPermissionsPolicyBuilderRendersDeterministicHeader(t *testing.T) {
	t.Parallel()

	name, value, err := NewPermissionsPolicyBuilder().
		Disable("microphone", "CAMERA").
		Allow("geolocation", "https://b.example.com/", PermissionsPolicySelf, `"https://a.example.com"`).
		Allow("geolocation", "https://a.example.com").
		Allow("fullscreen", PermissionsPolicyAll).
		Header()
	if err != nil {
		t.Fatalf("Header() error = %v", err)
	}
	if name != HeaderPermissionsPolicy {
		t.Fatalf("header name = %q, want %q", name, HeaderPermissionsPolicy)
	}

	want := `camera=(), fullscreen=*, geolocation=(self "https://a.example.com" "https://b.example.com"), microphone=()`
	if value != want {
		t.Fatalf("header value = %q, want %q", value, want)
	}
}

func TestPolicyHeaderValidationRejectsUnsafeInput(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		err  func() error
	}{
		{
			name: "empty frame options",
			err: func() error {
				_, err := NewXFrameOptionsBuilder().HeaderValue()
				return err
			},
		},
		{
			name: "obsolete frame allow-from",
			err: func() error {
				return (XFrameOptionsPolicy{Value: "ALLOW-FROM https://example.com"}).Validate()
			},
		},
		{
			name: "invalid content type options",
			err: func() error {
				return (XContentTypeOptionsPolicy{Value: "sniff"}).Validate()
			},
		},
		{
			name: "unknown referrer policy",
			err: func() error {
				return ReferrerPolicy{Values: []string{"same-origin\nunsafe-url"}}.Validate()
			},
		},
		{
			name: "empty permissions policy",
			err: func() error {
				return (PermissionsPolicy{}).Validate()
			},
		},
		{
			name: "permissions feature injection",
			err: func() error {
				_, err := NewPermissionsPolicyBuilder().Disable("geolocation; camera").HeaderValue()
				return err
			},
		},
		{
			name: "permissions origin path",
			err: func() error {
				_, err := NewPermissionsPolicyBuilder().Allow("geolocation", "https://example.com/map").HeaderValue()
				return err
			},
		},
		{
			name: "permissions wildcard mixed with self",
			err: func() error {
				_, err := NewPermissionsPolicyBuilder().Allow("camera", PermissionsPolicyAll, PermissionsPolicySelf).HeaderValue()
				return err
			},
		},
		{
			name: "permissions empty allowlist mixed with origins",
			err: func() error {
				_, err := NewPermissionsPolicyBuilder().Disable("camera").AllowSelf("camera").HeaderValue()
				return err
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if err := tt.err(); !errors.Is(err, ErrInvalidPolicyHeader) {
				t.Fatalf("error = %v, want ErrInvalidPolicyHeader", err)
			}
		})
	}
}
