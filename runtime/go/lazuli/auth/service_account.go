package auth

import (
	"encoding/json"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"
	"unicode"

	"lazuli.dev/runtime/lazuli"
)

const (
	// AuditActorServiceAccount is the audit actor kind used for service-account
	// principals. It is intentionally distinct from AuditActorService, which is
	// used for broader service integrations.
	AuditActorServiceAccount = "service_account"

	// ServiceAccountPrincipalKind is the stable principal namespace used in
	// service-account identifiers.
	ServiceAccountPrincipalKind = "service_account"

	maxServiceAccountIdentifierLength = 128
	maxServiceAccountTextLength       = 512
)

var (
	ErrServiceAccountPrincipalInvalid = errors.New("auth: service account principal invalid")
	ErrServiceAccountKeyInvalid       = errors.New("auth: service account key invalid")
	ErrServiceAccountKeyExpired       = errors.New("auth: service account key expired")
	ErrServiceAccountKeyNotYetValid   = errors.New("auth: service account key not yet valid")
	ErrServiceAccountKeyRevoked       = errors.New("auth: service account key revoked")
	ErrServiceAccountKeyRotated       = errors.New("auth: service account key rotated")
	ErrServiceAccountScopeDenied      = errors.New("auth: service account scope denied")
	ErrServiceAccountScopeInvalid     = ErrTokenInvalid
)

// ServiceAccountPrincipal describes the non-secret actor identity for a
// service account. Principal IDs are stable strings shaped as:
//
//	service_account:<id>
//	org:<org_id>:service_account:<id>
//	service_account:<name>
//	org:<org_id>:service_account:<name>
type ServiceAccountPrincipal struct {
	OrgID            lazuli.ID
	ServiceAccountID lazuli.ID
	Name             string
}

// PrincipalID returns the canonical service-account principal identifier.
func (p ServiceAccountPrincipal) PrincipalID() string {
	if p.OrgID > 0 {
		return ServiceAccountPrincipalIDForOrg(p.OrgID, p.ServiceAccountID, p.Name)
	}
	return ServiceAccountPrincipalID(p.ServiceAccountID, p.Name)
}

// AuditActorKind returns the actor kind used in audit rows.
func (p ServiceAccountPrincipal) AuditActorKind() string {
	return AuditActorServiceAccount
}

// ServiceAccountPrincipalID returns a canonical service-account principal ID.
// When accountID is positive it wins; otherwise name is used after validation.
// Invalid input returns the empty string.
func ServiceAccountPrincipalID(accountID lazuli.ID, name ...string) string {
	token, ok := serviceAccountPrincipalToken(accountID, name...)
	if !ok {
		return ""
	}
	return ServiceAccountPrincipalKind + ":" + token
}

// ServiceAccountPrincipalIDForOrg returns a tenant-scoped service-account
// principal ID. Invalid input returns the empty string.
func ServiceAccountPrincipalIDForOrg(orgID, accountID lazuli.ID, name ...string) string {
	if orgID <= 0 {
		return ""
	}
	token, ok := serviceAccountPrincipalToken(accountID, name...)
	if !ok {
		return ""
	}
	return "org:" + strconv.FormatInt(int64(orgID), 10) + ":" + ServiceAccountPrincipalKind + ":" + token
}

// NamedServiceAccountPrincipalID returns a canonical principal ID for an app or
// integration-level service account that is identified by name instead of a
// numeric resource id.
func NamedServiceAccountPrincipalID(name string) (string, error) {
	if err := validateServiceAccountIdentifier("service account name", name); err != nil {
		return "", fmt.Errorf("%w: %v", ErrServiceAccountPrincipalInvalid, err)
	}
	return ServiceAccountPrincipalKind + ":" + strings.TrimSpace(name), nil
}

