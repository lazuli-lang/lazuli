package lazuli

import (
	"encoding/xml"
	"errors"
	"fmt"
	"io"
	"mime"
	"net/http"
	"reflect"
	"strings"
)

const (
	xmlResponseContentType = "application/xml"

	// CodeXMLInvalid reports a malformed XML request body.
	CodeXMLInvalid = "xml_invalid"
	// CodeXMLNameMismatch reports a request root element that does not match
	// the destination struct's XMLName tag.
	CodeXMLNameMismatch = "xml_name_mismatch"
	// CodeXMLRequestTooLarge reports an XML request body that exceeded the
	// configured byte budget.
	CodeXMLRequestTooLarge = "xml_request_too_large"
	// CodeXMLUnsupportedMediaType reports a non-XML request Content-Type.
	CodeXMLUnsupportedMediaType = "xml_unsupported_media_type"
	// CodeXMLEncodeFailed reports a failure while encoding an XML response.
	CodeXMLEncodeFailed = "xml_encode_failed"
)

var (
	// ErrXMLInvalid is wrapped by XMLBindingError for malformed XML request
	// bodies.
	ErrXMLInvalid = errors.New("lazuli/http: xml_invalid")
	// ErrXMLNameMismatch is wrapped by XMLBindingError when the request root
	// element does not match the destination struct's XMLName tag.
	ErrXMLNameMismatch = errors.New("lazuli/http: xml_name_mismatch")
	// ErrXMLRequestTooLarge is wrapped by XMLBindingError when an XML request
	// body exceeds WithXMLBindingMaxBytes.
	ErrXMLRequestTooLarge = errors.New("lazuli/http: xml_request_too_large")
	// ErrXMLUnsupportedMediaType is wrapped by XMLBindingError when the request
	// Content-Type is not application/xml, text/xml, or a +xml media type.
	ErrXMLUnsupportedMediaType = errors.New("lazuli/http: xml_unsupported_media_type")
	// ErrXMLEncodeFailed is wrapped by XMLBindingError when an XML response
	// cannot be encoded or written.
	ErrXMLEncodeFailed = errors.New("lazuli/http: xml_encode_failed")
)

var xmlNameReflectType = reflect.TypeOf(xml.Name{})

// XMLBindingOption configures DecodeXMLRequest.
type XMLBindingOption func(*xmlBindingOptions)

type xmlBindingOptions struct {
	maxBytes int64
}

// WithXMLBindingMaxBytes caps the inbound request body. Values less than or
// equal to zero disable the cap.
func WithXMLBindingMaxBytes(maxBytes int64) XMLBindingOption {
	return func(options *xmlBindingOptions) {
		options.maxBytes = maxBytes
	}
}

// XMLBindingError carries structured XML binding context while still
// projecting to Lazuli's canonical Error envelope for Problem responses.
type XMLBindingError struct {
	Base ErrorBase
	Data map[string]any
}

// Error implements the error interface.
func (e *XMLBindingError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("xml_binding_error", e.Base)
}

