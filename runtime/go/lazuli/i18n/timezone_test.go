package i18n

import (
	"net/http"
	"testing"
	"time"
)

func TestWithTimezoneStoresLocationInContext(t *testing.T) {
	loc, err := time.LoadLocation("America/Sao_Paulo")
	if err != nil {
		t.Fatal(err)
	}

	ctx := WithTimezone(t.Context(), loc)

	if got := TimezoneFromContext(ctx); got != loc {
		t.Fatalf("TimezoneFromContext(ctx) = %v, want %v", got, loc)
	}
}

func TestTimezoneFromContextReturnsNilWhenAbsent(t *testing.T) {
	if got := TimezoneFromContext(t.Context()); got != nil {
		t.Fatalf("TimezoneFromContext(ctx) = %v, want nil", got)
	}
}

func TestParseTimezoneLoadsAllowedName(t *testing.T) {
	loc, err := ParseTimezone(" America/Sao_Paulo ", []string{"America/Sao_Paulo"}, "UTC")
	if err != nil {
		t.Fatalf("ParseTimezone returned error: %v", err)
	}
	if got := loc.String(); got != "America/Sao_Paulo" {
		t.Fatalf("ParseTimezone location = %q, want America/Sao_Paulo", got)
	}
}

func TestParseTimezoneAllowsAnyLoadableZoneWithoutAllowlist(t *testing.T) {
	loc, err := ParseTimezone("Europe/Berlin", nil, "UTC")
	if err != nil {
		t.Fatalf("ParseTimezone returned error: %v", err)
	}
	if got := loc.String(); got != "Europe/Berlin" {
		t.Fatalf("ParseTimezone location = %q, want Europe/Berlin", got)
	}
}

func TestParseTimezoneFallsBackForBlankInvalidOrDisallowedName(t *testing.T) {
	tests := []struct {
		name     string
		allowed  []string
		fallback string
	}{
		{name: "", fallback: "UTC"},
		{name: "Mars/Base", fallback: "UTC"},
		{name: "Europe/Berlin", allowed: []string{"America/Sao_Paulo"}, fallback: "UTC"},
	}

	for _, tt := range tests {
		loc, err := ParseTimezone(tt.name, tt.allowed, tt.fallback)
		if err != nil {
			t.Fatalf("ParseTimezone(%q) returned error: %v", tt.name, err)
		}
		if got := loc.String(); got != "UTC" {
			t.Fatalf("ParseTimezone(%q) location = %q, want UTC", tt.name, got)
		}
	}
}

func TestParseTimezoneReturnsInvalidFallbackError(t *testing.T) {
	if _, err := ParseTimezone("", nil, "Mars/Base"); err == nil {
		t.Fatal("ParseTimezone returned nil error for invalid fallback")
	}
}

func TestResolveTimezonePrefersValidUserThenTenantThenFallback(t *testing.T) {
	allowed := []string{"America/Sao_Paulo", "Europe/Berlin"}

	tests := []struct {
		name     string
		user     string
		tenant   string
		fallback string
		want     string
	}{
		{
			name:     "user wins over tenant",
			user:     "Europe/Berlin",
			tenant:   "America/Sao_Paulo",
			fallback: "UTC",
			want:     "Europe/Berlin",
		},
		{
			name:     "tenant wins when user is disallowed",
			user:     "Asia/Tokyo",
			tenant:   "America/Sao_Paulo",
			fallback: "UTC",
			want:     "America/Sao_Paulo",
		},
		{
			name:     "fallback wins when neither candidate resolves",
			user:     "Asia/Tokyo",
			tenant:   "Mars/Base",
			fallback: "UTC",
			want:     "UTC",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			loc, err := ResolveTimezone(tt.user, tt.tenant, allowed, tt.fallback)
			if err != nil {
				t.Fatalf("ResolveTimezone returned error: %v", err)
			}
			if got := loc.String(); got != tt.want {
				t.Fatalf("ResolveTimezone location = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestTimezoneFromHeaderResolvesConfiguredHeader(t *testing.T) {
	header := http.Header{}
	header.Set(TimezoneHeader, "America/Sao_Paulo")

	loc, err := TimezoneFromHeader(header, []string{"America/Sao_Paulo"}, "UTC")
	if err != nil {
		t.Fatalf("TimezoneFromHeader returned error: %v", err)
	}
	if got := loc.String(); got != "America/Sao_Paulo" {
		t.Fatalf("TimezoneFromHeader location = %q, want America/Sao_Paulo", got)
	}
}

func TestTimezoneFromHeaderFallsBackForNilHeader(t *testing.T) {
	loc, err := TimezoneFromHeader(nil, nil, "")
	if err != nil {
		t.Fatalf("TimezoneFromHeader returned error: %v", err)
	}
	if got := loc.String(); got != "UTC" {
		t.Fatalf("TimezoneFromHeader location = %q, want UTC", got)
	}
}