// ParseServiceAccountPrincipalID parses the canonical principal ID into its
// typed metadata.
func ParseServiceAccountPrincipalID(principalID string) (ServiceAccountPrincipal, error) {
	parts := strings.Split(strings.TrimSpace(principalID), ":")
	switch len(parts) {
	case 2:
		if parts[0] != ServiceAccountPrincipalKind {
			return ServiceAccountPrincipal{}, ErrServiceAccountPrincipalInvalid
		}
		principal, err := parseServiceAccountPrincipalToken(parts[1])
		if err != nil {
			return ServiceAccountPrincipal{}, err
		}
		return principal, nil
	case 4:
		if parts[0] != "org" || parts[2] != ServiceAccountPrincipalKind {
			return ServiceAccountPrincipal{}, ErrServiceAccountPrincipalInvalid
		}
		orgID, err := parsePositiveLazuliID("org ID", parts[1], ErrServiceAccountPrincipalInvalid)
		if err != nil {
			return ServiceAccountPrincipal{}, err
		}
		principal, err := parseServiceAccountPrincipalToken(parts[3])
		if err != nil {
			return ServiceAccountPrincipal{}, err
		}
		principal.OrgID = orgID
		return principal, nil
	default:
		return ServiceAccountPrincipal{}, ErrServiceAccountPrincipalInvalid
	}
}

// ValidateServiceAccountPrincipalID reports malformed service-account
// principal identifiers.
func ValidateServiceAccountPrincipalID(principalID string) error {
	_, err := ParseServiceAccountPrincipalID(principalID)
	return err
}

// ServiceAccountScopes is the grant set attached to a service account or one
// of its keys. Scope matching mirrors API token scopes: exact grants match
// exactly, and '*' is honored only on the grant side.
type ServiceAccountScopes []string

// Has reports whether scopes grant required.
func (scopes ServiceAccountScopes) Has(required string) bool {
	return ServiceAccountHasScope(scopes, required)
}

// Allows is an alias for Has.
func (scopes ServiceAccountScopes) Allows(required string) bool {
	return scopes.Has(required)
}

// HasAll reports whether scopes grant every required scope.
func (scopes ServiceAccountScopes) HasAll(required ...string) bool {
	for _, scope := range required {
		if !scopes.Has(scope) {
			return false
		}
	}
	return true
}

// HasAny reports whether scopes grant at least one required scope.
func (scopes ServiceAccountScopes) HasAny(required ...string) bool {
	for _, scope := range required {
		if scopes.Has(scope) {
			return true
		}
	}
	return false
}

// NormalizeServiceAccountScopes trims scopes, drops empty entries, and removes
// duplicates while preserving first occurrence order.
func NormalizeServiceAccountScopes(scopes []string) ServiceAccountScopes {
	normalized := NormalizeAPITokenScopes(scopes)
	return ServiceAccountScopes(normalized)
}

// ValidateServiceAccountScope validates a runtime scope atom.
func ValidateServiceAccountScope(scope string) error {
	return ValidateAPITokenScope(scope)
}

// ValidateServiceAccountScopes validates a service-account grant list.
func ValidateServiceAccountScopes(scopes []string) error {
	for _, scope := range scopes {
		if err := ValidateServiceAccountScope(scope); err != nil {
			return err
		}
	}
	return nil
}

// MatchServiceAccountScope reports whether a granted scope pattern covers a
// required concrete scope.
func MatchServiceAccountScope(granted, required string) bool {
	return MatchAPITokenScope(granted, required)
}

// ServiceAccountHasScope reports whether grants contain a scope that covers
// required.
func ServiceAccountHasScope(grants []string, required string) bool {
	return APITokenHasScope(grants, required)
}

// ServiceAccountKeyStatus is the derived lifecycle state for a service-account
// key.
type ServiceAccountKeyStatus string

const (
	ServiceAccountKeyStatusActive      ServiceAccountKeyStatus = "active"
	ServiceAccountKeyStatusNotYetValid ServiceAccountKeyStatus = "not_yet_valid"
	ServiceAccountKeyStatusExpired     ServiceAccountKeyStatus = "expired"
	ServiceAccountKeyStatusRotated     ServiceAccountKeyStatus = "rotated"
	ServiceAccountKeyStatusRevoked     ServiceAccountKeyStatus = "revoked"
)

// ServiceAccountKeyMetadata carries non-secret key state. Raw private keys,
// bearer tokens, signatures, and signing material do not belong in this struct.
type ServiceAccountKeyMetadata struct {
	// ID is a storage-row or adapter id. KeyID is the preferred key identifier
	// when both are available; if KeyID is empty, ID is used as the key id.
	ID    string
	KeyID string

	ServiceAccountID lazuli.ID
	OrgID            lazuli.ID
	PrincipalID      string

	Name        string
	KeyName     string
	Algorithm   string
	Fingerprint string

	Scopes ServiceAccountScopes

	CreatedAt        time.Time
	NotBefore        time.Time
	ExpiresAt        time.Time
	LastUsedAt       time.Time
	RotationDueAt    time.Time
	RotatedAt        time.Time
	RevokedAt        time.Time
	ReplacementKeyID string

	Attrs map[string]any
}

