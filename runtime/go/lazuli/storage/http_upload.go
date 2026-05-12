package storage

import (
	"encoding/json"
	"errors"
	"net/http"
	"time"
)

// DirectUploadHTTPResponse is the JSON shape returned by the direct-upload
// ticket endpoint.
type DirectUploadHTTPResponse struct {
	Key       Key               `json:"key"`
	UploadURL string            `json:"upload_url"`
	Headers   map[string]string `json:"headers,omitempty"`
}

// DirectUploadHTTPError is the JSON shape returned when a ticket request
// cannot be fulfilled.
type DirectUploadHTTPError struct {
	Code  string `json:"code"`
	Error string `json:"error"`
}

type directUploadHTTPRequest struct {
	Filename    string `json:"filename"`
	ContentType string `json:"content_type"`
	Size        int64  `json:"size"`
}

// DirectUploadHTTPHandler returns an HTTP handler that accepts file metadata
// as JSON, mints a direct-upload ticket, and writes the ticket as JSON.
func DirectUploadHTTPHandler(contract FileContract, store ObjectStore, ttl time.Duration) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ServeDirectUploadHTTP(w, r, contract, store, ttl)
	})
}

// ServeDirectUploadHTTP handles one direct-upload ticket request. It is kept
// as a function so generated APIs can call it from an already-mounted route.
func ServeDirectUploadHTTP(
	w http.ResponseWriter,
	r *http.Request,
	contract FileContract,
	store ObjectStore,
	ttl time.Duration,
) {
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", http.MethodPost)
		writeDirectUploadHTTPError(w, http.StatusMethodNotAllowed, "method_not_allowed", "direct upload tickets require POST")
		return
	}

	var request directUploadHTTPRequest
	decoder := json.NewDecoder(r.Body)
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&request); err != nil {
		writeDirectUploadHTTPError(w, http.StatusBadRequest, "bad_request", "invalid JSON metadata: "+err.Error())
		return
	}

	ticket, err := IssueDirectUpload(r.Context(), contract, store, DirectUploadRequest{
		Filename:    request.Filename,
		ContentType: request.ContentType,
		Size:        request.Size,
	}, ttl)
	if err != nil {
		writeDirectUploadHTTPError(w, directUploadHTTPStatus(err), directUploadHTTPCode(err), err.Error())
		return
	}

	writeDirectUploadHTTPJSON(w, http.StatusOK, DirectUploadHTTPResponse{
		Key:       ticket.Key,
		UploadURL: ticket.UploadURL,
		Headers:   ticket.Headers,
	})
}

func directUploadHTTPStatus(err error) int {
	switch {
	case errors.Is(err, ErrFileSizeExceeded):
		return http.StatusRequestEntityTooLarge
	case errors.Is(err, ErrFileMimeRejected):
		return http.StatusUnsupportedMediaType
	case errors.Is(err, ErrVisibilityMismatch):
		return http.StatusInternalServerError
	default:
		return http.StatusInternalServerError
	}
}

func directUploadHTTPCode(err error) string {
	switch {
	case errors.Is(err, ErrFileSizeExceeded):
		return "storage.file_size_exceeded"
	case errors.Is(err, ErrFileMimeRejected):
		return "storage.file_mime_rejected"
	case errors.Is(err, ErrVisibilityMismatch):
		return "storage.visibility_mismatch"
	default:
		return "storage.direct_upload_failed"
	}
}

func writeDirectUploadHTTPError(w http.ResponseWriter, status int, code, message string) {
	writeDirectUploadHTTPJSON(w, status, DirectUploadHTTPError{
		Code:  code,
		Error: message,
	})
}

func writeDirectUploadHTTPJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}
