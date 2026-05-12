package lazuli

import (
	"errors"
	"net/http"
	"strings"
)

// Error origin strings used by the runtime error catalog.
//
// EXPERIMENTAL: these mirror the planned Origin enum without depending on the
// typed hierarchy and may be replaced by that enum before 1.0.
const (
	ErrorOriginUserDSL        = "user_dsl"
	ErrorOriginLibInternal    = "lib_internal"
	ErrorOriginCodegenBug     = "codegen_bug"
	ErrorOriginAdapterRuntime = "adapter_runtime"
)

// CodeUncataloguedSentinel is returned when a bare runtime sentinel reaches
// a boundary without a stable catalog entry.
//
// EXPERIMENTAL: subject to change before 1.0.
const CodeUncataloguedSentinel = "uncatalogued_sentinel"

// ErrorClassification is the stable code/status/origin tuple assigned to a
// runtime error.
//
// EXPERIMENTAL: subject to change before 1.0.
type ErrorClassification struct {
	Code   string
	Status int
	Origin string
}

// ErrorCatalogEntry is one stable sentinel registration in the runtime
// classifier catalog. Sentinel is the exact sentinel Error() string.
//
// EXPERIMENTAL: subject to change before 1.0.
type ErrorCatalogEntry struct {
	Sentinel string
	Code     string
	Status   int
	Origin   string
}