// Clone returns metadata with cloned scope and attr containers.
func (m ServiceAccountKeyMetadata) Clone() ServiceAccountKeyMetadata {
	m.Scopes = append(ServiceAccountScopes(nil), m.Scopes...)
	m.Attrs = cloneSessionAttrs(m.Attrs)
	return m
}

// KeyIdentifier returns the audit-safe key identifier, preferring KeyID over ID.
func (m ServiceAccountKeyMetadata) KeyIdentifier() string {
	if keyID := strings.TrimSpace(m.KeyID); keyID != "" {
		return keyID
	}
	return strings.TrimSpace(m.ID)
}

// Principal returns parsed PrincipalID metadata when available, otherwise it
// derives a principal from OrgID, ServiceAccountID, and Name.
func (m ServiceAccountKeyMetadata) Principal() ServiceAccountPrincipal {
	if principal, err := ParseServiceAccountPrincipalID(m.PrincipalID); err == nil {
		return principal
	}
	return ServiceAccountPrincipal{
		OrgID:            m.OrgID,
		ServiceAccountID: m.ServiceAccountID,
		Name:             strings.TrimSpace(m.Name),
	}
}

// Expired reports whether the key is past ExpiresAt. A zero ExpiresAt means no
// expiry is enforced by this helper.
func (m ServiceAccountKeyMetadata) Expired(now time.Time) bool {
	now = normalizeServiceAccountTime(now)
	return !m.ExpiresAt.IsZero() && !m.ExpiresAt.After(now)
}

// IsExpired is an alias for Expired.
func (m ServiceAccountKeyMetadata) IsExpired(now time.Time) bool {
	return m.Expired(now)
}

// NotYetValid reports whether NotBefore is in the future.
func (m ServiceAccountKeyMetadata) NotYetValid(now time.Time) bool {
	now = normalizeServiceAccountTime(now)
	return !m.NotBefore.IsZero() && m.NotBefore.After(now)
}

// IsNotYetValid is an alias for NotYetValid.
func (m ServiceAccountKeyMetadata) IsNotYetValid(now time.Time) bool {
	return m.NotYetValid(now)
}

// Revoked reports whether the key has been explicitly revoked.
func (m ServiceAccountKeyMetadata) Revoked() bool {
	return !m.RevokedAt.IsZero()
}

// IsRevoked is an alias for Revoked.
func (m ServiceAccountKeyMetadata) IsRevoked() bool {
	return m.Revoked()
}

// Rotated reports whether the key has been superseded by rotation.
func (m ServiceAccountKeyMetadata) Rotated() bool {
	return !m.RotatedAt.IsZero() || strings.TrimSpace(m.ReplacementKeyID) != ""
}

// IsRotated is an alias for Rotated.
func (m ServiceAccountKeyMetadata) IsRotated() bool {
	return m.Rotated()
}

// Status returns the key lifecycle status at now. Revocation wins over
// rotation, and rotation wins over activation and expiry checks.
func (m ServiceAccountKeyMetadata) Status(now time.Time) ServiceAccountKeyStatus {
	switch {
	case m.Revoked():
		return ServiceAccountKeyStatusRevoked
	case m.Rotated():
		return ServiceAccountKeyStatusRotated
	case m.NotYetValid(now):
		return ServiceAccountKeyStatusNotYetValid
	case m.Expired(now):
		return ServiceAccountKeyStatusExpired
	default:
		return ServiceAccountKeyStatusActive
	}
}

// HasScope reports whether the key grants required.
func (m ServiceAccountKeyMetadata) HasScope(required string) bool {
	return m.Scopes.Has(required)
}

// AllowsScope is an alias for HasScope.
func (m ServiceAccountKeyMetadata) AllowsScope(required string) bool {
	return m.HasScope(required)
}

// Validate returns nil when the key is active and grants every required scope.
func (m ServiceAccountKeyMetadata) Validate(now time.Time, requiredScopes ...string) error {
	return ValidateServiceAccountKey(m, now, requiredScopes...)
}

