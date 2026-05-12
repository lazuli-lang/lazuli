package storage

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"strings"
)

const (
	// ContentHashSHA256Prefix prefixes storage content hashes. The digest is
	// lowercase hex so generated dedup keys stay deterministic.
	ContentHashSHA256Prefix = "sha256:"
)

var (
	// ErrDedupMetadataInvalid is returned when content-hash or reuse metadata
	// cannot safely identify an existing object.
	ErrDedupMetadataInvalid = errors.New("lazuli/storage: dedup_metadata_invalid")
)

// ContentHashResult carries the digest and byte count observed while hashing
// a storage object body. Hash is rendered as "sha256:<lowercase hex>".
type ContentHashResult struct {
	Hash string
	Size int64
}

// ReuseMetadata is the provider-neutral contract for reusing an object that
// already exists at its deterministic dedup key. It mirrors FileRef while also
// carrying the content hash used to prove object identity.
type ReuseMetadata struct {
	Key         Key
	ContentHash string
	ContentType string
	Size        int64
}

// FileRef returns the persisted file reference represented by metadata.
func (m ReuseMetadata) FileRef() FileRef {
	return FileRef{
		Key:         m.Key,
		ContentType: m.ContentType,
		Size:        m.Size,
	}
}

// CalculateContentHash streams body into SHA-256 and returns the canonical
// storage content hash plus the number of bytes read.
func CalculateContentHash(body io.Reader) (ContentHashResult, error) {
	if body == nil {
		return ContentHashResult{}, fmt.Errorf("%w: body is required", ErrDedupMetadataInvalid)
	}

	hasher := sha256.New()
	size, err := io.Copy(hasher, body)
	if err != nil {
		return ContentHashResult{}, err
	}
	return ContentHashResult{
		Hash: ContentHashSHA256Prefix + hex.EncodeToString(hasher.Sum(nil)),
		Size: size,
	}, nil
}

// DedupKey returns the deterministic object key for contentHash under
// contract's file site. The key is filesystem-safe for LocalStore and stable
// across adapters.
func DedupKey(contract FileContract, contentHash string) (Key, error) {
	digest, err := canonicalDedupDigest(contentHash)
	if err != nil {
		return "", err
	}

	scope, err := dedupScope(contract)
	if err != nil {
		return "", err
	}

	segments := make([]string, 0, len(scope)+3)
	segments = append(segments, "dedup")
	segments = append(segments, scope...)
	segments = append(segments, "sha256", digest)
	return Key(strings.Join(segments, "/")), nil
}

// ValidateReuseMetadata checks that metadata is a coherent reference to an
// already-stored dedup object and that its declared content type and size still
// satisfy contract.
func ValidateReuseMetadata(contract FileContract, metadata ReuseMetadata) error {
	if metadata.Key == "" {
		return fmt.Errorf("%w: key is required", ErrDedupMetadataInvalid)
	}

	expectedKey, err := DedupKey(contract, metadata.ContentHash)
	if err != nil {
		return err
	}
	if metadata.Key != expectedKey {
		return fmt.Errorf("%w: key %q does not match content hash", ErrDedupMetadataInvalid, metadata.Key)
	}
	if metadata.Size < 0 {
		return fmt.Errorf("%w: size must be non-negative", ErrDedupMetadataInvalid)
	}

	return validateUploadMetadata(contract, Metadata{
		ContentType: metadata.ContentType,
		Size:        metadata.Size,
	})
}

func canonicalDedupDigest(contentHash string) (string, error) {
	contentHash = strings.TrimSpace(contentHash)
	digest, ok := strings.CutPrefix(contentHash, ContentHashSHA256Prefix)
	if !ok {
		return "", fmt.Errorf("%w: content hash must use sha256", ErrDedupMetadataInvalid)
	}
	if len(digest) != sha256.Size*2 {
		return "", fmt.Errorf("%w: sha256 digest must be 64 hex characters", ErrDedupMetadataInvalid)
	}

	for _, r := range digest {
		if !isHexRune(r) {
			return "", fmt.Errorf("%w: sha256 digest must be hex", ErrDedupMetadataInvalid)
		}
	}
	return strings.ToLower(digest), nil
}

func dedupScope(contract FileContract) ([]string, error) {
	resource := strings.TrimSpace(contract.Resource)
	field := strings.TrimSpace(contract.Field)
	api := strings.TrimSpace(contract.API)

	switch {
	case resource != "" || field != "":
		if resource == "" || field == "" {
			return nil, fmt.Errorf("%w: resource and field are both required for resource file dedup", ErrDedupMetadataInvalid)
		}
		if !validDedupSegment(resource) || !validDedupSegment(field) {
			return nil, fmt.Errorf("%w: resource or field contains unsafe key characters", ErrDedupMetadataInvalid)
		}
		return []string{"resource", resource, field}, nil
	case api != "":
		if !validDedupSegment(api) {
			return nil, fmt.Errorf("%w: api contains unsafe key characters", ErrDedupMetadataInvalid)
		}
		return []string{"api", api}, nil
	default:
		return []string{"unscoped"}, nil
	}
}

func validDedupSegment(segment string) bool {
	if segment == "" || segment == "." || segment == ".." {
		return false
	}
	for _, r := range segment {
		switch {
		case r >= 'a' && r <= 'z',
			r >= 'A' && r <= 'Z',
			r >= '0' && r <= '9',
			r == '_',
			r == '-',
			r == '.':
			continue
		default:
			return false
		}
	}
	return true
}

func isHexRune(r rune) bool {
	return r >= '0' && r <= '9' ||
		r >= 'a' && r <= 'f' ||
		r >= 'A' && r <= 'F'
}
