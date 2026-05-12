package lazuli

import (
	"encoding/base64"
	"encoding/json"
	"net/http"
	"strings"
)

const (
	// DefaultPaginationLimit is used when a list request omits limit.
	DefaultPaginationLimit = 100

	// DefaultPaginationMaxLimit caps list request limits unless overridden.
	DefaultPaginationMaxLimit = 1000
)

// PaginationInput is the provider-neutral input accepted by generated list
// endpoints. Cursor, when set, wins over Offset.
type PaginationInput struct {
	Limit  int    `json:"limit,omitempty"`
	Offset int    `json:"offset,omitempty"`
	Cursor string `json:"cursor,omitempty"`
}

// PaginationOptions controls limit normalization for generated list endpoints.
type PaginationOptions struct {
	DefaultLimit int
	MaxLimit     int
}

// Page describes a normalized list request.
type Page struct {
	Limit  int `json:"limit"`
	Offset int `json:"offset"`
}

// PageCursor is the offset cursor payload encoded into opaque cursor tokens.
type PageCursor struct {
	Offset int `json:"offset"`
}

// PaginationMeta is the metadata emitted with paginated list responses.
type PaginationMeta struct {
	Limit      int    `json:"limit"`
	Offset     int    `json:"offset"`
	Count      int    `json:"count"`
	Total      *int   `json:"total,omitempty"`
	HasNext    bool   `json:"has_next"`
	HasPrev    bool   `json:"has_prev"`
	NextCursor string `json:"next_cursor,omitempty"`
	PrevCursor string `json:"prev_cursor,omitempty"`
}

// PaginatedResponse wraps list data with pagination metadata.
type PaginatedResponse[T any] struct {
	Data []T            `json:"data"`
	Meta PaginationMeta `json:"meta"`
}

// PaginationResponseOptions controls optional response metadata.
type PaginationResponseOptions struct {
	Total   *int
	HasNext bool
}

// NormalizePagination returns a validated and clamped page request. A zero
// limit uses the configured default; limits above MaxLimit are capped.
func NormalizePagination(input PaginationInput, options PaginationOptions) (Page, error) {
	options = normalizePaginationOptions(options)

	limit := input.Limit
	switch {
	case limit < 0:
		return Page{}, paginationBadRequest("pagination limit must not be negative")
	case limit == 0:
		limit = options.DefaultLimit
	case limit > options.MaxLimit:
		limit = options.MaxLimit
	}

	offset := input.Offset
	if offset < 0 {
		return Page{}, paginationBadRequest("pagination offset must not be negative")
	}

	if strings.TrimSpace(input.Cursor) != "" {
		cursor, err := DecodePageCursor(input.Cursor)
		if err != nil {
			return Page{}, err
		}
		offset = cursor.Offset
	}

	return Page{Limit: limit, Offset: offset}, nil
}

// NormalizeLimitOffset returns a normalized page request for offset-only list
// endpoints.
func NormalizeLimitOffset(limit, offset int, options PaginationOptions) (Page, error) {
	return NormalizePagination(PaginationInput{Limit: limit, Offset: offset}, options)
}

// EncodePageCursor encodes cursor as an opaque URL-safe token.
func EncodePageCursor(cursor PageCursor) (string, error) {
	if cursor.Offset < 0 {
		return "", paginationBadRequest("pagination cursor offset must not be negative")
	}
	payload, err := json.Marshal(cursor)
	if err != nil {
		return "", &Error{
			Status:  http.StatusInternalServerError,
			Code:    CodeInternal,
			Message: "failed to encode pagination cursor",
		}
	}
	return base64.RawURLEncoding.EncodeToString(payload), nil
}

// DecodePageCursor decodes an opaque cursor token.
func DecodePageCursor(token string) (PageCursor, error) {
	token = strings.TrimSpace(token)
	if token == "" {
		return PageCursor{}, nil
	}
	payload, err := base64.RawURLEncoding.DecodeString(token)
	if err != nil {
		return PageCursor{}, paginationBadRequest("invalid pagination cursor")
	}
	var cursor PageCursor
	if err := json.Unmarshal(payload, &cursor); err != nil {
		return PageCursor{}, paginationBadRequest("invalid pagination cursor")
	}
	if cursor.Offset < 0 {
		return PageCursor{}, paginationBadRequest("pagination cursor offset must not be negative")
	}
	return cursor, nil
}

// NewPaginatedResponse wraps items and builds offset/cursor metadata for the
// normalized page. HasNext may be supplied explicitly or derived from Total.
func NewPaginatedResponse[T any](items []T, page Page, options PaginationResponseOptions) (PaginatedResponse[T], error) {
	if page.Limit <= 0 {
		return PaginatedResponse[T]{}, paginationBadRequest("pagination limit must be greater than zero")
	}
	if page.Offset < 0 {
		return PaginatedResponse[T]{}, paginationBadRequest("pagination offset must not be negative")
	}

	count := len(items)
	hasNext := options.HasNext
	var total *int
	if options.Total != nil {
		value := *options.Total
		if value < 0 {
			return PaginatedResponse[T]{}, paginationBadRequest("pagination total must not be negative")
		}
		total = &value
		hasNext = hasNext || page.Offset+count < value
	}

	meta := PaginationMeta{
		Limit:   page.Limit,
		Offset:  page.Offset,
		Count:   count,
		Total:   total,
		HasNext: hasNext,
		HasPrev: page.Offset > 0,
	}
	if meta.HasNext {
		nextCursor, err := EncodePageCursor(PageCursor{Offset: page.Offset + page.Limit})
		if err != nil {
			return PaginatedResponse[T]{}, err
		}
		meta.NextCursor = nextCursor
	}
	if meta.HasPrev {
		prevOffset := page.Offset - page.Limit
		if prevOffset < 0 {
			prevOffset = 0
		}
		prevCursor, err := EncodePageCursor(PageCursor{Offset: prevOffset})
		if err != nil {
			return PaginatedResponse[T]{}, err
		}
		meta.PrevCursor = prevCursor
	}

	return PaginatedResponse[T]{Data: items, Meta: meta}, nil
}

func normalizePaginationOptions(options PaginationOptions) PaginationOptions {
	if options.DefaultLimit <= 0 {
		options.DefaultLimit = DefaultPaginationLimit
	}
	if options.MaxLimit <= 0 {
		options.MaxLimit = DefaultPaginationMaxLimit
	}
	if options.DefaultLimit > options.MaxLimit {
		options.DefaultLimit = options.MaxLimit
	}
	return options
}

func paginationBadRequest(message string) error {
	return &Error{Status: http.StatusBadRequest, Code: CodeBadRequest, Message: message}
}