// ValidateMetadata reports malformed key metadata before an adapter persists or
// consumes it.
func (m ServiceAccountKeyMetadata) ValidateMetadata() error {
	return ValidateServiceAccountKeyMetadata(m)
}

// ValidateServiceAccountKey returns nil when metadata describes an active key
// that grants every required scope.
func ValidateServiceAccountKey(meta ServiceAccountKeyMetadata, now time.Time, requiredScopes ...string) error {
	switch meta.Status(now) {
	case ServiceAccountKeyStatusRevoked:
		return ErrServiceAccountKeyRevoked
	case ServiceAccountKeyStatusRotated:
		return ErrServiceAccountKeyRotated
	case ServiceAccountKeyStatusNotYetValid:
		return ErrServiceAccountKeyNotYetValid
	case ServiceAccountKeyStatusExpired:
		return ErrServiceAccountKeyExpired
	}

	for _, required := range requiredScopes {
		if err := ValidateServiceAccountScope(required); err != nil {
			return err
		}
		if !meta.HasScope(required) {
			return ErrServiceAccountScopeDenied
		}
	}
	return nil
}

// ValidateServiceAccountKeyMetadata reports malformed non-secret key metadata.
func ValidateServiceAccountKeyMetadata(m ServiceAccountKeyMetadata) error {
	var errs []error

	id := strings.TrimSpace(m.ID)
	keyID := strings.TrimSpace(m.KeyID)
	if id == "" && keyID == "" {
		errs = append(errs, fmt.Errorf("%w: key id is required", ErrServiceAccountKeyInvalid))
	}
	if id != "" {
		if err := validateServiceAccountIdentifier("ID", id); err != nil {
			errs = append(errs, fmt.Errorf("%w: %v", ErrServiceAccountKeyInvalid, err))
		}
	}
	if keyID != "" {
		if err := validateServiceAccountIdentifier("KeyID", keyID); err != nil {
			errs = append(errs, fmt.Errorf("%w: %v", ErrServiceAccountKeyInvalid, err))
		}
	}
	if id != "" && keyID != "" && id != keyID {
		errs = append(errs, fmt.Errorf("%w: ID and KeyID must match when both are set", ErrServiceAccountKeyInvalid))
	}

	principal, principalErr := ParseServiceAccountPrincipalID(m.PrincipalID)
	if strings.TrimSpace(m.PrincipalID) == "" {
		principalErr = nil
		principal = ServiceAccountPrincipal{OrgID: m.OrgID, ServiceAccountID: m.ServiceAccountID, Name: strings.TrimSpace(m.Name)}
	}
	if principalErr != nil {
		errs = append(errs, fmt.Errorf("%w: PrincipalID is invalid", ErrServiceAccountKeyInvalid))
	}
	if principal.PrincipalID() == "" {
		errs = append(errs, fmt.Errorf("%w: PrincipalID or ServiceAccountID is required", ErrServiceAccountKeyInvalid))
	}
	if principal.ServiceAccountID > 0 && m.ServiceAccountID > 0 && principal.ServiceAccountID != m.ServiceAccountID {
		errs = append(errs, fmt.Errorf("%w: PrincipalID does not match ServiceAccountID", ErrServiceAccountKeyInvalid))
	}
	if principal.OrgID > 0 && m.OrgID > 0 && principal.OrgID != m.OrgID {
		errs = append(errs, fmt.Errorf("%w: PrincipalID does not match OrgID", ErrServiceAccountKeyInvalid))
	}
	if m.ServiceAccountID < 0 {
		errs = append(errs, fmt.Errorf("%w: ServiceAccountID must not be negative", ErrServiceAccountKeyInvalid))
	}
	if m.OrgID < 0 {
		errs = append(errs, fmt.Errorf("%w: OrgID must not be negative", ErrServiceAccountKeyInvalid))
	}

	if err := ValidateServiceAccountScopes(m.Scopes); err != nil {
		errs = append(errs, fmt.Errorf("%w: scopes invalid", ErrServiceAccountKeyInvalid))
	}
	if m.CreatedAt.IsZero() {
		errs = append(errs, fmt.Errorf("%w: CreatedAt is required", ErrServiceAccountKeyInvalid))
	}
	if !m.NotBefore.IsZero() && !m.CreatedAt.IsZero() && m.NotBefore.Before(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: NotBefore must not be before CreatedAt", ErrServiceAccountKeyInvalid))
	}
	if !m.ExpiresAt.IsZero() && !m.CreatedAt.IsZero() && !m.ExpiresAt.After(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: ExpiresAt must be after CreatedAt", ErrServiceAccountKeyInvalid))
	}
	if !m.ExpiresAt.IsZero() && !m.NotBefore.IsZero() && !m.ExpiresAt.After(m.NotBefore) {
		errs = append(errs, fmt.Errorf("%w: ExpiresAt must be after NotBefore", ErrServiceAccountKeyInvalid))
	}
	if !m.LastUsedAt.IsZero() && !m.CreatedAt.IsZero() && m.LastUsedAt.Before(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: LastUsedAt must not be before CreatedAt", ErrServiceAccountKeyInvalid))
	}
	if !m.LastUsedAt.IsZero() && !m.ExpiresAt.IsZero() && m.LastUsedAt.After(m.ExpiresAt) {
		errs = append(errs, fmt.Errorf("%w: LastUsedAt must not be after ExpiresAt", ErrServiceAccountKeyInvalid))
	}
	if !m.RotationDueAt.IsZero() && !m.CreatedAt.IsZero() && m.RotationDueAt.Before(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: RotationDueAt must not be before CreatedAt", ErrServiceAccountKeyInvalid))
	}
	if !m.RotatedAt.IsZero() && !m.CreatedAt.IsZero() && m.RotatedAt.Before(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: RotatedAt must not be before CreatedAt", ErrServiceAccountKeyInvalid))
	}
	if !m.RevokedAt.IsZero() && !m.CreatedAt.IsZero() && m.RevokedAt.Before(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: RevokedAt must not be before CreatedAt", ErrServiceAccountKeyInvalid))
	}
	if replacement := strings.TrimSpace(m.ReplacementKeyID); replacement != "" {
		if err := validateServiceAccountIdentifier("ReplacementKeyID", replacement); err != nil {
			errs = append(errs, fmt.Errorf("%w: %v", ErrServiceAccountKeyInvalid, err))
		}
	}
	if err := validateOptionalServiceAccountText("Algorithm", m.Algorithm); err != nil {
		errs = append(errs, fmt.Errorf("%w: %v", ErrServiceAccountKeyInvalid, err))
	}
	if err := validateOptionalServiceAccountText("Fingerprint", m.Fingerprint); err != nil {
		errs = append(errs, fmt.Errorf("%w: %v", ErrServiceAccountKeyInvalid, err))
	}

	return errors.Join(errs...)
}

