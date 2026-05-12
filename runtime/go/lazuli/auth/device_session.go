package auth

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net/netip"
	"sort"
	"strings"
	"time"

	"lazuli.dev/runtime/lazuli"
)

const (
	deviceFingerprintVersion = "v1"

	maxDeviceFingerprintSignalLength = 1024
	maxDeviceIdentifierLength        = 128
)

var (
	// ErrDeviceFingerprintInvalid reports a device fingerprint that cannot be
	// normalized into a stable provider-neutral device identity.
	ErrDeviceFingerprintInvalid = errors.New("auth: device fingerprint invalid")
	// ErrTrustedDeviceTokenInvalid reports malformed remember-device token
	// metadata before it reaches a concrete adapter.
	ErrTrustedDeviceTokenInvalid = errors.New("auth: trusted device token invalid")
	// ErrDeviceSessionInvalid reports a malformed per-device session snapshot
	// or revocation target.
	ErrDeviceSessionInvalid = errors.New("auth: device session invalid")
)

// DeviceFingerprint captures provider-neutral signals that can distinguish a
// browser or client device without binding the runtime to an HTTP adapter.
//
// IPAddress is optional, but when present it must be a literal IP address.
// Label is intended for display labels such as "Chrome on macOS".
type DeviceFingerprint struct {
	UserAgent      string
	IPAddress      string
	Platform       string
	AcceptLanguage string
	Label          string
}

// Normalize returns a copy with trimmed text and canonical literal IP form.
func (f DeviceFingerprint) Normalize() DeviceFingerprint {
	normalized := DeviceFingerprint{
		UserAgent:      strings.TrimSpace(f.UserAgent),
		IPAddress:      strings.TrimSpace(f.IPAddress),
		Platform:       strings.TrimSpace(f.Platform),
		AcceptLanguage: strings.TrimSpace(f.AcceptLanguage),
		Label:          strings.TrimSpace(f.Label),
	}
	if normalized.IPAddress != "" {
		if addr, err := netip.ParseAddr(normalized.IPAddress); err == nil {
			normalized.IPAddress = addr.Unmap().String()
		}
	}
	return normalized
}

// Validate reports whether the fingerprint has at least one usable signal and
// any provided IP address can be parsed.
func (f DeviceFingerprint) Validate() error {
	return ValidateDeviceFingerprint(f)
}

// ValidateDeviceFingerprint reports malformed device fingerprint signals.
func ValidateDeviceFingerprint(f DeviceFingerprint) error {
	normalized := f.Normalize()
	var errs []error
	if normalized.UserAgent == "" &&
		normalized.IPAddress == "" &&
		normalized.Platform == "" &&
		normalized.AcceptLanguage == "" &&
		normalized.Label == "" {
		errs = append(errs, fmt.Errorf("%w: at least one signal is required", ErrDeviceFingerprintInvalid))
	}
	if rawIP := strings.TrimSpace(f.IPAddress); rawIP != "" {
		if _, err := netip.ParseAddr(rawIP); err != nil {
			errs = append(errs, fmt.Errorf("%w: IPAddress is not a literal IP address", ErrDeviceFingerprintInvalid))
		}
	}
	if signalTooLong(normalized.UserAgent) ||
		signalTooLong(normalized.IPAddress) ||
		signalTooLong(normalized.Platform) ||
		signalTooLong(normalized.AcceptLanguage) ||
		signalTooLong(normalized.Label) {
		errs = append(errs, fmt.Errorf("%w: signal exceeds %d bytes", ErrDeviceFingerprintInvalid, maxDeviceFingerprintSignalLength))
	}
	return errors.Join(errs...)
}

// DeviceFingerprintID returns a deterministic device identifier for normalized
// fingerprint signals. Adapters may persist this value without storing raw
// request headers in session rows.
func DeviceFingerprintID(f DeviceFingerprint) (string, error) {
	normalized := f.Normalize()
	if err := ValidateDeviceFingerprint(normalized); err != nil {
		return "", err
	}

	sum := sha256.Sum256([]byte(strings.Join([]string{
		deviceFingerprintVersion,
		normalized.UserAgent,
		normalized.IPAddress,
		normalized.Platform,
		normalized.AcceptLanguage,
		normalized.Label,
	}, "\x00")))
	return hex.EncodeToString(sum[:]), nil
}