var errorCodeCatalog = map[string]ErrorClassification{
	CodePolicyDenied:         {Code: CodePolicyDenied, Status: http.StatusForbidden, Origin: ErrorOriginUserDSL},
	CodeRateLimited:          {Code: CodeRateLimited, Status: http.StatusTooManyRequests, Origin: ErrorOriginUserDSL},
	CodeValidationFailed:     {Code: CodeValidationFailed, Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	CodeNotFound:             {Code: CodeNotFound, Status: http.StatusNotFound, Origin: ErrorOriginUserDSL},
	CodeTenantMismatch:       {Code: CodeTenantMismatch, Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	CodeInternal:             {Code: CodeInternal, Status: http.StatusInternalServerError, Origin: ErrorOriginLibInternal},
	CodeBadRequest:           {Code: CodeBadRequest, Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	CodeMethodNotAllowed:     {Code: CodeMethodNotAllowed, Status: http.StatusMethodNotAllowed, Origin: ErrorOriginUserDSL},
	CodeIntegrationError:     {Code: CodeIntegrationError, Status: http.StatusBadGateway, Origin: ErrorOriginAdapterRuntime},
	CodeUncataloguedSentinel: {Code: CodeUncataloguedSentinel, Status: http.StatusInternalServerError, Origin: ErrorOriginLibInternal},
}

var runtimeSentinelCatalog = []ErrorCatalogEntry{
	{Sentinel: "auth: password mismatch", Code: "auth.password_mismatch", Status: http.StatusUnauthorized, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: password rate limited", Code: "auth.rate_limited", Status: http.StatusTooManyRequests, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: password hash malformed", Code: "auth.password_hash_malformed", Status: http.StatusInternalServerError, Origin: ErrorOriginLibInternal},
	{Sentinel: "auth: password algorithm unsupported", Code: "auth.password_algorithm_unsupported", Status: http.StatusInternalServerError, Origin: ErrorOriginLibInternal},
	{Sentinel: "auth: session expired", Code: "auth.session_expired", Status: http.StatusUnauthorized, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: session not found", Code: "auth.session_unknown", Status: http.StatusUnauthorized, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: token invalid", Code: "auth.token_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: oauth state mismatch", Code: "auth.oauth_state_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: oauth adapter unregistered", Code: "auth.oauth_adapter_unbound", Status: http.StatusInternalServerError, Origin: ErrorOriginAdapterRuntime},
	{Sentinel: "auth: oauth provider unknown", Code: "auth.oauth_provider_unknown", Status: http.StatusInternalServerError, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: mfa code invalid", Code: "auth.mfa_code_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: mfa not enrolled", Code: "auth.mfa_not_enrolled", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: mfa method unsupported", Code: "auth.mfa_method_unsupported", Status: http.StatusInternalServerError, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: jwt invalid", Code: "auth.jwt_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: jwt expired", Code: "auth.jwt_expired", Status: http.StatusUnauthorized, Origin: ErrorOriginUserDSL},
	{Sentinel: "auth: jwt signature mismatch", Code: "auth.jwt_signature_mismatch", Status: http.StatusUnauthorized, Origin: ErrorOriginUserDSL},

	{Sentinel: "jobs: timeout", Code: "jobs.timeout", Status: http.StatusGatewayTimeout, Origin: ErrorOriginAdapterRuntime},
	{Sentinel: "jobs: retry budget exhausted", Code: "jobs.max_retries", Status: http.StatusInternalServerError, Origin: ErrorOriginAdapterRuntime},
	{Sentinel: "jobs: fanout enumeration failed", Code: "jobs.fanout_invalid", Status: http.StatusInternalServerError, Origin: ErrorOriginLibInternal},
	{Sentinel: "jobs: tenant_from unresolved", Code: "jobs.tenant_unresolved", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "jobs: invalid progress", Code: "jobs.progress_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "jobs: progress is terminal", Code: "jobs.progress_terminal", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "jobs: dead-letter entry not found", Code: "jobs.dead_letter_not_found", Status: http.StatusNotFound, Origin: ErrorOriginUserDSL},

	{Sentinel: "lazuli/storage: file_size_exceeded", Code: "storage.file_size_exceeded", Status: http.StatusRequestEntityTooLarge, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: file_mime_rejected", Code: "storage.file_mime_rejected", Status: http.StatusUnsupportedMediaType, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: file_not_found", Code: "storage.file_not_found", Status: http.StatusNotFound, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: signed_url_expired", Code: "storage.signed_url_expired", Status: http.StatusGone, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: visibility_mismatch", Code: "storage.visibility_mismatch", Status: http.StatusInternalServerError, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: multipart_part_size_invalid", Code: "storage.multipart_part_size_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: multipart_part_count_invalid", Code: "storage.multipart_part_count_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: multipart_part_order_invalid", Code: "storage.multipart_part_order_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: resumable_session_exists", Code: "storage.resumable_session_exists", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: resumable_session_not_found", Code: "storage.resumable_session_not_found", Status: http.StatusNotFound, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: resumable_session_expired", Code: "storage.resumable_session_expired", Status: http.StatusGone, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: resumable_session_closed", Code: "storage.resumable_session_closed", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: resumable_session_invalid", Code: "storage.resumable_session_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: resumable_range_invalid", Code: "storage.resumable_range_invalid", Status: http.StatusRequestedRangeNotSatisfiable, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: resumable_range_overlap", Code: "storage.resumable_range_overlap", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: resumable_range_gap", Code: "storage.resumable_range_gap", Status: http.StatusRequestedRangeNotSatisfiable, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: file_infected", Code: "storage.file_infected", Status: http.StatusUnprocessableEntity, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: file_blocked", Code: "storage.file_blocked", Status: http.StatusForbidden, Origin: ErrorOriginUserDSL},
	{Sentinel: "lazuli/storage: scan_unavailable", Code: "storage.scan_unavailable", Status: http.StatusServiceUnavailable, Origin: ErrorOriginAdapterRuntime},

	{Sentinel: "notifications: channel not supported by any bound adapter", Code: "notifications.channel_unsupported", Status: http.StatusInternalServerError, Origin: ErrorOriginAdapterRuntime},
	{Sentinel: "notifications: delivery failed after retries", Code: "notifications.delivery_failed", Status: http.StatusBadGateway, Origin: ErrorOriginAdapterRuntime},
	{Sentinel: "notifications: tenant_from unresolved", Code: "notifications.tenant_unresolved", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "notifications: duplicate idempotency key", Code: "notifications.idempotent", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "notifications: digest window full, flush required", Code: "notifications.digest_full", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "notifications: throttle bucket exhausted", Code: "notifications.throttle_exceeded", Status: http.StatusTooManyRequests, Origin: ErrorOriginUserDSL},
	{Sentinel: "notifications: invalid duration literal", Code: "notifications.invalid_duration", Status: http.StatusInternalServerError, Origin: ErrorOriginUserDSL},
	{Sentinel: "notifications: unsupported digest template strategy", Code: "notifications.digest_strategy_unsupported", Status: http.StatusInternalServerError, Origin: ErrorOriginUserDSL},
	{Sentinel: "notifications: registry is nil", Code: "notifications.registry_nil", Status: http.StatusInternalServerError, Origin: ErrorOriginLibInternal},
	{Sentinel: "notifications: no channels declared", Code: "notifications.no_channels", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "notifications: recipient unresolved", Code: "notifications.recipient_unresolved", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "notifications: idempotency key unresolved", Code: "notifications.idempotency_unresolved", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "notifications: in-app message not found", Code: "notifications.in_app_message_not_found", Status: http.StatusNotFound, Origin: ErrorOriginUserDSL},

	{Sentinel: "migrations: timeout", Code: "migrations.timeout", Status: http.StatusGatewayTimeout, Origin: ErrorOriginAdapterRuntime},
	{Sentinel: "migrations: lock acquisition timeout", Code: "migrations.lock_timeout", Status: http.StatusGatewayTimeout, Origin: ErrorOriginAdapterRuntime},
	{Sentinel: "migrations: retry budget exhausted", Code: "migrations.max_retries", Status: http.StatusInternalServerError, Origin: ErrorOriginAdapterRuntime},
	{Sentinel: "migrations: tenant axis unknown", Code: "migrations.tenant_axis_unknown", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: checkpoint snapshot stale", Code: "migrations.checkpoint_stale", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: lock name required", Code: "migrations.lock_name_required", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: lock ttl must be positive", Code: "migrations.lock_ttl_required", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: lock held", Code: "migrations.lock_held", Status: http.StatusLocked, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: lock ownership lost", Code: "migrations.lock_ownership_lost", Status: http.StatusConflict, Origin: ErrorOriginAdapterRuntime},
	{Sentinel: "migrations: duplicate migration id", Code: "migrations.duplicate_migration_id", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: migration record id required", Code: "migrations.record_id_required", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: invalid SQL identifier", Code: "migrations.invalid_sql_identifier", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: invalid reset mode", Code: "migrations.invalid_reset_mode", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: invalid drop behavior", Code: "migrations.invalid_drop_behavior", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "migrations: rollback migration not found", Code: "migrations.rollback_migration_not_found", Status: http.StatusNotFound, Origin: ErrorOriginUserDSL},

	{Sentinel: "webhooks: hmac verification failed", Code: "webhooks.hmac_invalid", Status: http.StatusUnauthorized, Origin: ErrorOriginUserDSL},
	{Sentinel: "webhooks: duplicate envelope", Code: "webhooks.duplicate_envelope", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "webhooks: tenant_from unresolved", Code: "webhooks.tenant_unscoped", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "webhooks: replay denied for this contract", Code: "webhooks.replay_denied", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "webhooks: replay window expired", Code: "webhooks.replay_window_expired", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
	{Sentinel: "webhooks: replay timestamp invalid", Code: "webhooks.replay_timestamp_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "webhooks: replay window invalid", Code: "webhooks.replay_window_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "webhooks: replay mode invalid", Code: "webhooks.replay_mode_invalid", Status: http.StatusBadRequest, Origin: ErrorOriginUserDSL},
	{Sentinel: "webhooks: dlq entry already exists", Code: "webhooks.dlq_duplicate", Status: http.StatusConflict, Origin: ErrorOriginUserDSL},
}

var runtimeSentinelCatalogByMessage = buildRuntimeSentinelCatalog(runtimeSentinelCatalog)

var runtimeSentinelPrefixMessages = map[string]struct{}{
	"webhooks: replay denied for this contract": {},
	"webhooks: replay window expired":           {},
	"webhooks: replay timestamp invalid":        {},
	"webhooks: replay window invalid":           {},
	"webhooks: replay mode invalid":             {},
}

// RuntimeErrorCatalog returns a copy of the closed runtime sentinel catalog.
//
// EXPERIMENTAL: subject to change before 1.0.
func RuntimeErrorCatalog() []ErrorCatalogEntry {
	out := make([]ErrorCatalogEntry, len(runtimeSentinelCatalog))
	copy(out, runtimeSentinelCatalog)
	return out
}

// ClassifyError returns the stable runtime classification for err. Lazuli
// Error values keep their declared code/status; known runtime sentinels use
// the closed catalog; everything else falls back to uncatalogued_sentinel.
//
// EXPERIMENTAL: subject to change before 1.0.
func ClassifyError(err error) ErrorClassification {
	if err == nil {
		return internalErrorClassification()
	}

	var le *Error
	if errors.As(err, &le) && le != nil {
		return ClassifyLazuliError(le)
	}
	return ClassifyUnknownError(err)
}

// ClassifyLazuliError classifies the legacy flat *Error envelope without
// depending on the experimental typed hierarchy.
//
// EXPERIMENTAL: subject to change before 1.0.
func ClassifyLazuliError(err *Error) ErrorClassification {
	if err == nil {
		return internalErrorClassification()
	}

	classification, ok := errorCodeCatalog[err.Code]
	if !ok {
		status := err.Status
		if !validHTTPStatus(status) {
			status = http.StatusInternalServerError
		}
		classification = ErrorClassification{
			Code:   err.Code,
			Status: status,
			Origin: originForStatus(status),
		}
	}
	if err.Code != "" {
		classification.Code = err.Code
	}
	if classification.Code == "" {
		classification.Code = CodeInternal
	}
	if validHTTPStatus(err.Status) {
		classification.Status = err.Status
	}
	if !validHTTPStatus(classification.Status) {
		classification.Status = http.StatusInternalServerError
	}
	if classification.Origin == "" {
		classification.Origin = originForStatus(classification.Status)
	}
	return classification
}

// ClassifyUnknownError classifies non-*Error values against the runtime
// sentinel catalog and returns the internal fallback for uncatalogued errors.
//
// EXPERIMENTAL: subject to change before 1.0.
func ClassifyUnknownError(err error) ErrorClassification {
	if classification, ok := ClassifyRuntimeSentinel(err); ok {
		return classification
	}
	return uncataloguedSentinelClassification()
}

// ClassifyRuntimeSentinel reports whether err wraps a registered runtime
// sentinel and returns its stable classification.
//
// EXPERIMENTAL: subject to change before 1.0.
func ClassifyRuntimeSentinel(err error) (ErrorClassification, bool) {
	if err == nil {
		return ErrorClassification{}, false
	}
	return classifyRuntimeSentinel(err)
}

func classifyRuntimeSentinel(err error) (ErrorClassification, bool) {
	if err == nil {
		return ErrorClassification{}, false
	}
	if classification, ok := classifyRuntimeSentinelMessage(err.Error()); ok {
		return classification, true
	}

	switch unwrapped := err.(type) {
	case interface{ Unwrap() []error }:
		for _, child := range unwrapped.Unwrap() {
			if classification, ok := classifyRuntimeSentinel(child); ok {
				return classification, true
			}
		}
	case interface{ Unwrap() error }:
		return classifyRuntimeSentinel(unwrapped.Unwrap())
	}
	return ErrorClassification{}, false
}

func classifyRuntimeSentinelMessage(message string) (ErrorClassification, bool) {
	if classification, ok := runtimeSentinelCatalogByMessage[message]; ok {
		return classification, true
	}
	for sentinel := range runtimeSentinelPrefixMessages {
		if strings.HasPrefix(message, sentinel+": ") {
			if classification, ok := runtimeSentinelCatalogByMessage[sentinel]; ok {
				return classification, true
			}
		}
	}
	return ErrorClassification{}, false
}

func buildRuntimeSentinelCatalog(entries []ErrorCatalogEntry) map[string]ErrorClassification {
	catalog := make(map[string]ErrorClassification, len(entries))
	for _, entry := range entries {
		if entry.Sentinel == "" {
			continue
		}
		catalog[entry.Sentinel] = ErrorClassification{
			Code:   entry.Code,
			Status: entry.Status,
			Origin: entry.Origin,
		}
	}
	return catalog
}

func internalErrorClassification() ErrorClassification {
	return errorCodeCatalog[CodeInternal]
}

func uncataloguedSentinelClassification() ErrorClassification {
	return errorCodeCatalog[CodeUncataloguedSentinel]
}

func validHTTPStatus(status int) bool {
	return status >= 100 && status <= 599
}

func originForStatus(status int) string {
	if status >= http.StatusInternalServerError {
		return ErrorOriginLibInternal
	}
	return ErrorOriginUserDSL
}