// ServiceAccountKeyRotationReason names a rule that selected a key for
// rotation.
type ServiceAccountKeyRotationReason string

const (
	ServiceAccountKeyRotationReasonMaxAge      ServiceAccountKeyRotationReason = "max_age"
	ServiceAccountKeyRotationReasonExpiresSoon ServiceAccountKeyRotationReason = "expires_soon"
	ServiceAccountKeyRotationReasonRotationDue ServiceAccountKeyRotationReason = "rotation_due"
	ServiceAccountKeyRotationReasonExpired     ServiceAccountKeyRotationReason = "expired"
)

// ServiceAccountKeyRotationPolicy controls storage-agnostic key rotation.
type ServiceAccountKeyRotationPolicy struct {
	// MaxAge rotates at or after CreatedAt plus this duration. Non-positive
	// values disable max-age rotation.
	MaxAge time.Duration
	// RotateBeforeExpiry opens the rotation window before ExpiresAt. Non-positive
	// values disable expiry-lead rotation.
	RotateBeforeExpiry time.Duration
	// GracePeriod is reported on the computed window for adapters that allow old
	// and new keys to overlap. It does not make expired keys validate.
	GracePeriod time.Duration
}

// ServiceAccountKeyRotationWindow describes when adapters should issue a
// replacement key.
type ServiceAccountKeyRotationWindow struct {
	OpensAt    time.Time
	ClosesAt   time.Time
	GraceUntil time.Time
}

// Contains reports whether now is inside the pre-expiry rotation window.
func (w ServiceAccountKeyRotationWindow) Contains(now time.Time) bool {
	now = normalizeServiceAccountTime(now)
	if w.OpensAt.IsZero() || w.OpensAt.After(now) {
		return false
	}
	return w.ClosesAt.IsZero() || now.Before(w.ClosesAt)
}