// TrustedDeviceTokenMetadata is the adapter-neutral metadata for a
// remember-device token. TokenHash is the SHA-256 hex digest of the raw token;
// callers should never persist or expose the raw token value.
type TrustedDeviceTokenMetadata struct {
	ID        string
	TokenHash string
	DeviceID  string

	Fingerprint DeviceFingerprint

	CreatedAt  time.Time
	LastUsedAt time.Time
	ExpiresAt  time.Time
	RevokedAt  time.Time
}

// NewTrustedDeviceTokenMetadata returns normalized metadata for a raw
// remember-device token and fingerprint. The raw token is never retained.
func NewTrustedDeviceTokenMetadata(
	token string,
	fingerprint DeviceFingerprint,
	now time.Time,
	ttl time.Duration,
) (TrustedDeviceTokenMetadata, error) {
	if ttl <= 0 {
		return TrustedDeviceTokenMetadata{}, fmt.Errorf("%w: ttl must be positive", ErrTrustedDeviceTokenInvalid)
	}
	tokenHash, err := HashTrustedDeviceToken(token)
	if err != nil {
		return TrustedDeviceTokenMetadata{}, err
	}
	deviceID, err := DeviceFingerprintID(fingerprint)
	if err != nil {
		return TrustedDeviceTokenMetadata{}, errors.Join(
			fmt.Errorf("%w: fingerprint invalid", ErrTrustedDeviceTokenInvalid),
			err,
		)
	}
	if now.IsZero() {
		now = time.Now()
	}
	now = now.UTC()

	return TrustedDeviceTokenMetadata{
		ID:          tokenHash[:32],
		TokenHash:   tokenHash,
		DeviceID:    deviceID,
		Fingerprint: fingerprint.Normalize(),
		CreatedAt:   now,
		LastUsedAt:  now,
		ExpiresAt:   now.Add(ttl),
	}, nil
}

// HashTrustedDeviceToken hashes a raw remember-device token for persistence.
func HashTrustedDeviceToken(token string) (string, error) {
	token = strings.TrimSpace(token)
	if token == "" {
		return "", ErrTokenInvalid
	}
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:]), nil
}

// Validate reports whether metadata is complete and internally consistent.
func (m TrustedDeviceTokenMetadata) Validate() error {
	return ValidateTrustedDeviceTokenMetadata(m)
}

// ValidateTrustedDeviceTokenMetadata reports malformed remember-device token
// metadata before an adapter persists or consumes it.
func ValidateTrustedDeviceTokenMetadata(m TrustedDeviceTokenMetadata) error {
	var errs []error
	if err := validateDeviceIdentifier("trusted device token ID", m.ID); err != nil {
		errs = append(errs, fmt.Errorf("%w: %v", ErrTrustedDeviceTokenInvalid, err))
	}
	if tokenHash := strings.TrimSpace(m.TokenHash); !isSHA256Hex(tokenHash) {
		errs = append(errs, fmt.Errorf("%w: token hash must be SHA-256 hex", ErrTrustedDeviceTokenInvalid))
	}
	if err := validateDeviceIdentifier("device ID", m.DeviceID); err != nil {
		errs = append(errs, fmt.Errorf("%w: %v", ErrTrustedDeviceTokenInvalid, err))
	}
	if err := ValidateDeviceFingerprint(m.Fingerprint); err != nil {
		errs = append(errs, errors.Join(
			fmt.Errorf("%w: fingerprint invalid", ErrTrustedDeviceTokenInvalid),
			err,
		))
	}
	if m.CreatedAt.IsZero() {
		errs = append(errs, fmt.Errorf("%w: CreatedAt is required", ErrTrustedDeviceTokenInvalid))
	}
	if m.ExpiresAt.IsZero() {
		errs = append(errs, fmt.Errorf("%w: ExpiresAt is required", ErrTrustedDeviceTokenInvalid))
	}
	if !m.CreatedAt.IsZero() && !m.ExpiresAt.IsZero() && !m.ExpiresAt.After(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: ExpiresAt must be after CreatedAt", ErrTrustedDeviceTokenInvalid))
	}
	if !m.LastUsedAt.IsZero() && !m.CreatedAt.IsZero() && m.LastUsedAt.Before(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: LastUsedAt must not be before CreatedAt", ErrTrustedDeviceTokenInvalid))
	}
	if !m.LastUsedAt.IsZero() && !m.ExpiresAt.IsZero() && m.LastUsedAt.After(m.ExpiresAt) {
		errs = append(errs, fmt.Errorf("%w: LastUsedAt must not be after ExpiresAt", ErrTrustedDeviceTokenInvalid))
	}
	if !m.RevokedAt.IsZero() && !m.CreatedAt.IsZero() && m.RevokedAt.Before(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: RevokedAt must not be before CreatedAt", ErrTrustedDeviceTokenInvalid))
	}
	return errors.Join(errs...)
}

