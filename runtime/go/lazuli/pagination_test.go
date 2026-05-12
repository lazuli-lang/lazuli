package lazuli

import (
	"encoding/json"
	"errors"
	"net/http"
	"testing"
)

func TestNormalizePaginationDefaultsAndClampsLimit(t *testing.T) {
	page, err := NormalizePagination(PaginationInput{}, PaginationOptions{DefaultLimit: 25, MaxLimit: 50})
	if err != nil {
		t.Fatalf("NormalizePagination returned error: %v", err)
	}
	if page != (Page{Limit: 25, Offset: 0}) {
		t.Fatalf("page = %#v, want default limit and zero offset", page)
	}

	page, err = NormalizePagination(PaginationInput{Limit: 100, Offset: 10}, PaginationOptions{DefaultLimit: 25, MaxLimit: 50})
	if err != nil {
		t.Fatalf("NormalizePagination returned error: %v", err)
	}
	if page != (Page{Limit: 50, Offset: 10}) {
		t.Fatalf("page = %#v, want clamped limit", page)
	}
}

func TestNormalizeLimitOffset(t *testing.T) {
	page, err := NormalizeLimitOffset(0, 15, PaginationOptions{DefaultLimit: 30, MaxLimit: 100})
	if err != nil {
		t.Fatalf("NormalizeLimitOffset returned error: %v", err)
	}
	if page != (Page{Limit: 30, Offset: 15}) {
		t.Fatalf("page = %#v, want normalized limit and offset", page)
	}
}

func TestNormalizePaginationRejectsNegativeValues(t *testing.T) {
	tests := []PaginationInput{
		{Limit: -1},
		{Limit: 10, Offset: -1},
	}

	for _, input := range tests {
		if _, err := NormalizePagination(input, PaginationOptions{}); !isBadRequest(err) {
			t.Fatalf("NormalizePagination(%#v) error = %v, want bad request", input, err)
		}
	}
}

func TestNormalizePaginationUsesCursorOffset(t *testing.T) {
	token, err := EncodePageCursor(PageCursor{Offset: 80})
	if err != nil {
		t.Fatalf("EncodePageCursor returned error: %v", err)
	}

	page, err := NormalizePagination(PaginationInput{Limit: 20, Offset: 40, Cursor: token}, PaginationOptions{})
	if err != nil {
		t.Fatalf("NormalizePagination returned error: %v", err)
	}
	if page != (Page{Limit: 20, Offset: 80}) {
		t.Fatalf("page = %#v, want cursor offset", page)
	}
}

func TestPageCursorRoundTripAndValidation(t *testing.T) {
	token, err := EncodePageCursor(PageCursor{Offset: 120})
	if err != nil {
		t.Fatalf("EncodePageCursor returned error: %v", err)
	}

	cursor, err := DecodePageCursor(token)
	if err != nil {
		t.Fatalf("DecodePageCursor returned error: %v", err)
	}
	if cursor.Offset != 120 {
		t.Fatalf("cursor offset = %d, want 120", cursor.Offset)
	}

	if _, err := DecodePageCursor("not-base64"); !isBadRequest(err) {
		t.Fatalf("DecodePageCursor invalid error = %v, want bad request", err)
	}
	if _, err := EncodePageCursor(PageCursor{Offset: -1}); !isBadRequest(err) {
		t.Fatalf("EncodePageCursor negative error = %v, want bad request", err)
	}
}

func TestNewPaginatedResponseBuildsMetadataAndCursors(t *testing.T) {
	total := 9
	response, err := NewPaginatedResponse(
		[]string{"c", "d"},
		Page{Limit: 2, Offset: 4},
		PaginationResponseOptions{Total: &total},
	)
	if err != nil {
		t.Fatalf("NewPaginatedResponse returned error: %v", err)
	}

	if response.Meta.Count != 2 || response.Meta.Total == nil || *response.Meta.Total != 9 {
		t.Fatalf("meta count/total = %#v, want count 2 total 9", response.Meta)
	}
	if !response.Meta.HasNext || !response.Meta.HasPrev {
		t.Fatalf("meta next/prev = %#v, want both true", response.Meta)
	}

	next, err := DecodePageCursor(response.Meta.NextCursor)
	if err != nil {
		t.Fatalf("DecodePageCursor(next) returned error: %v", err)
	}
	if next.Offset != 6 {
		t.Fatalf("next cursor offset = %d, want 6", next.Offset)
	}
	prev, err := DecodePageCursor(response.Meta.PrevCursor)
	if err != nil {
		t.Fatalf("DecodePageCursor(prev) returned error: %v", err)
	}
	if prev.Offset != 2 {
		t.Fatalf("prev cursor offset = %d, want 2", prev.Offset)
	}
}

func TestPaginatedResponseMarshalShape(t *testing.T) {
	response, err := NewPaginatedResponse([]string{"a"}, Page{Limit: 10, Offset: 0}, PaginationResponseOptions{})
	if err != nil {
		t.Fatalf("NewPaginatedResponse returned error: %v", err)
	}

	data, err := json.Marshal(response)
	if err != nil {
		t.Fatalf("Marshal returned error: %v", err)
	}

	var body map[string]any
	if err := json.Unmarshal(data, &body); err != nil {
		t.Fatalf("Unmarshal returned error: %v; JSON = %s", err, data)
	}
	if _, ok := body["data"].([]any); !ok {
		t.Fatalf("data = %#v, want array", body["data"])
	}
	meta := body["meta"].(map[string]any)
	if meta["limit"] != float64(10) || meta["offset"] != float64(0) || meta["count"] != float64(1) {
		t.Fatalf("meta = %#v, want limit/offset/count", meta)
	}
	if _, ok := meta["total"]; ok {
		t.Fatalf("meta includes total without total option: %#v", meta)
	}
	if _, ok := meta["next_cursor"]; ok {
		t.Fatalf("meta includes next_cursor without next page: %#v", meta)
	}
}

func isBadRequest(err error) bool {
	var le *Error
	return errors.As(err, &le) && le.Status == http.StatusBadRequest && le.Code == CodeBadRequest
}