// Open is an alias for Contains.
func (w ServiceAccountKeyRotationWindow) Open(now time.Time) bool {
	return w.Contains(now)
}

// PastDue reports whether the key is at or beyond the window close.
func (w ServiceAccountKeyRotationWindow) PastDue(now time.Time) bool {
	now = normalizeServiceAccountTime(now)
	return !w.ClosesAt.IsZero() && !w.ClosesAt.After(now)
}

// Expired is an alias for PastDue.
func (w ServiceAccountKeyRotationWindow) Expired(now time.Time) bool {
	return w.PastDue(now)
}

// ServiceAccountKeyRotationPlan is a dry-run decision concrete adapters can
// apply by issuing a replacement key and retiring the old one.
type ServiceAccountKeyRotationPlan struct {
	GeneratedAt time.Time
	Rotate      bool
	Status      ServiceAccountKeyStatus
	Reasons     []ServiceAccountKeyRotationReason
	Window      ServiceAccountKeyRotationWindow
	KeyID       string
	PrincipalID string
}

// RotationWindow returns the configured key rotation window.
func (m ServiceAccountKeyMetadata) RotationWindow(policy ServiceAccountKeyRotationPolicy) ServiceAccountKeyRotationWindow {
	return BuildServiceAccountKeyRotationWindow(m, policy)
}

// BuildServiceAccountKeyRotationWindow returns the earliest configured
// rotation-open time and the expiry close time for a key.
func BuildServiceAccountKeyRotationWindow(
	meta ServiceAccountKeyMetadata,
	policy ServiceAccountKeyRotationPolicy,
) ServiceAccountKeyRotationWindow {
	var candidates []time.Time
	if !meta.RotationDueAt.IsZero() {
		candidates = append(candidates, meta.RotationDueAt.UTC())
	}
	if !meta.CreatedAt.IsZero() && policy.MaxAge > 0 {
		candidates = append(candidates, meta.CreatedAt.Add(policy.MaxAge).UTC())
	}
	if !meta.ExpiresAt.IsZero() && policy.RotateBeforeExpiry > 0 {
		candidates = append(candidates, meta.ExpiresAt.Add(-policy.RotateBeforeExpiry).UTC())
	}

	opensAt := earliestServiceAccountTime(candidates...)
	if !opensAt.IsZero() && !meta.CreatedAt.IsZero() && opensAt.Before(meta.CreatedAt) {
		opensAt = meta.CreatedAt.UTC()
	}

	window := ServiceAccountKeyRotationWindow{OpensAt: opensAt}
	if !meta.ExpiresAt.IsZero() {
		window.ClosesAt = meta.ExpiresAt.UTC()
		if policy.GracePeriod > 0 {
			window.GraceUntil = meta.ExpiresAt.Add(policy.GracePeriod).UTC()
		}
	}
	return window
}

// ShouldRotateServiceAccountKey reports whether the key should rotate at now.
func ShouldRotateServiceAccountKey(
	now time.Time,
	meta ServiceAccountKeyMetadata,
	policy ServiceAccountKeyRotationPolicy,
) bool {
	return PlanServiceAccountKeyRotation(now, meta, policy).Rotate
}

// PlanServiceAccountKeyRotation returns a storage-agnostic rotation decision.
func PlanServiceAccountKeyRotation(
	now time.Time,
	meta ServiceAccountKeyMetadata,
	policy ServiceAccountKeyRotationPolicy,
) ServiceAccountKeyRotationPlan {
	now = normalizeServiceAccountTime(now)
	status := meta.Status(now)
	plan := ServiceAccountKeyRotationPlan{
		GeneratedAt: now,
		Status:      status,
		Window:      BuildServiceAccountKeyRotationWindow(meta, policy),
		KeyID:       meta.KeyIdentifier(),
		PrincipalID: meta.Principal().PrincipalID(),
	}
	if status == ServiceAccountKeyStatusRevoked ||
		status == ServiceAccountKeyStatusRotated ||
		status == ServiceAccountKeyStatusNotYetValid {
		return plan
	}

	reasons := make([]ServiceAccountKeyRotationReason, 0, 4)
	if !meta.ExpiresAt.IsZero() && !meta.ExpiresAt.After(now) {
		reasons = append(reasons, ServiceAccountKeyRotationReasonExpired)
	}
	if !meta.RotationDueAt.IsZero() && !meta.RotationDueAt.After(now) {
		reasons = append(reasons, ServiceAccountKeyRotationReasonRotationDue)
	}
	if !meta.CreatedAt.IsZero() && policy.MaxAge > 0 && !meta.CreatedAt.Add(policy.MaxAge).After(now) {
		reasons = append(reasons, ServiceAccountKeyRotationReasonMaxAge)
	}
	if !meta.ExpiresAt.IsZero() && policy.RotateBeforeExpiry > 0 && !meta.ExpiresAt.Add(-policy.RotateBeforeExpiry).After(now) {
		reasons = append(reasons, ServiceAccountKeyRotationReasonExpiresSoon)
	}

	plan.Reasons = serviceAccountRotationReasonsUnique(reasons)
	plan.Rotate = len(plan.Reasons) > 0
	return plan
}