// DeviceSession is an adapter-neutral snapshot used to list and revoke active
// sessions by device. SessionID is an application/session row identifier, not
// the raw bearer token.
type DeviceSession struct {
	SessionID string
	UserID    lazuli.ID
	DeviceID  string

	Fingerprint          DeviceFingerprint
	TrustedDeviceTokenID string

	CreatedAt  time.Time
	LastSeenAt time.Time
	ExpiresAt  time.Time
	RevokedAt  time.Time
	Current    bool
}

// Validate reports whether a device session snapshot can be planned safely.
func (s DeviceSession) Validate() error {
	return ValidateDeviceSession(s)
}

// ValidateDeviceSession reports malformed per-device session snapshots.
func ValidateDeviceSession(s DeviceSession) error {
	_, err := normalizeDeviceSession(s)
	return err
}

// DeviceSessionDevice is one device group in a session listing.
type DeviceSessionDevice struct {
	DeviceID    string
	Fingerprint DeviceFingerprint
	Trusted     bool
	Current     bool
	LastSeenAt  time.Time
	Sessions    []DeviceSession
}

// DeviceSessionList is a dry-run listing of active sessions grouped by device.
type DeviceSessionList struct {
	GeneratedAt time.Time
	Devices     []DeviceSessionDevice
}

// BuildDeviceSessionList groups active, unrevoked sessions by device. It does
// not call a store; concrete adapters provide snapshots and apply the result.
func BuildDeviceSessionList(sessions []DeviceSession, now time.Time) (DeviceSessionList, error) {
	now = normalizeDeviceSessionTime(now)
	groups := make(map[string]*DeviceSessionDevice)
	order := make([]string, 0, len(sessions))

	for _, raw := range sessions {
		session, err := normalizeDeviceSession(raw)
		if err != nil {
			return DeviceSessionList{}, err
		}
		if !deviceSessionActiveAt(session, now) {
			continue
		}

		group, ok := groups[session.DeviceID]
		if !ok {
			group = &DeviceSessionDevice{DeviceID: session.DeviceID}
			groups[session.DeviceID] = group
			order = append(order, session.DeviceID)
		}
		group.Sessions = append(group.Sessions, session)
		if session.TrustedDeviceTokenID != "" {
			group.Trusted = true
		}
		if session.Current {
			group.Current = true
		}
		if observed := deviceSessionObservedAt(session); observed.After(group.LastSeenAt) {
			group.LastSeenAt = observed
			group.Fingerprint = session.Fingerprint
		}
	}

	devices := make([]DeviceSessionDevice, 0, len(groups))
	for _, deviceID := range order {
		group := groups[deviceID]
		sort.SliceStable(group.Sessions, func(i, j int) bool {
			return deviceSessionAfter(group.Sessions[i], group.Sessions[j])
		})
		devices = append(devices, *group)
	}
	sort.SliceStable(devices, func(i, j int) bool {
		if !devices[i].LastSeenAt.Equal(devices[j].LastSeenAt) {
			return devices[i].LastSeenAt.After(devices[j].LastSeenAt)
		}
		return devices[i].DeviceID < devices[j].DeviceID
	})

	return DeviceSessionList{GeneratedAt: now, Devices: devices}, nil
}

