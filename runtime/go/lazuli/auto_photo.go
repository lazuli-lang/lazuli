package lazuli

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

// AutoPhotoSpec is the per-`@cap.File`-site configuration that the
// generated init() blocks pass to RegisterAutoPhotoCommands. The
// codegen materializes one of these per resource-field site (FR-3b.2).
type AutoPhotoSpec struct {
	// Feature name (snake_case) -- used for command-name qualification.
	Feature string
	// Resource name (PascalCase).
	Resource string
	// Field name (snake_case) on the resource.
	Field string
	// SQL table name where the resource lives. Conventionally
	// lower-snake of Resource (e.g., `host`, `traveler`).
	Table string
	// `lazuli.ObjectStore(<binding>)` lookup key. Conventionally
	// `"object_store"` for hostpoint pilots.
	StoreBinding string
	// Logical bucket label per `registry.capabilities
	// object_storage <slot>`. Conventionally `"media"`.
	BucketSlot string
	// FileContract from the codegen-emitted `<Resource><PascalField>File`.
	Contract storage.FileContract
	// PolicyAtoms recorded for parity with command-level policy on
	// the synthesized commands. Currently unused by the helper but
	// retained for future runtime-side policy enforcement.
	PolicyAtoms []PolicyAtom
	// Custom TTLs. Zero values fall through to the defaults below.
	PutTTL time.Duration
	GetTTL time.Duration
}

// Runtime helper args/returns; generated per-feature structs convert
// through thin wrappers in auto_photo.gen.go init() blocks.
type AutoPhotoRequestArgs struct {
	ContentType string
	SizeBytes   int64
}

type AutoPhotoUploadIntent struct {
	URL                string
	Method             string
	HeadersContentType string
	Key                string
	ExpiresAt          time.Time
}

type AutoPhotoConfirmArgs struct {
	Key string
}

type AutoPhotoDisplayURL struct {
	URL       string
	ExpiresAt time.Time
}

// Defaults applied when the spec leaves TTLs unset.
const (
	defaultAutoPhotoPutTTL = 15 * time.Minute
	defaultAutoPhotoGetTTL = 24 * time.Hour
)

// AutoPhotoRequest mints a presigned PUT URL for the active user's
// stored file under the spec'd field. The deterministic key shape is
// `<table>/<field>/org-<org_id>/user-<user_id>` -- see
// docs/proposals/fileref-jsonb-fr3-design.md §D2.
func AutoPhotoRequest(
	ctx *Ctx,
	spec AutoPhotoSpec,
	args AutoPhotoRequestArgs,
) (AutoPhotoUploadIntent, error) {
	if ctx == nil || ctx.User == nil {
		return AutoPhotoUploadIntent{}, errors.New("auto_photo: not_authenticated")
	}
	orgID, err := autoPhotoOrgID(ctx)
	if err != nil {
		return AutoPhotoUploadIntent{}, err
	}

	// Size guard -- short-circuit before we touch the object store.
	if args.SizeBytes > spec.Contract.MaxSize {
		return AutoPhotoUploadIntent{}, errors.New("auto_photo: file_size_exceeded")
	}
	// SECURITY (SEC-H7): the presigned PUT URL alone does not enforce
	// max_size at the bucket. Clients can lie about Content-Length or
	// upload via multipart. Defense-in-depth options:
	//   1. Sign the URL with explicit Content-Length-Range constraints
	//      (S3 supports POST policy with content-length-range; PUT does not).
	//   2. Post-PUT HEAD-probe in the Confirm path and reject + delete
	//      objects exceeding contract.MaxSize.
	//   3. Bucket lifecycle policy with object-size limit.
	// Provider has no HeadObject/StatObject hook today, so v0 keeps the
	// pre-PUT declaration check and logs the residual follow-up.
	slog.Info("auto_photo: SEC-H7: relying on pre-PUT size declaration; HEAD-probe is followup work",
		"store_binding", spec.StoreBinding,
		"bucket_slot", spec.BucketSlot,
		"max_size", spec.Contract.MaxSize,
	)
	// MIME guard against the contract's Accept list (any wildcard
	// entry passes -- see storage.MimeType.Matches).
	if !contractAcceptsMime(spec.Contract, args.ContentType) {
		return AutoPhotoUploadIntent{}, errors.New("auto_photo: file_mime_rejected")
	}

	key := autoPhotoKey(spec, orgID, ctx.User.ID)
	ttl := spec.PutTTL
	if ttl == 0 {
		ttl = defaultAutoPhotoPutTTL
	}

	store, err := ObjectStore(spec.StoreBinding)
	if err != nil {
		return AutoPhotoUploadIntent{}, fmt.Errorf("auto_photo: %w", err)
	}
	url, err := store.PresignedURL(autoPhotoContext(ctx), spec.BucketSlot, key, ttl, "PUT")
	if err != nil {
		return AutoPhotoUploadIntent{}, fmt.Errorf("auto_photo: %w", err)
	}

	return AutoPhotoUploadIntent{
		URL:                url,
		Method:             "PUT",
		HeadersContentType: args.ContentType,
		Key:                key,
		ExpiresAt:          time.Now().Add(ttl),
	}, nil
}