// ServiceAccountKeyDisplay is a non-secret display shape suitable for audit
// payloads and admin listings. Attrs are intentionally omitted because adapters
// may use them for provider-specific sensitive data.
type ServiceAccountKeyDisplay struct {
	PrincipalID      string                  `json:"principal_id,omitempty"`
	ServiceAccountID lazuli.ID               `json:"service_account_id,omitempty"`
	OrgID            lazuli.ID               `json:"org_id,omitempty"`
	KeyID            string                  `json:"key_id,omitempty"`
	Name             string                  `json:"name,omitempty"`
	KeyName          string                  `json:"key_name,omitempty"`
	Algorithm        string                  `json:"algorithm,omitempty"`
	Fingerprint      string                  `json:"fingerprint,omitempty"`
	Status           ServiceAccountKeyStatus `json:"status,omitempty"`
	Scopes           ServiceAccountScopes    `json:"scopes,omitempty"`
	CreatedAt        time.Time               `json:"created_at,omitempty"`
	NotBefore        time.Time               `json:"not_before,omitempty"`
	ExpiresAt        time.Time               `json:"expires_at,omitempty"`
	LastUsedAt       time.Time               `json:"last_used_at,omitempty"`
	RotationDueAt    time.Time               `json:"rotation_due_at,omitempty"`
	RotatedAt        time.Time               `json:"rotated_at,omitempty"`
	RevokedAt        time.Time               `json:"revoked_at,omitempty"`
	ReplacementKeyID string                  `json:"replacement_key_id,omitempty"`
}

// AuditDisplay returns a non-secret display shape for audit/admin surfaces.
func (m ServiceAccountKeyMetadata) AuditDisplay(now time.Time) ServiceAccountKeyDisplay {
	return AuditSafeServiceAccountKeyDisplay(m, now)
}

// AuditSafeDisplay is an alias for AuditDisplay.
func (m ServiceAccountKeyMetadata) AuditSafeDisplay(now time.Time) ServiceAccountKeyDisplay {
	return m.AuditDisplay(now)
}

// AuditSafeServiceAccountKeyDisplay returns a non-secret display shape for
// service-account key metadata.
func AuditSafeServiceAccountKeyDisplay(meta ServiceAccountKeyMetadata, now time.Time) ServiceAccountKeyDisplay {
	principal := meta.Principal()
	return ServiceAccountKeyDisplay{
		PrincipalID:      principal.PrincipalID(),
		ServiceAccountID: firstNonZeroID(meta.ServiceAccountID, principal.ServiceAccountID),
		OrgID:            firstNonZeroID(meta.OrgID, principal.OrgID),
		KeyID:            meta.KeyIdentifier(),
		Name:             strings.TrimSpace(meta.Name),
		KeyName:          strings.TrimSpace(meta.KeyName),
		Algorithm:        strings.TrimSpace(meta.Algorithm),
		Fingerprint:      strings.TrimSpace(meta.Fingerprint),
		Status:           meta.Status(now),
		Scopes:           NormalizeServiceAccountScopes(meta.Scopes),
		CreatedAt:        utcOrZero(meta.CreatedAt),
		NotBefore:        utcOrZero(meta.NotBefore),
		ExpiresAt:        utcOrZero(meta.ExpiresAt),
		LastUsedAt:       utcOrZero(meta.LastUsedAt),
		RotationDueAt:    utcOrZero(meta.RotationDueAt),
		RotatedAt:        utcOrZero(meta.RotatedAt),
		RevokedAt:        utcOrZero(meta.RevokedAt),
		ReplacementKeyID: strings.TrimSpace(meta.ReplacementKeyID),
	}
}

