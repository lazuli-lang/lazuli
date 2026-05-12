package auth

import (
	"errors"
	"slices"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

func TestDeviceFingerprintIDNormalizesAndValidates(t *testing.T) {
	t.Parallel()

	fingerprint := DeviceFingerprint{
		UserAgent:      " Mozilla/5.0 ",
		IPAddress:      " ::ffff:203.0.113.10 ",
		Platform:       " web ",
		AcceptLanguage: " en-US ",
		Label:          " Lucas Laptop ",
	}
	normalized := fingerprint.Normalize()
	if normalized.IPAddress != "203.0.113.10" {
		t.Fatalf("Normalize() IPAddress = %q, want canonical IPv4", normalized.IPAddress)
	}

	got, err := DeviceFingerprintID(fingerprint)
	if err != nil {
		t.Fatalf("DeviceFingerprintID() error = %v", err)
	}
	want, err := DeviceFingerprintID(DeviceFingerprint{
		UserAgent:      "Mozilla/5.0",
		IPAddress:      "203.0.113.10",
		Platform:       "web",
		AcceptLanguage: "en-US",
		Label:          "Lucas Laptop",
	})
	if err != nil {
		t.Fatalf("DeviceFingerprintID(canonical) error = %v", err)
	}
	if got != want {
		t.Fatalf("DeviceFingerprintID() = %q, want normalized ID %q", got, want)
	}

	if err := ValidateDeviceFingerprint(DeviceFingerprint{}); !errors.Is(err, ErrDeviceFingerprintInvalid) {
		t.Fatalf("ValidateDeviceFingerprint(empty) error = %v, want ErrDeviceFingerprintInvalid", err)
	}
	_, err = DeviceFingerprintID(DeviceFingerprint{IPAddress: "not an ip"})
	if !errors.Is(err, ErrDeviceFingerprintInvalid) {
		t.Fatalf("DeviceFingerprintID(invalid IP) error = %v, want ErrDeviceFingerprintInvalid", err)
	}
}

func TestTrustedDeviceTokenMetadataHashesRawToken(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	fingerprint := DeviceFingerprint{
		UserAgent: "Mozilla/5.0",
		IPAddress: "203.0.113.10",
		Platform:  "web",
	}

	meta, err := NewTrustedDeviceTokenMetadata("remember-token", fingerprint, now, 30*24*time.Hour)
	if err != nil {
		t.Fatalf("NewTrustedDeviceTokenMetadata() error = %v", err)
	}
	wantHash, err := HashTrustedDeviceToken("remember-token")
	if err != nil {
		t.Fatalf("HashTrustedDeviceToken() error = %v", err)
	}
	if meta.TokenHash != wantHash {
		t.Fatalf("TokenHash = %q, want %q", meta.TokenHash, wantHash)
	}
	if meta.TokenHash == "remember-token" {
		t.Fatalf("TokenHash must not retain the raw token")
	}
	if meta.ID != wantHash[:32] {
		t.Fatalf("ID = %q, want token hash prefix", meta.ID)
	}
	if meta.DeviceID == "" {
		t.Fatal("DeviceID is empty")
	}
	if !meta.CreatedAt.Equal(now) || !meta.LastUsedAt.Equal(now) {
		t.Fatalf("timestamps = CreatedAt %s LastUsedAt %s, want %s", meta.CreatedAt, meta.LastUsedAt, now)
	}
	if want := now.Add(30 * 24 * time.Hour); !meta.ExpiresAt.Equal(want) {
		t.Fatalf("ExpiresAt = %s, want %s", meta.ExpiresAt, want)
	}
	if err := meta.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	bad := meta
	bad.TokenHash = "remember-token"
	if err := bad.Validate(); !errors.Is(err, ErrTrustedDeviceTokenInvalid) {
		t.Fatalf("Validate(raw token hash) error = %v, want ErrTrustedDeviceTokenInvalid", err)
	}
	bad = meta
	bad.ExpiresAt = now
	if err := bad.Validate(); !errors.Is(err, ErrTrustedDeviceTokenInvalid) {
		t.Fatalf("Validate(non-positive lifetime) error = %v, want ErrTrustedDeviceTokenInvalid", err)
	}
}

func TestBuildDeviceSessionListGroupsActiveSessionsByDevice(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 16, 0, 0, 0, time.UTC)
	deviceA := DeviceFingerprint{UserAgent: "Mozilla/5.0", IPAddress: "203.0.113.10", Platform: "web"}
	deviceAID, err := DeviceFingerprintID(deviceA)
	if err != nil {
		t.Fatalf("DeviceFingerprintID() error = %v", err)
	}
	sessions := []DeviceSession{
		{
			SessionID:            "a-older",
			UserID:               lazuli.ID(42),
			Fingerprint:          deviceA,
			TrustedDeviceTokenID: "trusted-a",
			CreatedAt:            now.Add(-2 * time.Hour),
			LastSeenAt:           now.Add(-3 * time.Minute),
			ExpiresAt:            now.Add(time.Hour),
		},
		{
			SessionID:   "b-active",
			UserID:      lazuli.ID(42),
			DeviceID:    "device-b",
			Fingerprint: DeviceFingerprint{Label: "Mobile app"},
			CreatedAt:   now.Add(-time.Hour),
			LastSeenAt:  now.Add(-2 * time.Minute),
			ExpiresAt:   now.Add(time.Hour),
		},
		{
			SessionID:   "a-current",
			UserID:      lazuli.ID(42),
			Fingerprint: deviceA,
			CreatedAt:   now.Add(-30 * time.Minute),
			LastSeenAt:  now.Add(-time.Minute),
			ExpiresAt:   now.Add(time.Hour),
			Current:     true,
		},
		{
			SessionID:   "expired",
			UserID:      lazuli.ID(42),
			Fingerprint: deviceA,
			CreatedAt:   now.Add(-2 * time.Hour),
			ExpiresAt:   now.Add(-time.Minute),
		},
		{
			SessionID: "revoked",
			UserID:    lazuli.ID(42),
			DeviceID:  "device-c",
			CreatedAt: now.Add(-2 * time.Hour),
			ExpiresAt: now.Add(time.Hour),
			RevokedAt: now.Add(-time.Minute),
		},
	}

	list, err := BuildDeviceSessionList(sessions, now)
	if err != nil {
		t.Fatalf("BuildDeviceSessionList() error = %v", err)
	}
	if !list.GeneratedAt.Equal(now) {
		t.Fatalf("GeneratedAt = %s, want %s", list.GeneratedAt, now)
	}
	if len(list.Devices) != 2 {
		t.Fatalf("devices len = %d, want 2: %#v", len(list.Devices), list.Devices)
	}
	if list.Devices[0].DeviceID != deviceAID {
		t.Fatalf("first device = %q, want active current device %q", list.Devices[0].DeviceID, deviceAID)
	}
	if !list.Devices[0].Trusted {
		t.Fatal("device A Trusted = false, want true")
	}
	if !list.Devices[0].Current {
		t.Fatal("device A Current = false, want true")
	}
	if len(list.Devices[0].Sessions) != 2 {
		t.Fatalf("device A session count = %d, want 2", len(list.Devices[0].Sessions))
	}
	if got := list.Devices[0].Sessions[0].SessionID; got != "a-current" {
		t.Fatalf("first device A session = %q, want most recently seen current session", got)
	}
	if list.Devices[1].DeviceID != "device-b" {
		t.Fatalf("second device = %q, want device-b", list.Devices[1].DeviceID)
	}
}