// AutoPhotoConfirm persists the FileRef on the resource row. The
// caller MUST pass the same key the request handler returned -- we
// cross-check the deterministic shape to refuse cross-user pivots.
func AutoPhotoConfirm(ctx *Ctx, spec AutoPhotoSpec, args AutoPhotoConfirmArgs) error {
	if ctx == nil || ctx.User == nil {
		return errors.New("auto_photo: not_authenticated")
	}
	orgID, err := autoPhotoOrgID(ctx)
	if err != nil {
		return err
	}
	expected := autoPhotoKey(spec, orgID, ctx.User.ID)
	if args.Key != expected {
		return errors.New("auto_photo: file_key_mismatch")
	}

	// We don't have ContentType / Size at confirm time without an
	// extra round-trip to the bucket. v0 writes the key alone and
	// leaves ContentType/Size empty in the FileRef -- adapters that
	// need them HEAD the object on demand. Future cycle can wire a
	// HEAD probe here.
	ref := storage.FileRef{Key: storage.Key(args.Key)}
	payload, err := json.Marshal(struct {
		Key         storage.Key `json:"key"`
		ContentType string      `json:"content_type"`
		Size        int64       `json:"size"`
	}{Key: ref.Key, ContentType: ref.ContentType, Size: ref.Size})
	if err != nil {
		return fmt.Errorf("auto_photo: %w", err)
	}

	res, err := DB().Exec(autoPhotoContext(ctx),
		fmt.Sprintf(`UPDATE %s SET %s = $1 WHERE "user" = $2 AND org_id = $3`,
			quoteIdent(spec.Table), quoteIdent(spec.Field)),
		payload, ctx.User.ID, orgID,
	)
	if err != nil {
		return fmt.Errorf("auto_photo: %w", err)
	}
	if res.RowsAffected() == 0 {
		return errors.New("auto_photo: row_missing")
	}
	return nil
}

// AutoPhotoClear nullifies the resource row's field and best-effort
// deletes the bytes from the object store. Idempotent -- clearing an
// already-empty row succeeds silently.
func AutoPhotoClear(ctx *Ctx, spec AutoPhotoSpec) error {
	if ctx == nil || ctx.User == nil {
		return errors.New("auto_photo: not_authenticated")
	}
	orgID, err := autoPhotoOrgID(ctx)
	if err != nil {
		return err
	}
	_, err = DB().Exec(autoPhotoContext(ctx),
		fmt.Sprintf(`UPDATE %s SET %s = NULL WHERE "user" = $1 AND org_id = $2`,
			quoteIdent(spec.Table), quoteIdent(spec.Field)),
		ctx.User.ID, orgID,
	)
	if err != nil {
		return fmt.Errorf("auto_photo: %w", err)
	}

	// Best-effort object delete. We swallow missing-binding and
	// missing-object errors so the column clear stays idempotent in
	// dev environments without the plugin wired.
	store, err := ObjectStore(spec.StoreBinding)
	if err != nil {
		return nil
	}
	_ = store.DeleteObject(autoPhotoContext(ctx), spec.BucketSlot, autoPhotoKey(spec, orgID, ctx.User.ID))
	return nil
}