// DeviceSessionRevocationTarget selects one user's sessions on one device.
type DeviceSessionRevocationTarget struct {
	UserID   lazuli.ID
	DeviceID string

	// KeepCurrent preserves sessions marked Current and the optional
	// CurrentSessionID when revoking the rest of a device.
	KeepCurrent      bool
	CurrentSessionID string

	// IncludeExpired includes expired but not-yet-revoked sessions in the plan.
	IncludeExpired bool
	// RevokeTrustedDeviceTokens includes remember-device token IDs associated
	// with planned sessions.
	RevokeTrustedDeviceTokens bool
}

// DeviceSessionRevocationPlan is a dry-run plan of provider-neutral IDs for an
// adapter to revoke.
type DeviceSessionRevocationPlan struct {
	DryRun      bool
	GeneratedAt time.Time
	UserID      lazuli.ID
	DeviceID    string

	SessionIDs            []string
	TrustedDeviceTokenIDs []string
}

// PlanDeviceSessionRevocation selects session row IDs, and optionally trusted
// device token IDs, for one user's device. It does not mutate storage.
func PlanDeviceSessionRevocation(
	sessions []DeviceSession,
	target DeviceSessionRevocationTarget,
	now time.Time,
) (DeviceSessionRevocationPlan, error) {
	if err := validateDeviceSessionRevocationTarget(target); err != nil {
		return DeviceSessionRevocationPlan{}, err
	}
	now = normalizeDeviceSessionTime(now)
	deviceID := strings.TrimSpace(target.DeviceID)
	currentSessionID := strings.TrimSpace(target.CurrentSessionID)

	plan := DeviceSessionRevocationPlan{
		DryRun:      true,
		GeneratedAt: now,
		UserID:      target.UserID,
		DeviceID:    deviceID,
	}
	sessionIDs := make(map[string]struct{}, len(sessions))
	trustedTokenIDs := make(map[string]struct{}, len(sessions))

	for _, raw := range sessions {
		session, err := normalizeDeviceSession(raw)
		if err != nil {
			return DeviceSessionRevocationPlan{}, err
		}
		if session.UserID != target.UserID || session.DeviceID != deviceID {
			continue
		}
		if !session.RevokedAt.IsZero() {
			continue
		}
		if !target.IncludeExpired && !session.ExpiresAt.After(now) {
			continue
		}
		if target.KeepCurrent && (session.Current || session.SessionID == currentSessionID) {
			continue
		}

		sessionIDs[session.SessionID] = struct{}{}
		if target.RevokeTrustedDeviceTokens && session.TrustedDeviceTokenID != "" {
			trustedTokenIDs[session.TrustedDeviceTokenID] = struct{}{}
		}
	}

	plan.SessionIDs = sortedMapKeys(sessionIDs)
	plan.TrustedDeviceTokenIDs = sortedMapKeys(trustedTokenIDs)
	return plan, nil
}

func normalizeDeviceSession(raw DeviceSession) (DeviceSession, error) {
	session := raw
	session.SessionID = strings.TrimSpace(session.SessionID)
	session.DeviceID = strings.TrimSpace(session.DeviceID)
	session.TrustedDeviceTokenID = strings.TrimSpace(session.TrustedDeviceTokenID)
	session.Fingerprint = session.Fingerprint.Normalize()

	var errs []error
	if err := validateDeviceIdentifier("session ID", session.SessionID); err != nil {
		errs = append(errs, fmt.Errorf("%w: %v", ErrDeviceSessionInvalid, err))
	}
	if session.UserID <= 0 {
		errs = append(errs, fmt.Errorf("%w: UserID is required", ErrDeviceSessionInvalid))
	}
	if session.DeviceID == "" {
		deviceID, err := DeviceFingerprintID(session.Fingerprint)
		if err != nil {
			errs = append(errs, errors.Join(
				fmt.Errorf("%w: DeviceID or valid fingerprint is required", ErrDeviceSessionInvalid),
				err,
			))
		} else {
			session.DeviceID = deviceID
		}
	} else if err := validateDeviceIdentifier("device ID", session.DeviceID); err != nil {
		errs = append(errs, fmt.Errorf("%w: %v", ErrDeviceSessionInvalid, err))
	}
	if !emptyDeviceFingerprint(session.Fingerprint) {
		if err := ValidateDeviceFingerprint(session.Fingerprint); err != nil {
			errs = append(errs, errors.Join(
				fmt.Errorf("%w: fingerprint invalid", ErrDeviceSessionInvalid),
				err,
			))
		}
	}
	if session.TrustedDeviceTokenID != "" {
		if err := validateDeviceIdentifier("trusted device token ID", session.TrustedDeviceTokenID); err != nil {
			errs = append(errs, fmt.Errorf("%w: %v", ErrDeviceSessionInvalid, err))
		}
	}
	if session.ExpiresAt.IsZero() {
		errs = append(errs, fmt.Errorf("%w: ExpiresAt is required", ErrDeviceSessionInvalid))
	}
	if !session.CreatedAt.IsZero() && !session.ExpiresAt.IsZero() && session.ExpiresAt.Before(session.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: ExpiresAt must not be before CreatedAt", ErrDeviceSessionInvalid))
	}
	if !session.LastSeenAt.IsZero() && !session.CreatedAt.IsZero() && session.LastSeenAt.Before(session.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: LastSeenAt must not be before CreatedAt", ErrDeviceSessionInvalid))
	}
	if !session.RevokedAt.IsZero() && !session.CreatedAt.IsZero() && session.RevokedAt.Before(session.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: RevokedAt must not be before CreatedAt", ErrDeviceSessionInvalid))
	}
	if err := errors.Join(errs...); err != nil {
		return DeviceSession{}, err
	}
	return session, nil
}

