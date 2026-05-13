package lazuli

import (
	"bytes"
	"encoding"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"mime"
	"net/http"
	"net/url"
	"reflect"
	"strconv"
	"strings"
)

const (
	// CodeRequestBindingInvalid reports a malformed body or unbindable field
	// value.
	CodeRequestBindingInvalid = "request_binding_invalid"
	// CodeRequestBindingUnknownField reports a JSON object member rejected by
	// WithRequestBindingDisallowUnknownJSONFields.
	CodeRequestBindingUnknownField = "request_binding_unknown_field"
	// CodeRequestBindingUnsupportedMediaType reports a missing, malformed, or
	// unsupported request Content-Type.
	CodeRequestBindingUnsupportedMediaType = "request_binding_unsupported_media_type"
	// CodeRequestBindingRequestTooLarge reports a body that exceeds the
	// configured byte limit.
	CodeRequestBindingRequestTooLarge = "request_binding_request_too_large"
	// CodeRequestBindingTargetInvalid reports a programmer error in the binding
	// destination.
	CodeRequestBindingTargetInvalid = "request_binding_target_invalid"
)

var (
	// ErrRequestBindingInvalid is wrapped by RequestBindingError for malformed
	// request bodies or invalid field values.
	ErrRequestBindingInvalid = errors.New("lazuli/http: request_binding_invalid")
	// ErrRequestBindingUnknownField is wrapped by RequestBindingError when a
	// JSON object member is rejected by the unknown-field option.
	ErrRequestBindingUnknownField = errors.New("lazuli/http: request_binding_unknown_field")
	// ErrRequestBindingUnsupportedMediaType is wrapped by RequestBindingError
	// when a request Content-Type does not match the selected binder.
	ErrRequestBindingUnsupportedMediaType = errors.New("lazuli/http: request_binding_unsupported_media_type")
	// ErrRequestBindingRequestTooLarge is wrapped by RequestBindingError when a
	// request body exceeds the configured byte limit.
	ErrRequestBindingRequestTooLarge = errors.New("lazuli/http: request_binding_request_too_large")
	// ErrRequestBindingTargetInvalid is wrapped by RequestBindingError when dst
	// is nil, not a pointer, or otherwise cannot be populated.
	ErrRequestBindingTargetInvalid = errors.New("lazuli/http: request_binding_target_invalid")
)

var textUnmarshalerType = reflect.TypeOf((*encoding.TextUnmarshaler)(nil)).Elem()

// RequestBindingOption configures request binding helpers.
type RequestBindingOption func(*requestBindingOptions)

type requestBindingOptions struct {
	MaxBytes                  int64
	DisallowUnknownJSONFields bool
}

// WithRequestBindingMaxBytes caps the request body size. A maxBytes value of
// zero or less disables the limit.
func WithRequestBindingMaxBytes(maxBytes int64) RequestBindingOption {
	return func(options *requestBindingOptions) {
		options.MaxBytes = maxBytes
	}
}

// WithRequestBindingDisallowUnknownJSONFields rejects JSON object members that
// do not map to a field in dst.
func WithRequestBindingDisallowUnknownJSONFields() RequestBindingOption {
	return func(options *requestBindingOptions) {
		options.DisallowUnknownJSONFields = true
	}
}

// RequestBindingError carries structured request binding context while still
// projecting to Lazuli's canonical Error envelope for Problem responses.
type RequestBindingError struct {
	Base  ErrorBase
	Field string
	Data  map[string]any
}

// Error implements the error interface.
func (e *RequestBindingError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("request_binding_error", e.Base)
}

