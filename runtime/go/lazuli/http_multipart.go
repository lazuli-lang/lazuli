package lazuli

import (
	"errors"
	"mime/multipart"
	"net/http"
)

const (
	// DefaultMultipartMaxMemory is the net/http default memory budget used by
	// ParseMultipartRequest when MultipartLimits.MaxMemory is not set.
	DefaultMultipartMaxMemory int64 = 32 << 20

	// CodeMultipartInvalid reports a malformed or non-multipart request body.
	CodeMultipartInvalid = "multipart_invalid"
	// CodeMultipartRequestTooLarge reports a multipart request that exceeded
	// the configured byte budget.
	CodeMultipartRequestTooLarge = "multipart_request_too_large"
	// CodeMultipartTooManyFiles reports a multipart request with too many file
	// parts.
	CodeMultipartTooManyFiles = "multipart_too_many_files"
	// CodeMultipartFieldMissing reports a required multipart field lookup miss.
	CodeMultipartFieldMissing = "multipart_field_missing"
)

var (
	// ErrMultipartInvalid is wrapped by MultipartError for malformed or
	// non-multipart request bodies.
	ErrMultipartInvalid = errors.New("lazuli/http: multipart_invalid")
	// ErrMultipartRequestTooLarge is wrapped by MultipartError when the request
	// body exceeds MultipartLimits.MaxRequestBytes.
	ErrMultipartRequestTooLarge = errors.New("lazuli/http: multipart_request_too_large")
	// ErrMultipartTooManyFiles is wrapped by MultipartError when the parsed form
	// exceeds MultipartLimits.MaxFiles.
	ErrMultipartTooManyFiles = errors.New("lazuli/http: multipart_too_many_files")
	// ErrMultipartFieldMissing is wrapped by MultipartError when a required
	// multipart field is absent.
	ErrMultipartFieldMissing = errors.New("lazuli/http: multipart_field_missing")
)

// MultipartLimits configures ParseMultipartRequest.
//
// MaxMemory is passed to Request.ParseMultipartForm. When MaxMemory is zero or
// negative, DefaultMultipartMaxMemory is used. MaxRequestBytes and MaxFiles are
// disabled when zero or negative.
type MultipartLimits struct {
	MaxMemory       int64
	MaxRequestBytes int64
	MaxFiles        int
}

// MultipartError carries structured multipart parsing context while still
// projecting to Lazuli's canonical Error envelope for Problem responses.
type MultipartError struct {
	Base  ErrorBase
	Field string
	Data  map[string]any
}

// Error implements the error interface.
func (e *MultipartError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("multipart_error", e.Base)
}

