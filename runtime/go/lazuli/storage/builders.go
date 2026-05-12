package storage

import "time"

// Public returns a public-visibility FileContract shell.
func Public(resource, field string, maxSize int64, accept ...MimeType) FileContract {
	return FileContract{
		Resource:   resource,
		Field:      field,
		MaxSize:    maxSize,
		Accept:     accept,
		Visibility: VisibilityPublic,
	}
}

// Private returns a private FileContract.
func Private(resource, field string, maxSize int64, accept ...MimeType) FileContract {
	return FileContract{
		Resource:   resource,
		Field:      field,
		MaxSize:    maxSize,
		Accept:     accept,
		Visibility: VisibilityPrivate,
	}
}

// Signed returns a signed FileContract with TTL.
func Signed(resource, field string, maxSize int64, ttl time.Duration, accept ...MimeType) FileContract {
	return FileContract{
		Resource:   resource,
		Field:      field,
		MaxSize:    maxSize,
		Accept:     accept,
		Visibility: VisibilitySigned,
		SignedTTL:  ttl,
	}
}

// ImageMime returns an image MIME type with the provided subtype.
func ImageMime(subtype string) MimeType {
	return MimeType{Family: "image", Subtype: subtype}
}

// ImageAny returns the image/* MIME wildcard.
func ImageAny() MimeType {
	return MimeType{Family: "image", Subtype: "*"}
}

// TextMime returns a text MIME type with the provided subtype.
func TextMime(subtype string) MimeType {
	return MimeType{Family: "text", Subtype: subtype}
}

// App returns an application MIME type with the provided subtype.
func App(subtype string) MimeType {
	return MimeType{Family: "application", Subtype: subtype}
}