// Unwrap exposes the binding sentinel and source cause for errors.Is/As.
func (e *RequestBindingError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

// As lets ProblemFromError and ClassifyError treat RequestBindingError as the
// canonical Lazuli Error envelope without losing its concrete typed fields.
func (e *RequestBindingError) As(target any) bool {
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

// BindJSONRequest decodes an application/json or application/*+json body into
// dst. dst is passed to encoding/json and should usually be a pointer to a
// struct.
func BindJSONRequest(r *http.Request, dst any, options ...RequestBindingOption) error {
	if r == nil {
		return requestBindingInvalidError("request is nil", nil, nil)
	}
	opts := applyRequestBindingOptions(options)
	if _, err := requireRequestBindingContentType(r, []string{"application/json", "application/*+json"}, isJSONRequestBindingMediaType); err != nil {
		return err
	}

	body, err := readRequestBindingBody(r, opts.MaxBytes)
	if err != nil {
		return err
	}

	decoder := json.NewDecoder(bytes.NewReader(body))
	if opts.DisallowUnknownJSONFields {
		decoder.DisallowUnknownFields()
	}
	if err := decoder.Decode(dst); err != nil {
		return jsonRequestBindingError(err)
	}

	var extra any
	if err := decoder.Decode(&extra); err != io.EOF {
		if err == nil {
			err = errors.New("multiple JSON values")
		}
		return requestBindingInvalidError("invalid JSON body", err, nil)
	}
	return nil
}

// BindFormRequest binds a multipart/form-data body into dst using form tags,
// then json tags, then exported field names. File parts are ignored; callers
// that need file handles should use ParseMultipartRequest directly.
func BindFormRequest(r *http.Request, dst any, options ...RequestBindingOption) error {
	if r == nil {
		return requestBindingInvalidError("request is nil", nil, nil)
	}
	opts := applyRequestBindingOptions(options)
	if _, err := requireRequestBindingContentType(r, []string{"multipart/form-data"}, func(mediaType string) bool {
		return mediaType == "multipart/form-data"
	}); err != nil {
		return err
	}

	if opts.MaxBytes > 0 {
		body, err := readRequestBindingBody(r, opts.MaxBytes)
		if err != nil {
			return err
		}
		r.Body = io.NopCloser(bytes.NewReader(body))
		r.ContentLength = int64(len(body))
	} else if r.Body == nil {
		return requestBindingInvalidError("request body is required", nil, nil)
	}
	defer r.Body.Close()

	if err := r.ParseMultipartForm(DefaultMultipartMaxMemory); err != nil {
		return requestBindingInvalidError("invalid multipart form body", err, nil)
	}
	if r.MultipartForm == nil {
		return requestBindingInvalidError("invalid multipart form body", errors.New("multipart form was not parsed"), nil)
	}
	defer r.MultipartForm.RemoveAll()

	values := make(url.Values, len(r.MultipartForm.Value))
	for name, fieldValues := range r.MultipartForm.Value {
		values[name] = append([]string(nil), fieldValues...)
	}
	return bindRequestBindingValues(values, dst)
}

// BindURLEncodedRequest binds an application/x-www-form-urlencoded request
// body into dst using form tags, then json tags, then exported field names.
func BindURLEncodedRequest(r *http.Request, dst any, options ...RequestBindingOption) error {
	if r == nil {
		return requestBindingInvalidError("request is nil", nil, nil)
	}
	opts := applyRequestBindingOptions(options)
	if _, err := requireRequestBindingContentType(r, []string{"application/x-www-form-urlencoded"}, func(mediaType string) bool {
		return mediaType == "application/x-www-form-urlencoded"
	}); err != nil {
		return err
	}

	body, err := readRequestBindingBody(r, opts.MaxBytes)
	if err != nil {
		return err
	}
	values, err := url.ParseQuery(string(body))
	if err != nil {
		return requestBindingInvalidError("invalid urlencoded form body", err, nil)
	}
	return bindRequestBindingValues(values, dst)
}

func applyRequestBindingOptions(options []RequestBindingOption) requestBindingOptions {
	var opts requestBindingOptions
	for _, option := range options {
		if option != nil {
			option(&opts)
		}
	}
	return opts
}

func requireRequestBindingContentType(r *http.Request, expected []string, accepts func(string) bool) (string, error) {
	raw := strings.TrimSpace(r.Header.Get("Content-Type"))
	if raw == "" {
		return "", requestBindingUnsupportedMediaTypeError("", expected, nil)
	}
	mediaType, _, err := mime.ParseMediaType(raw)
	if err != nil {
		return "", requestBindingUnsupportedMediaTypeError(raw, expected, err)
	}
	mediaType = strings.ToLower(mediaType)
	if !accepts(mediaType) {
		return "", requestBindingUnsupportedMediaTypeError(raw, expected, nil)
	}
	return mediaType, nil
}

func isJSONRequestBindingMediaType(mediaType string) bool {
	return mediaType == "application/json" ||
		(strings.HasPrefix(mediaType, "application/") && strings.HasSuffix(mediaType, "+json"))
}

func readRequestBindingBody(r *http.Request, maxBytes int64) ([]byte, error) {
	if r.Body == nil {
		return nil, requestBindingInvalidError("request body is required", nil, nil)
	}
	defer r.Body.Close()

	if maxBytes > 0 && r.ContentLength > maxBytes {
		return nil, requestBindingRequestTooLargeError(maxBytes, r.ContentLength, nil)
	}

	if maxBytes <= 0 {
		body, err := io.ReadAll(r.Body)
		if err != nil {
			return nil, requestBindingInvalidError("failed to read request body", err, nil)
		}
		return body, nil
	}

	limited := io.LimitReader(r.Body, maxBytes+1)
	body, err := io.ReadAll(limited)
	if err != nil {
		return nil, requestBindingInvalidError("failed to read request body", err, nil)
	}
	if int64(len(body)) > maxBytes {
		return nil, requestBindingRequestTooLargeError(maxBytes, 0, nil)
	}
	return body, nil
}

func jsonRequestBindingError(err error) error {
	var invalidUnmarshal *json.InvalidUnmarshalError
	if errors.As(err, &invalidUnmarshal) {
		return requestBindingTargetInvalidError(err)
	}
	if field, ok := unknownJSONFieldName(err); ok {
		return requestBindingUnknownFieldError(field, err)
	}
	return requestBindingInvalidError("invalid JSON body", err, nil)
}

func unknownJSONFieldName(err error) (string, bool) {
	if err == nil {
		return "", false
	}
	const prefix = "json: unknown field "
	message := err.Error()
	if !strings.HasPrefix(message, prefix) {
		return "", false
	}
	field, unquoteErr := strconv.Unquote(strings.TrimPrefix(message, prefix))
	if unquoteErr != nil {
		return "", true
	}
	return field, true
}

func bindRequestBindingValues(values url.Values, dst any) error {
	switch out := dst.(type) {
	case *url.Values:
		if out == nil {
			return requestBindingTargetInvalidError(errors.New("nil *url.Values destination"))
		}
		*out = cloneRequestBindingValues(values)
		return nil
	case *map[string][]string:
		if out == nil {
			return requestBindingTargetInvalidError(errors.New("nil *map[string][]string destination"))
		}
		*out = map[string][]string(cloneRequestBindingValues(values))
		return nil
	case *map[string]string:
		if out == nil {
			return requestBindingTargetInvalidError(errors.New("nil *map[string]string destination"))
		}
		*out = firstRequestBindingValues(values)
		return nil
	}

	rv := reflect.ValueOf(dst)
	if !rv.IsValid() || rv.Kind() != reflect.Pointer || rv.IsNil() {
		return requestBindingTargetInvalidError(fmt.Errorf("destination must be a non-nil pointer, got %T", dst))
	}
	rv = rv.Elem()
	if rv.Kind() != reflect.Struct {
		return requestBindingTargetInvalidError(fmt.Errorf("destination must point to a struct, got %s", rv.Type()))
	}
	return bindRequestBindingStruct(values, rv)
}

func bindRequestBindingStruct(values url.Values, rv reflect.Value) error {
	rt := rv.Type()
	for i := 0; i < rt.NumField(); i++ {
		fieldType := rt.Field(i)
		if !fieldType.IsExported() {
			continue
		}
		field := rv.Field(i)
		name, skip := requestBindingFieldName(fieldType)
		if skip {
			continue
		}
		if fieldType.Anonymous && name == "" {
			if err := bindRequestBindingEmbeddedStruct(values, field); err != nil {
				return err
			}
			continue
		}
		if name == "" {
			name = fieldType.Name
		}
		fieldValues, ok := values[name]
		if !ok {
			continue
		}
		if !field.CanSet() {
			return requestBindingTargetInvalidError(fmt.Errorf("field %s cannot be set", fieldType.Name))
		}
		if err := setRequestBindingField(field, fieldValues); err != nil {
			return requestBindingFieldInvalidError(name, err)
		}
	}
	return nil
}

func bindRequestBindingEmbeddedStruct(values url.Values, field reflect.Value) error {
	if field.Kind() == reflect.Pointer {
		if field.IsNil() {
			field.Set(reflect.New(field.Type().Elem()))
		}
		field = field.Elem()
	}
	if field.Kind() != reflect.Struct {
		return nil
	}
	return bindRequestBindingStruct(values, field)
}

func requestBindingFieldName(field reflect.StructField) (string, bool) {
	for _, tagName := range []string{"form", "json"} {
		tag := field.Tag.Get(tagName)
		if tag == "" {
			continue
		}
		name := strings.SplitN(tag, ",", 2)[0]
		if name == "-" {
			return "", true
		}
		if name != "" {
			return name, false
		}
	}
	return "", false
}

func setRequestBindingField(field reflect.Value, values []string) error {
	if len(values) == 0 {
		return nil
	}
	if field.Kind() == reflect.Slice {
		if field.Type().Elem().Kind() == reflect.Uint8 {
			field.SetBytes([]byte(values[0]))
			return nil
		}
		slice := reflect.MakeSlice(field.Type(), len(values), len(values))
		for i, value := range values {
			if err := setRequestBindingScalar(slice.Index(i), value); err != nil {
				return err
			}
		}
		field.Set(slice)
		return nil
	}
	return setRequestBindingScalar(field, values[0])
}

func setRequestBindingScalar(field reflect.Value, value string) error {
	if field.Kind() == reflect.Pointer {
		if field.IsNil() {
			field.Set(reflect.New(field.Type().Elem()))
		}
		if field.Type().Implements(textUnmarshalerType) {
			return field.Interface().(encoding.TextUnmarshaler).UnmarshalText([]byte(value))
		}
		return setRequestBindingScalar(field.Elem(), value)
	}

	if field.CanAddr() && field.Addr().Type().Implements(textUnmarshalerType) {
		return field.Addr().Interface().(encoding.TextUnmarshaler).UnmarshalText([]byte(value))
	}

	switch field.Kind() {
	case reflect.String:
		field.SetString(value)
		return nil
	case reflect.Bool:
		parsed, err := strconv.ParseBool(value)
		if err != nil {
			return err
		}
		field.SetBool(parsed)
		return nil
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		parsed, err := strconv.ParseInt(value, 10, field.Type().Bits())
		if err != nil {
			return err
		}
		field.SetInt(parsed)
		return nil
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		parsed, err := strconv.ParseUint(value, 10, field.Type().Bits())
		if err != nil {
			return err
		}
		field.SetUint(parsed)
		return nil
	case reflect.Float32, reflect.Float64:
		parsed, err := strconv.ParseFloat(value, field.Type().Bits())
		if err != nil {
			return err
		}
		field.SetFloat(parsed)
		return nil
	case reflect.Interface:
		if field.NumMethod() == 0 {
			field.Set(reflect.ValueOf(value))
			return nil
		}
	}
	return fmt.Errorf("unsupported field type %s", field.Type())
}

func cloneRequestBindingValues(values url.Values) url.Values {
	if len(values) == 0 {
		return nil
	}
	clone := make(url.Values, len(values))
	for name, fieldValues := range values {
		clone[name] = append([]string(nil), fieldValues...)
	}
	return clone
}

func firstRequestBindingValues(values url.Values) map[string]string {
	if len(values) == 0 {
		return nil
	}
	out := make(map[string]string, len(values))
	for name, fieldValues := range values {
		if len(fieldValues) == 0 {
			out[name] = ""
			continue
		}
		out[name] = fieldValues[0]
	}
	return out
}

func requestBindingInvalidError(message string, cause error, data map[string]any) *RequestBindingError {
	return newRequestBindingError(
		ErrRequestBindingInvalid,
		http.StatusBadRequest,
		CodeRequestBindingInvalid,
		message,
		OriginUserDSL,
		cause,
		data,
	)
}

func requestBindingUnknownFieldError(field string, cause error) *RequestBindingError {
	err := newRequestBindingError(
		ErrRequestBindingUnknownField,
		http.StatusBadRequest,
		CodeRequestBindingUnknownField,
		"JSON body contains an unknown field",
		OriginUserDSL,
		cause,
		map[string]any{"field": field},
	)
	err.Field = field
	return err
}

func requestBindingUnsupportedMediaTypeError(contentType string, expected []string, cause error) *RequestBindingError {
	data := map[string]any{
		"expected_content_types": append([]string(nil), expected...),
	}
	if contentType != "" {
		data["content_type"] = contentType
	}
	return newRequestBindingError(
		ErrRequestBindingUnsupportedMediaType,
		http.StatusUnsupportedMediaType,
		CodeRequestBindingUnsupportedMediaType,
		"unsupported request content type",
		OriginUserDSL,
		cause,
		data,
	)
}

func requestBindingRequestTooLargeError(maxBytes, contentLength int64, cause error) *RequestBindingError {
	data := make(map[string]any, 2)
	if maxBytes > 0 {
		data["max_bytes"] = maxBytes
	}
	if contentLength > 0 {
		data["content_length"] = contentLength
	}
	return newRequestBindingError(
		ErrRequestBindingRequestTooLarge,
		http.StatusRequestEntityTooLarge,
		CodeRequestBindingRequestTooLarge,
		"request body exceeds configured byte limit",
		OriginUserDSL,
		cause,
		data,
	)
}

func requestBindingFieldInvalidError(field string, cause error) *RequestBindingError {
	err := requestBindingInvalidError(
		"invalid form field value",
		cause,
		map[string]any{"field": field},
	)
	err.Field = field
	return err
}

func requestBindingTargetInvalidError(cause error) *RequestBindingError {
	return newRequestBindingError(
		ErrRequestBindingTargetInvalid,
		http.StatusInternalServerError,
		CodeRequestBindingTargetInvalid,
		"request binding destination is invalid",
		OriginLibInternal,
		cause,
		nil,
	)
}

func newRequestBindingError(kind error, status int, code, message string, origin Origin, cause error, data map[string]any) *RequestBindingError {
	if cause != nil {
		cause = errors.Join(kind, cause)
	} else {
		cause = kind
	}
	return &RequestBindingError{
		Base: ErrorBase{
			Code:    code,
			Origin:  origin,
			Status:  status,
			Message: message,
			Feature: "http_request_binding",
			Kind:    "request_binding",
			Cause:   cause,
		},
		Data: data,
	}
}

func (e *RequestBindingError) lazuliError() *Error {
	return &Error{
		Status:  e.Base.Status,
		Code:    e.Base.Code,
		Message: e.Base.Message,
		Data:    e.problemData(),
		Base:    e.Base,
	}
}

func (e *RequestBindingError) problemData() map[string]any {
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