func TestPlanDeviceSessionRevocationSelectsTargetDevice(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 17, 0, 0, 0, time.UTC)
	sessions := []DeviceSession{
		{
			SessionID:            "revoke-a",
			UserID:               lazuli.ID(42),
			DeviceID:             "device-a",
			TrustedDeviceTokenID: "trusted-a",
			CreatedAt:            now.Add(-2 * time.Hour),
			LastSeenAt:           now.Add(-10 * time.Minute),
			ExpiresAt:            now.Add(time.Hour),
		},
		{
			SessionID:            "revoke-b",
			UserID:               lazuli.ID(42),
			DeviceID:             "device-a",
			TrustedDeviceTokenID: "trusted-a",
			CreatedAt:            now.Add(-time.Hour),
			LastSeenAt:           now.Add(-5 * time.Minute),
			ExpiresAt:            now.Add(time.Hour),
		},
		{
			SessionID:            "current",
			UserID:               lazuli.ID(42),
			DeviceID:             "device-a",
			TrustedDeviceTokenID: "trusted-current",
			CreatedAt:            now.Add(-time.Hour),
			LastSeenAt:           now.Add(-time.Minute),
			ExpiresAt:            now.Add(time.Hour),
			Current:              true,
		},
		{
			SessionID:            "expired",
			UserID:               lazuli.ID(42),
			DeviceID:             "device-a",
			TrustedDeviceTokenID: "trusted-expired",
			CreatedAt:            now.Add(-2 * time.Hour),
			ExpiresAt:            now.Add(-time.Minute),
		},
		{
			SessionID: "revoked",
			UserID:    lazuli.ID(42),
			DeviceID:  "device-a",
			CreatedAt: now.Add(-2 * time.Hour),
			ExpiresAt: now.Add(time.Hour),
			RevokedAt: now.Add(-time.Minute),
		},
		{
			SessionID: "other-device",
			UserID:    lazuli.ID(42),
			DeviceID:  "device-b",
			CreatedAt: now.Add(-2 * time.Hour),
			ExpiresAt: now.Add(time.Hour),
		},
		{
			SessionID: "other-user",
			UserID:    lazuli.ID(7),
			DeviceID:  "device-a",
			CreatedAt: now.Add(-2 * time.Hour),
			ExpiresAt: now.Add(time.Hour),
		},
	}

	plan, err := PlanDeviceSessionRevocation(sessions, DeviceSessionRevocationTarget{
		UserID:                    lazuli.ID(42),
		DeviceID:                  " device-a ",
		KeepCurrent:               true,
		CurrentSessionID:          "current",
		RevokeTrustedDeviceTokens: true,
	}, now)
	if err != nil {
		t.Fatalf("PlanDeviceSessionRevocation() error = %v", err)
	}
	if !plan.DryRun {
		t.Fatal("DryRun = false, want true")
	}
	if got, want := plan.SessionIDs, []string{"revoke-a", "revoke-b"}; !slices.Equal(got, want) {
		t.Fatalf("SessionIDs = %#v, want %#v", got, want)
	}
	if got, want := plan.TrustedDeviceTokenIDs, []string{"trusted-a"}; !slices.Equal(got, want) {
		t.Fatalf("TrustedDeviceTokenIDs = %#v, want %#v", got, want)
	}

	includeExpired, err := PlanDeviceSessionRevocation(sessions, DeviceSessionRevocationTarget{
		UserID:         lazuli.ID(42),
		DeviceID:       "device-a",
		IncludeExpired: true,
	}, now)
	if err != nil {
		t.Fatalf("PlanDeviceSessionRevocation(include expired) error = %v", err)
	}
	if got, want := includeExpired.SessionIDs, []string{"current", "expired", "revoke-a", "revoke-b"}; !slices.Equal(got, want) {
		t.Fatalf("SessionIDs = %#v, want %#v", got, want)
	}

	_, err = PlanDeviceSessionRevocation(nil, DeviceSessionRevocationTarget{}, now)
	if !errors.Is(err, ErrDeviceSessionInvalid) {
		t.Fatalf("PlanDeviceSessionRevocation(invalid target) error = %v, want ErrDeviceSessionInvalid", err)
	}
}