// AutoPhotoGetURL mints a signed GET URL for the active user's
// stored file. Empty URL when no file is set.
func AutoPhotoGetURL(ctx *Ctx, spec AutoPhotoSpec) (AutoPhotoDisplayURL, error) {
	if ctx == nil || ctx.User == nil {
		return AutoPhotoDisplayURL{}, errors.New("auto_photo: not_authenticated")
	}
	orgID, err := autoPhotoOrgID(ctx)
	if err != nil {
		return AutoPhotoDisplayURL{}, err
	}

	// Read the JSONB column. Scan via storage.FileRef.Scan (FR-1)
	// handles the JSONB -> struct deserialisation.
	var ref storage.FileRef
	err = DB().QueryRow(autoPhotoContext(ctx),
		fmt.Sprintf(`SELECT %s FROM %s WHERE "user" = $1 AND org_id = $2`,
			quoteIdent(spec.Field), quoteIdent(spec.Table)),
		ctx.User.ID, orgID,
	).Scan(&ref)
	if err != nil {
		// Row missing (e.g., onboarding pendente) -- empty result.
		return AutoPhotoDisplayURL{}, nil
	}
	if ref.Key == "" {
		return AutoPhotoDisplayURL{}, nil
	}

	ttl := spec.GetTTL
	if ttl == 0 {
		ttl = defaultAutoPhotoGetTTL
	}

	store, err := ObjectStore(spec.StoreBinding)
	if err != nil {
		// Dev environments without the plugin wired -- degrade gracefully.
		return AutoPhotoDisplayURL{}, nil
	}
	url, err := store.PresignedURL(autoPhotoContext(ctx), spec.BucketSlot, string(ref.Key), ttl, "GET")
	if err != nil {
		return AutoPhotoDisplayURL{}, fmt.Errorf("auto_photo: %w", err)
	}

	return AutoPhotoDisplayURL{
		URL:       url,
		ExpiresAt: time.Now().Add(ttl),
	}, nil
}

// autoPhotoKey returns the deterministic storage key for the active
// user under the spec'd resource field.
func autoPhotoKey(spec AutoPhotoSpec, orgID, userID ID) string {
	return fmt.Sprintf("%s/%s/org-%d/user-%d", spec.Table, spec.Field, orgID, userID)
}

func autoPhotoOrgID(ctx *Ctx) (ID, error) {
	if ctx == nil || ctx.Tenant == nil {
		return 0, tenantRequiredError()
	}
	return ctx.Tenant.OrgID, nil
}

// contractAcceptsMime reports whether the request's content_type
// intersects with the contract's Accept list. Empty Accept means
// "no validator" (codegen would have flagged that earlier).
func contractAcceptsMime(contract storage.FileContract, contentType string) bool {
	if len(contract.Accept) == 0 {
		return true
	}
	inFamily, inSubtype, ok := splitMime(contentType)
	if !ok {
		return false
	}
	in := storage.MimeType{Family: inFamily, Subtype: inSubtype}
	for _, m := range contract.Accept {
		if m.Matches(in) {
			return true
		}
	}
	return false
}

func splitMime(s string) (family, subtype string, ok bool) {
	for i := 0; i < len(s); i++ {
		if s[i] == '/' {
			return s[:i], s[i+1:], true
		}
	}
	return "", "", false
}

func autoPhotoContext(ctx *Ctx) context.Context {
	if ctx != nil && ctx.Context != nil {
		return ctx.Context
	}
	return context.Background()
}