// Unwrap exposes the multipart sentinel and source cause for errors.Is/As.
func (e *MultipartError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

// As lets ProblemFromError and ClassifyError treat MultipartError as the
// canonical Lazuli Error envelope without losing its concrete typed fields.
func (e *MultipartError) As(target any) bool {
	if e == nil {
		return false
	}
	out, ok := target.(**Error)
	if !ok || out == nil {
		return false
	}
	*out = e.lazuliError()
	return true
}

// ParseMultipartRequest parses r as multipart/form-data, enforcing the given
// memory, request byte, and file count limits. The returned form may contain
// temporary files; callers own cleanup and should call form.RemoveAll when the
// handler is done with uploaded files.
func ParseMultipartRequest(w http.ResponseWriter, r *http.Request, limits MultipartLimits) (*multipart.Form, error) {
	if r == nil {
		return nil, multipartInvalidError(errors.New("nil request"))
	}

	if limits.MaxRequestBytes > 0 {
		if r.ContentLength > limits.MaxRequestBytes {
			return nil, multipartRequestTooLargeError(limits.MaxRequestBytes, r.ContentLength, nil)
		}
		if r.Body != nil {
			r.Body = http.MaxBytesReader(w, r.Body, limits.MaxRequestBytes)
		}
	}

	maxMemory := limits.MaxMemory
	if maxMemory <= 0 {
		maxMemory = DefaultMultipartMaxMemory
	}
	if err := r.ParseMultipartForm(maxMemory); err != nil {
		if r.MultipartForm != nil {
			_ = r.MultipartForm.RemoveAll()
		}
		return nil, multipartParseError(err, limits)
	}

	form := r.MultipartForm
	if form == nil {
		return nil, multipartInvalidError(errors.New("multipart form was not parsed"))
	}

	if limits.MaxFiles > 0 {
		fileCount := CountMultipartFiles(form)
		if fileCount > limits.MaxFiles {
			_ = form.RemoveAll()
			return nil, multipartTooManyFilesError(limits.MaxFiles, fileCount)
		}
	}

	return form, nil
}

// CountMultipartFiles returns the total number of file parts in form.
func CountMultipartFiles(form *multipart.Form) int {
	if form == nil {
		return 0
	}
	count := 0
	for _, files := range form.File {
		count += len(files)
	}
	return count
}

// RequiredMultipartValue returns the first value for a required non-file
// multipart field.
func RequiredMultipartValue(form *multipart.Form, field string) (string, error) {
	if form == nil || len(form.Value[field]) == 0 {
		return "", multipartFieldMissingError(field)
	}
	return form.Value[field][0], nil
}

// RequiredMultipartFile returns the first file header for a required multipart
// file field.
func RequiredMultipartFile(form *multipart.Form, field string) (*multipart.FileHeader, error) {
	files, err := RequiredMultipartFiles(form, field)
	if err != nil {
		return nil, err
	}
	return files[0], nil
}

// RequiredMultipartFiles returns every file header for a required multipart
// file field.
func RequiredMultipartFiles(form *multipart.Form, field string) ([]*multipart.FileHeader, error) {
	if form == nil || len(form.File[field]) == 0 {
		return nil, multipartFieldMissingError(field)
	}
	files := make([]*multipart.FileHeader, 0, len(form.File[field]))
	for _, file := range form.File[field] {
		if file != nil {
			files = append(files, file)
		}
	}
	if len(files) == 0 {
		return nil, multipartFieldMissingError(field)
	}
	return files, nil
}

func multipartParseError(err error, limits MultipartLimits) *MultipartError {
	var maxBytesErr *http.MaxBytesError
	if errors.As(err, &maxBytesErr) {
		limit := maxBytesErr.Limit
		if limit <= 0 {
			limit = limits.MaxRequestBytes
		}
		return multipartRequestTooLargeError(limit, 0, err)
	}
	if errors.Is(err, multipart.ErrMessageTooLarge) {
		return multipartRequestTooLargeError(limits.MaxRequestBytes, 0, err)
	}
	return multipartInvalidError(err)
}

func multipartInvalidError(cause error) *MultipartError {
	return newMultipartError(
		ErrMultipartInvalid,
		http.StatusBadRequest,
		CodeMultipartInvalid,
		"invalid multipart request",
		cause,
		nil,
	)
}

func multipartRequestTooLargeError(maxBytes, contentLength int64, cause error) *MultipartError {
	data := make(map[string]any, 2)
	if maxBytes > 0 {
		data["max_request_bytes"] = maxBytes
	}
	if contentLength > 0 {
		data["content_length"] = contentLength
	}
	return newMultipartError(
		ErrMultipartRequestTooLarge,
		http.StatusRequestEntityTooLarge,
		CodeMultipartRequestTooLarge,
		"multipart request exceeds configured byte limit",
		cause,
		data,
	)
}

func multipartTooManyFilesError(maxFiles, fileCount int) *MultipartError {
	return newMultipartError(
		ErrMultipartTooManyFiles,
		http.StatusBadRequest,
		CodeMultipartTooManyFiles,
		"multipart request has too many files",
		nil,
		map[string]any{
			"max_files":  maxFiles,
			"file_count": fileCount,
		},
	)
}

func multipartFieldMissingError(field string) *MultipartError {
	err := newMultipartError(
		ErrMultipartFieldMissing,
		http.StatusBadRequest,
		CodeMultipartFieldMissing,
		"required multipart field is missing",
		nil,
		map[string]any{
			"field": field,
		},
	)
	err.Field = field
	return err
}

func newMultipartError(kind error, status int, code, message string, cause error, data map[string]any) *MultipartError {
	if cause != nil {
		cause = errors.Join(kind, cause)
	} else {
		cause = kind
	}
	return &MultipartError{
		Base: ErrorBase{
			Code:    code,
			Origin:  OriginUserDSL,
			Status:  status,
			Message: message,
			Feature: "http_multipart",
			Kind:    "multipart",
			Cause:   cause,
		},
		Data: data,
	}
}

func (e *MultipartError) lazuliError() *Error {
	return &Error{
		Status:  e.Base.Status,
		Code:    e.Base.Code,
		Message: e.Base.Message,
		Data:    e.problemData(),
		Base:    e.Base,
	}
}

func (e *MultipartError) problemData() map[string]any {
	if e == nil || (len(e.Data) == 0 && e.Field == "") {
		return nil
	}
	data := make(map[string]any, len(e.Data)+1)
	for key, value := range e.Data {
		data[key] = value
	}
	if e.Field != "" {
		data["field"] = e.Field
	}
	return data
}