// BuildServiceAccountKeyAuditPayload marshals the audit-safe display shape as
// JSON. It excludes Attrs and any signing material by construction.
func BuildServiceAccountKeyAuditPayload(meta ServiceAccountKeyMetadata, now time.Time) ([]byte, error) {
	return json.Marshal(AuditSafeServiceAccountKeyDisplay(meta, now))
}

func serviceAccountPrincipalToken(accountID lazuli.ID, name ...string) (string, bool) {
	if accountID > 0 {
		return strconv.FormatInt(int64(accountID), 10), true
	}
	if len(name) == 0 {
		return "", false
	}
	value := strings.TrimSpace(name[0])
	if validateServiceAccountIdentifier("service account name", value) != nil {
		return "", false
	}
	return value, true
}

func parseServiceAccountPrincipalToken(token string) (ServiceAccountPrincipal, error) {
	token = strings.TrimSpace(token)
	if token == "" {
		return ServiceAccountPrincipal{}, ErrServiceAccountPrincipalInvalid
	}
	if id, err := strconv.ParseInt(token, 10, 64); err == nil {
		if id <= 0 {
			return ServiceAccountPrincipal{}, ErrServiceAccountPrincipalInvalid
		}
		return ServiceAccountPrincipal{ServiceAccountID: lazuli.ID(id)}, nil
	}
	if err := validateServiceAccountIdentifier("service account name", token); err != nil {
		return ServiceAccountPrincipal{}, fmt.Errorf("%w: %v", ErrServiceAccountPrincipalInvalid, err)
	}
	return ServiceAccountPrincipal{Name: token}, nil
}

func parsePositiveLazuliID(kind, token string, sentinel error) (lazuli.ID, error) {
	id, err := strconv.ParseInt(strings.TrimSpace(token), 10, 64)
	if err != nil || id <= 0 {
		return 0, fmt.Errorf("%w: %s is invalid", sentinel, kind)
	}
	return lazuli.ID(id), nil
}

func validateServiceAccountIdentifier(kind, value string) error {
	value = strings.TrimSpace(value)
	if value == "" {
		return fmt.Errorf("%s is required", kind)
	}
	if len(value) > maxServiceAccountIdentifierLength {
		return fmt.Errorf("%s exceeds %d bytes", kind, maxServiceAccountIdentifierLength)
	}
	for _, r := range value {
		if r == ':' || unicode.IsSpace(r) || r < 0x20 || r == 0x7f {
			return fmt.Errorf("%s contains an invalid character", kind)
		}
	}
	return nil
}

func validateOptionalServiceAccountText(kind, value string) error {
	value = strings.TrimSpace(value)
	if value == "" {
		return nil
	}
	if len(value) > maxServiceAccountTextLength {
		return fmt.Errorf("%s exceeds %d bytes", kind, maxServiceAccountTextLength)
	}
	for _, r := range value {
		if r < 0x20 || r == 0x7f {
			return fmt.Errorf("%s contains a control character", kind)
		}
	}
	return nil
}

func normalizeServiceAccountTime(t time.Time) time.Time {
	if t.IsZero() {
		return time.Now().UTC()
	}
	return t.UTC()
}

func earliestServiceAccountTime(times ...time.Time) time.Time {
	var earliest time.Time
	for _, t := range times {
		if t.IsZero() {
			continue
		}
		t = t.UTC()
		if earliest.IsZero() || t.Before(earliest) {
			earliest = t
		}
	}
	return earliest
}

func serviceAccountRotationReasonsUnique(reasons []ServiceAccountKeyRotationReason) []ServiceAccountKeyRotationReason {
	if len(reasons) == 0 {
		return nil
	}
	unique := make([]ServiceAccountKeyRotationReason, 0, len(reasons))
	seen := make(map[ServiceAccountKeyRotationReason]struct{}, len(reasons))
	for _, reason := range reasons {
		if _, ok := seen[reason]; ok {
			continue
		}
		seen[reason] = struct{}{}
		unique = append(unique, reason)
	}
	return unique
}

func firstNonZeroID(a, b lazuli.ID) lazuli.ID {
	if a != 0 {
		return a
	}
	return b
}

func utcOrZero(t time.Time) time.Time {
	if t.IsZero() {
		return time.Time{}
	}
	return t.UTC()
}
