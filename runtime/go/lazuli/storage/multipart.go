package storage

import (
	"context"
	"errors"
	"time"
)

const (
	// DefaultMultipartMinPartSize is the default lower bound for every
	// non-final multipart upload part.
	DefaultMultipartMinPartSize int64 = 5 * 1024 * 1024

	// DefaultMultipartMaxPartCount is the default upper bound for the number
	// of parts in one multipart upload session.
	DefaultMultipartMaxPartCount = 10_000
)

var (
	// ErrMultipartPartSizeInvalid is returned when a part has an invalid size
	// for the multipart contract. Oversized uploads still return
	// ErrFileSizeExceeded so transports can preserve the existing 413 mapping.
	ErrMultipartPartSizeInvalid = errors.New("lazuli/storage: multipart_part_size_invalid")

	// ErrMultipartPartCountInvalid is returned when a multipart upload has no
	// parts or exceeds the configured part-count limit.
	ErrMultipartPartCountInvalid = errors.New("lazuli/storage: multipart_part_count_invalid")

	// ErrMultipartPartOrderInvalid is returned when completion parts are not
	// listed in strictly ascending part-number order.
	ErrMultipartPartOrderInvalid = errors.New("lazuli/storage: multipart_part_order_invalid")
)

// MultipartLimits describes provider-neutral validation limits for a
// multipart upload. Zero values disable the corresponding bound.
type MultipartLimits struct {
	// MinPartSize is enforced for every non-final part when greater than zero.
	MinPartSize int64

	// MaxPartSize is enforced for every part when greater than zero.
	MaxPartSize int64

	// MaxPartCount is enforced for the completed upload when greater than zero.
	MaxPartCount int
}

// DefaultMultipartLimits returns the runtime's default multipart limits.
func DefaultMultipartLimits() MultipartLimits {
	return MultipartLimits{
		MinPartSize:  DefaultMultipartMinPartSize,
		MaxPartCount: DefaultMultipartMaxPartCount,
	}
}

// MultipartSession is the provider-neutral state for an in-progress
// multipart upload. ID is opaque adapter state; generated code and callers
// should persist it only long enough to sign parts, complete, or abort.
type MultipartSession struct {
	// ID is the adapter-owned opaque session identifier.
	ID string

	// Key is the final storage-side object identifier.
	Key Key

	// ContentType is the MIME type that will be stored with the completed file.
	ContentType string

	// Size is the expected final object size in bytes when known.
	Size int64

	// PartSize is the target size for non-final parts when known.
	PartSize int64

	// PartCount is the expected number of parts when known.
	PartCount int

	// ExpiresAt is the session expiry when the adapter exposes one.
	ExpiresAt time.Time
}

// MultipartPart identifies one completed multipart upload part. Token is
// adapter-owned opaque completion metadata, such as a part checksum or entity
// tag, and is passed back unchanged to CompleteMultipart.
type MultipartPart struct {
	Number int
	Size   int64
	Token  string
}

// CreateMultipartInput is the provider-neutral request to start a multipart
// upload session.
type CreateMultipartInput struct {
	Key         Key
	ContentType string
	Size        int64
	PartSize    int64
	PartCount   int
}

// SignPartInput asks an adapter to produce upload instructions for one part
// in an existing multipart session.
type SignPartInput struct {
	Session    MultipartSession
	PartNumber int
	Size       int64
	TTL        time.Duration
}

// MultipartPartUpload is the provider-neutral result of signing a single
// multipart part upload.
type MultipartPartUpload struct {
	Method    string
	URL       string
	Headers   map[string]string
	ExpiresAt time.Time
}

// CompleteMultipartInput asks an adapter to commit a multipart session using
// the ordered set of completed parts.
type CompleteMultipartInput struct {
	Session MultipartSession
	Parts   []MultipartPart
}

// AbortMultipartInput asks an adapter to discard an in-progress multipart
// session and any already uploaded parts.
type AbortMultipartInput struct {
	Session MultipartSession
}

// MultipartStore is the optional ObjectStore extension for providers that can
// manage client-driven multipart uploads. It exposes only Lazuli storage
// structs and stdlib types; adapters translate these values to their provider
// SDK calls internally.
type MultipartStore interface {
	CreateMultipart(ctx context.Context, input CreateMultipartInput) (MultipartSession, error)
	SignPart(ctx context.Context, input SignPartInput) (MultipartPartUpload, error)
	CompleteMultipart(ctx context.Context, input CompleteMultipartInput) (FileRef, error)
	AbortMultipart(ctx context.Context, input AbortMultipartInput) error
}

// ValidateMultipartContentType checks a declared part/session content type
// against FileContract.Accept. Empty content types follow Upload's behavior:
// they are accepted here and may be handled by the caller's binding layer.
func ValidateMultipartContentType(contract FileContract, contentType string) error {
	if contentType == "" || len(contract.Accept) == 0 {
		return nil
	}
	got := parseMime(contentType)
	for _, accept := range contract.Accept {
		if accept.Matches(got) {
			return nil
		}
	}
	return ErrFileMimeRejected
}

// ValidateMultipartPartCount checks that a completed multipart upload has a
// valid number of parts under limits.
func ValidateMultipartPartCount(partCount int, limits MultipartLimits) error {
	if partCount <= 0 {
		return ErrMultipartPartCountInvalid
	}
	if limits.MaxPartCount > 0 && partCount > limits.MaxPartCount {
		return ErrMultipartPartCountInvalid
	}
	return nil
}

// ValidateMultipartPartOrder checks that parts are listed in strictly
// ascending order by part number. Part numbers do not have to be contiguous.
func ValidateMultipartPartOrder(parts []MultipartPart) error {
	previous := 0
	for _, part := range parts {
		if part.Number <= previous {
			return ErrMultipartPartOrderInvalid
		}
		previous = part.Number
	}
	return nil
}

// ValidateMultipartPartSize checks one part's declared size. The min-part
// bound is skipped for the final part, matching common multipart protocols.
func ValidateMultipartPartSize(contract FileContract, part MultipartPart, final bool, limits MultipartLimits) error {
	if part.Size <= 0 {
		return ErrMultipartPartSizeInvalid
	}
	if contract.MaxSize > 0 && part.Size > contract.MaxSize {
		return ErrFileSizeExceeded
	}
	if limits.MaxPartSize > 0 && part.Size > limits.MaxPartSize {
		return ErrMultipartPartSizeInvalid
	}
	if !final && limits.MinPartSize > 0 && part.Size < limits.MinPartSize {
		return ErrMultipartPartSizeInvalid
	}
	return nil
}

// ValidateMultipartParts checks part count, ordering, individual sizes, and
// total size against the FileContract and multipart limits.
func ValidateMultipartParts(contract FileContract, parts []MultipartPart, limits MultipartLimits) error {
	if err := ValidateMultipartPartCount(len(parts), limits); err != nil {
		return err
	}
	if err := ValidateMultipartPartOrder(parts); err != nil {
		return err
	}

	var total int64
	for i, part := range parts {
		final := i == len(parts)-1
		if err := ValidateMultipartPartSize(contract, part, final, limits); err != nil {
			return err
		}
		if contract.MaxSize > 0 && part.Size > contract.MaxSize-total {
			return ErrFileSizeExceeded
		}
		total += part.Size
	}
	return nil
}