// Unwrap exposes the XML binding sentinel and source cause for errors.Is/As.
func (e *XMLBindingError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

// As lets ProblemFromError treat XMLBindingError as the canonical Lazuli Error
// envelope without losing its concrete typed fields.
func (e *XMLBindingError) As(target any) bool {
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

// IsXMLContentType reports whether contentType names an XML media type. It
// accepts application/xml, text/xml, and structured syntax suffixes such as
// application/problem+xml, including parameters.
func IsXMLContentType(contentType string) bool {
	mediaType, _, err := mime.ParseMediaType(contentType)
	if err != nil {
		return false
	}
	mediaType = strings.ToLower(mediaType)
	return mediaType == "application/xml" ||
		mediaType == "text/xml" ||
		strings.HasSuffix(mediaType, "+xml")
}

// DecodeXMLRequest decodes an XML request body into dst. It rejects non-XML
// Content-Type values, honors WithXMLBindingMaxBytes, produces friendly
// XMLName mismatch diagnostics for root elements, and rejects trailing
// non-whitespace XML content.
func DecodeXMLRequest(w http.ResponseWriter, r *http.Request, dst any, opts ...XMLBindingOption) error {
	if r == nil {
		return xmlInvalidError(errors.New("nil request"))
	}
	if !IsXMLContentType(r.Header.Get("Content-Type")) {
		return xmlUnsupportedMediaTypeError(r.Header.Get("Content-Type"))
	}
	if dst == nil {
		return xmlInvalidError(errors.New("nil XML destination"))
	}
	if r.Body == nil {
		return xmlInvalidError(io.EOF)
	}
	defer r.Body.Close()

	options := applyXMLBindingOptions(opts)
	if options.maxBytes > 0 {
		if r.ContentLength > options.maxBytes {
			return xmlRequestTooLargeError(options.maxBytes, r.ContentLength, nil)
		}
		r.Body = http.MaxBytesReader(w, r.Body, options.maxBytes)
	}

	decoder := xml.NewDecoder(r.Body)
	if expected, ok := expectedRootXMLName(dst); ok {
		start, err := readXMLRootStart(decoder)
		if err != nil {
			return xmlDecodeError(err)
		}
		if !xmlNameMatches(expected, start.Name) {
			return xmlNameMismatchError(expected, start.Name)
		}
		if err := decoder.DecodeElement(dst, &start); err != nil {
			return xmlDecodeError(err)
		}
	} else if err := decoder.Decode(dst); err != nil {
		return xmlDecodeError(err)
	}

	if err := rejectTrailingXML(decoder); err != nil {
		return xmlDecodeError(err)
	}
	return nil
}

// EncodeXMLResponse writes v as an XML response. Encoding happens before the
// response headers are written so callers can still convert failures to a
// Problem response.
func EncodeXMLResponse(w http.ResponseWriter, status int, v any) error {
	if w == nil {
		return xmlEncodeError(errors.New("nil response writer"))
	}
	data, err := xml.Marshal(v)
	if err != nil {
		return xmlEncodeError(err)
	}
	if status == 0 {
		status = http.StatusOK
	}
	w.Header().Set("Content-Type", xmlResponseContentType)
	w.WriteHeader(status)
	if _, err := w.Write(data); err != nil {
		return xmlEncodeError(err)
	}
	return nil
}

func applyXMLBindingOptions(opts []XMLBindingOption) xmlBindingOptions {
	var options xmlBindingOptions
	for _, opt := range opts {
		if opt != nil {
			opt(&options)
		}
	}
	return options
}

func readXMLRootStart(decoder *xml.Decoder) (xml.StartElement, error) {
	for {
		token, err := decoder.Token()
		if err != nil {
			return xml.StartElement{}, err
		}
		switch token := token.(type) {
		case xml.StartElement:
			return token, nil
		case xml.CharData:
			if strings.TrimSpace(string(token)) != "" {
				return xml.StartElement{}, errors.New("text before XML root element")
			}
		case xml.Comment, xml.Directive, xml.ProcInst:
		default:
			return xml.StartElement{}, fmt.Errorf("unexpected XML token before root element: %T", token)
		}
	}
}

func rejectTrailingXML(decoder *xml.Decoder) error {
	for {
		token, err := decoder.Token()
		if errors.Is(err, io.EOF) {
			return nil
		}
		if err != nil {
			return err
		}
		switch token := token.(type) {
		case xml.CharData:
			if strings.TrimSpace(string(token)) == "" {
				continue
			}
		case xml.Comment, xml.Directive, xml.ProcInst:
			continue
		}
		return errors.New("unexpected trailing XML content")
	}
}

func expectedRootXMLName(dst any) (xml.Name, bool) {
	typ := reflect.TypeOf(dst)
	for typ != nil && typ.Kind() == reflect.Pointer {
		typ = typ.Elem()
	}
	if typ == nil || typ.Kind() != reflect.Struct {
		return xml.Name{}, false
	}
	field, ok := typ.FieldByName("XMLName")
	if !ok || field.Type != xmlNameReflectType {
		return xml.Name{}, false
	}
	name := parseXMLNameTag(field.Tag.Get("xml"))
	return name, name.Local != ""
}

func parseXMLNameTag(tag string) xml.Name {
	name, _, _ := strings.Cut(tag, ",")
	name = strings.TrimSpace(name)
	if name == "" || name == "-" {
		return xml.Name{}
	}
	if namespace, local, ok := strings.Cut(name, " "); ok {
		return xml.Name{
			Space: strings.TrimSpace(namespace),
			Local: strings.TrimSpace(local),
		}
	}
	return xml.Name{Local: name}
}

func xmlNameMatches(expected, actual xml.Name) bool {
	if expected.Local != actual.Local {
		return false
	}
	return expected.Space == "" || expected.Space == actual.Space
}

func xmlDecodeError(err error) error {
	var maxBytesErr *http.MaxBytesError
	if errors.As(err, &maxBytesErr) {
		return xmlRequestTooLargeError(maxBytesErr.Limit, 0, err)
	}
	return xmlInvalidError(err)
}

func xmlInvalidError(cause error) *XMLBindingError {
	message := "invalid XML body"
	if errors.Is(cause, io.EOF) {
		message = "empty XML body"
	}
	data := map[string]any{}
	var syntaxErr *xml.SyntaxError
	if errors.As(cause, &syntaxErr) && syntaxErr.Line > 0 {
		data["line"] = syntaxErr.Line
	}
	if len(data) == 0 {
		data = nil
	}
	return newXMLBindingError(
		ErrXMLInvalid,
		OriginUserDSL,
		http.StatusBadRequest,
		CodeXMLInvalid,
		message,
		cause,
		data,
	)
}

func xmlNameMismatchError(expected, actual xml.Name) *XMLBindingError {
	data := map[string]any{
		"actual_element":    actual.Local,
		"actual_xml_name":   formatXMLName(actual),
		"expected_element":  expected.Local,
		"expected_xml_name": formatXMLName(expected),
	}
	if actual.Space != "" {
		data["actual_namespace"] = actual.Space
	}
	if expected.Space != "" {
		data["expected_namespace"] = expected.Space
	}
	return newXMLBindingError(
		ErrXMLNameMismatch,
		OriginUserDSL,
		http.StatusBadRequest,
		CodeXMLNameMismatch,
		fmt.Sprintf("XML root element mismatch: expected %s, got %s", formatXMLTag(expected), formatXMLTag(actual)),
		nil,
		data,
	)
}

func xmlRequestTooLargeError(maxBytes, contentLength int64, cause error) *XMLBindingError {
	data := make(map[string]any, 2)
	if maxBytes > 0 {
		data["max_request_bytes"] = maxBytes
	}
	if contentLength > 0 {
		data["content_length"] = contentLength
	}
	return newXMLBindingError(
		ErrXMLRequestTooLarge,
		OriginUserDSL,
		http.StatusRequestEntityTooLarge,
		CodeXMLRequestTooLarge,
		"XML request exceeds configured byte limit",
		cause,
		data,
	)
}

func xmlUnsupportedMediaTypeError(contentType string) *XMLBindingError {
	data := map[string]any{
		"expected_content_types": []string{"application/xml", "text/xml", "application/*+xml"},
	}
	if contentType != "" {
		data["content_type"] = contentType
	}
	return newXMLBindingError(
		ErrXMLUnsupportedMediaType,
		OriginUserDSL,
		http.StatusUnsupportedMediaType,
		CodeXMLUnsupportedMediaType,
		"unsupported XML request content type",
		nil,
		data,
	)
}

func xmlEncodeError(cause error) *XMLBindingError {
	return newXMLBindingError(
		ErrXMLEncodeFailed,
		OriginLibInternal,
		http.StatusInternalServerError,
		CodeXMLEncodeFailed,
		"failed to encode XML response",
		cause,
		nil,
	)
}

func newXMLBindingError(kind error, origin Origin, status int, code, message string, cause error, data map[string]any) *XMLBindingError {
	if cause != nil {
		cause = errors.Join(kind, cause)
	} else {
		cause = kind
	}
	return &XMLBindingError{
		Base: ErrorBase{
			Code:    code,
			Origin:  origin,
			Status:  status,
			Message: message,
			Feature: "http_xml_binding",
			Kind:    "xml",
			Cause:   cause,
		},
		Data: data,
	}
}

func (e *XMLBindingError) lazuliError() *Error {
	return &Error{
		Status:  e.Base.Status,
		Code:    e.Base.Code,
		Message: e.Base.Message,
		Data:    e.problemData(),
		Base:    e.Base,
	}
}

func (e *XMLBindingError) problemData() map[string]any {
	if e == nil || len(e.Data) == 0 {
		return nil
	}
	data := make(map[string]any, len(e.Data))
	for key, value := range e.Data {
		data[key] = value
	}
	return data
}

func formatXMLName(name xml.Name) string {
	if name.Space == "" {
		return name.Local
	}
	return name.Space + " " + name.Local
}

func formatXMLTag(name xml.Name) string {
	if name.Space == "" {
		return "<" + name.Local + ">"
	}
	return "<{" + name.Space + "}" + name.Local + ">"
}