func validateDeviceSessionRevocationTarget(target DeviceSessionRevocationTarget) error {
	var errs []error
	if target.UserID <= 0 {
		errs = append(errs, fmt.Errorf("%w: UserID is required", ErrDeviceSessionInvalid))
	}
	if err := validateDeviceIdentifier("device ID", target.DeviceID); err != nil {
		errs = append(errs, fmt.Errorf("%w: %v", ErrDeviceSessionInvalid, err))
	}
	if currentSessionID := strings.TrimSpace(target.CurrentSessionID); currentSessionID != "" {
		if err := validateDeviceIdentifier("current session ID", currentSessionID); err != nil {
			errs = append(errs, fmt.Errorf("%w: %v", ErrDeviceSessionInvalid, err))
		}
	}
	return errors.Join(errs...)
}

func deviceSessionActiveAt(session DeviceSession, now time.Time) bool {
	return session.RevokedAt.IsZero() && session.ExpiresAt.After(now)
}

func deviceSessionAfter(a, b DeviceSession) bool {
	aObserved := deviceSessionObservedAt(a)
	bObserved := deviceSessionObservedAt(b)
	if !aObserved.Equal(bObserved) {
		return aObserved.After(bObserved)
	}
	return a.SessionID < b.SessionID
}

func deviceSessionObservedAt(session DeviceSession) time.Time {
	if !session.LastSeenAt.IsZero() {
		return session.LastSeenAt
	}
	if !session.CreatedAt.IsZero() {
		return session.CreatedAt
	}
	return session.ExpiresAt
}

func normalizeDeviceSessionTime(t time.Time) time.Time {
	if t.IsZero() {
		return time.Now().UTC()
	}
	return t.UTC()
}

func validateDeviceIdentifier(kind, value string) error {
	value = strings.TrimSpace(value)
	if value == "" {
		return fmt.Errorf("%s is required", kind)
	}
	if len(value) > maxDeviceIdentifierLength {
		return fmt.Errorf("%s exceeds %d bytes", kind, maxDeviceIdentifierLength)
	}
	for _, r := range value {
		if r < 0x20 || r == 0x7f {
			return fmt.Errorf("%s contains a control character", kind)
		}
	}
	return nil
}

func isSHA256Hex(value string) bool {
	if len(value) != sha256.Size*2 {
		return false
	}
	_, err := hex.DecodeString(value)
	return err == nil
}

func signalTooLong(value string) bool {
	return len(value) > maxDeviceFingerprintSignalLength
}

func emptyDeviceFingerprint(f DeviceFingerprint) bool {
	return f.UserAgent == "" &&
		f.IPAddress == "" &&
		f.Platform == "" &&
		f.AcceptLanguage == "" &&
		f.Label == ""
}

func sortedMapKeys(values map[string]struct{}) []string {
	if len(values) == 0 {
		return nil
	}
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}
