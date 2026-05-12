package storage

import (
	"context"
	"time"
)

// DirectUploadRequest carries the client-declared metadata used to
// mint a direct-upload ticket. It aliases Metadata so proxied uploads
// and direct-upload tickets validate the same filename, MIME type, and
// declared size shape.
type DirectUploadRequest = Metadata

// DirectUploadTicket is the presigned upload instruction returned to a
// client that will PUT bytes directly into the bound object store.
type DirectUploadTicket struct {
	// Key is the opaque storage-side identifier the caller should
	// persist after the client completes the direct upload.
	Key Key

	// UploadURL is the presigned PUT URL for the object store.
	UploadURL string

	// Headers lists request headers the client must send with the PUT.
	Headers map[string]string
}

// IssueDirectUpload validates the requested file metadata against
// contract and asks signer for a presigned PUT URL valid for ttl.
//
// signer may be a bound ObjectStore or a concrete adapter value; stores
// that support direct upload opt in by also implementing
// PresignedURLWriter. ObjectStore itself remains limited to the core
// Put/Get/Sign/Delete surface.
func IssueDirectUpload(
	ctx context.Context,
	contract FileContract,
	signer any,
	metadata DirectUploadRequest,
	ttl time.Duration,
) (DirectUploadTicket, error) {
	var zero DirectUploadTicket
	if err := validateUploadMetadata(contract, metadata); err != nil {
		return zero, err
	}
	if err := validateDirectUploadContract(contract, ttl); err != nil {
		return zero, err
	}

	writer, ok := signer.(PresignedURLWriter)
	if !ok {
		return zero, ErrVisibilityMismatch
	}

	key := mintKey(contract, metadata)
	uploadURL, err := writer.SignPut(ctx, key, metadata.ContentType, ttl)
	if err != nil {
		return zero, err
	}

	return DirectUploadTicket{
		Key:       key,
		UploadURL: uploadURL,
		Headers:   directUploadHeaders(metadata),
	}, nil
}

func validateUploadMetadata(contract FileContract, metadata Metadata) error {
	if metadata.Size > 0 && contract.MaxSize > 0 && metadata.Size > contract.MaxSize {
		return ErrFileSizeExceeded
	}
	if metadata.ContentType != "" && len(contract.Accept) > 0 {
		got := parseMime(metadata.ContentType)
		matched := false
		for _, accept := range contract.Accept {
			if accept.Matches(got) {
				matched = true
				break
			}
		}
		if !matched {
			return ErrFileMimeRejected
		}
	}
	return nil
}

func validateDirectUploadContract(contract FileContract, ttl time.Duration) error {
	if ttl <= 0 {
		return ErrVisibilityMismatch
	}
	switch contract.Visibility {
	case VisibilityPrivate, VisibilityPublic:
		if contract.SignedTTL > 0 {
			return ErrVisibilityMismatch
		}
		return nil
	case VisibilitySigned:
		if contract.SignedTTL <= 0 {
			return ErrVisibilityMismatch
		}
		return nil
	default:
		return ErrVisibilityMismatch
	}
}

func directUploadHeaders(metadata Metadata) map[string]string {
	headers := make(map[string]string)
	if metadata.ContentType != "" {
		headers["Content-Type"] = metadata.ContentType
	}
	return headers
}
