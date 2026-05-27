package lazuli

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
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
	// `"object_store"` for the canonical pilot pilots.
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
		return AutoPhotoUploadIntent{}, fileSizeExceededError(spec.Contract.MaxSize, args.SizeBytes)
	}
	// SECURITY (SEC-H7): the pre-PUT check above is a fast-fail on
	// honest clients. The Confirm path runs an authoritative HEAD
	// probe and deletes+rejects anything that exceeded MaxSize or
	// fell outside the Accept list at PUT time, so a lying client
	// cannot persist an oversized/wrong-mime FileRef.
	// MIME guard against the contract's Accept list (any wildcard
	// entry passes -- see storage.MimeType.Matches).
	if !contractAcceptsMime(spec.Contract, args.ContentType) {
		return AutoPhotoUploadIntent{}, fileMimeRejectedError(spec.Contract.Accept, args.ContentType)
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
//
// SECURITY (SEC-H7 closure): the request-time `size_bytes` /
// `content_type` are client-declared and not bound to the actual PUT
// (the presigned PUT URL is keyed on bucket+key+TTL only — S3 has no
// way to bind a Content-Length to a PUT signature). Confirm closes
// the loop with a single HEAD probe: anything that exceeds
// `Contract.MaxSize` or sits outside `Contract.Accept` gets deleted
// and rejected with `file_size_exceeded` / `file_mime_rejected`. The
// honest size + mime are then backfilled onto the persisted FileRef
// so downstream views render correct metadata.
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
		return fileKeyMismatchError()
	}

	store, err := ObjectStore(spec.StoreBinding)
	if err != nil {
		return fmt.Errorf("auto_photo: %w", err)
	}

	meta, headErr := store.HeadObject(autoPhotoContext(ctx), spec.BucketSlot, args.Key)
	if headErr != nil {
		// NoSuchKey here means the client never finished the PUT
		// (or the URL expired before they did). Surface it as a
		// distinct 409 so the UI can prompt retry instead of a
		// generic 500.
		if errors.Is(headErr, storage.ErrFileNotFound) {
			return fileNotUploadedError()
		}
		return fmt.Errorf("auto_photo: head_probe: %w", headErr)
	}

	if spec.Contract.MaxSize > 0 && meta.Size > spec.Contract.MaxSize {
		// Stream rejected: client lied about size_bytes at request
		// time and pushed past the contract. Best-effort delete so
		// the orphan doesn't squat in the bucket.
		_ = store.DeleteObject(autoPhotoContext(ctx), spec.BucketSlot, args.Key)
		return fileSizeExceededError(spec.Contract.MaxSize, meta.Size)
	}
	if !contractAcceptsMime(spec.Contract, meta.ContentType) {
		_ = store.DeleteObject(autoPhotoContext(ctx), spec.BucketSlot, args.Key)
		return fileMimeRejectedError(spec.Contract.Accept, meta.ContentType)
	}

	ref := storage.FileRef{
		Key:         storage.Key(args.Key),
		ContentType: meta.ContentType,
		Size:        meta.Size,
	}
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

// Typed-error constructors. Each one carries the structured `Data`
// payload the UI can use to render specific copy (e.g. "Arquivo maior
// que 2 MB" instead of "Tamanho excedido"). Apps wire the matching
// `errors` block in `.lzi` to localize.
func fileSizeExceededError(maxBytes, gotBytes int64) *Error {
	return &Error{
		Status:     400,
		Code:       CodeFileSizeExceeded,
		MessageKey: CodeFileSizeExceeded,
		Message:    fmt.Sprintf("file exceeds max size of %d bytes (got %d)", maxBytes, gotBytes),
		Data: map[string]any{
			"max_bytes": maxBytes,
			"got_bytes": gotBytes,
		},
	}
}

func fileMimeRejectedError(accept []storage.MimeType, got string) *Error {
	acceptList := make([]string, 0, len(accept))
	for _, m := range accept {
		acceptList = append(acceptList, m.Family+"/"+m.Subtype)
	}
	return &Error{
		Status:     400,
		Code:       CodeFileMimeRejected,
		MessageKey: CodeFileMimeRejected,
		Message:    fmt.Sprintf("content type %q not in accept list %v", got, acceptList),
		Data: map[string]any{
			"got":    got,
			"accept": acceptList,
		},
	}
}

func fileNotUploadedError() *Error {
	return &Error{
		Status:     409,
		Code:       CodeFileNotUploaded,
		MessageKey: CodeFileNotUploaded,
		Message:    "file not found in object store; the PUT did not complete",
	}
}

func fileKeyMismatchError() *Error {
	return &Error{
		Status:     400,
		Code:       CodeFileKeyMismatch,
		MessageKey: CodeFileKeyMismatch,
		Message:    "key does not match the one issued by the request handler",
	}
}
