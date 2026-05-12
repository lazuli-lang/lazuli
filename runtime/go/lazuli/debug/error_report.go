package debug

import "reflect"

const errorReportMaxChain = 64

// ErrorReport is a JSON-friendly summary of a Go error and its unwrap chain.
type ErrorReport struct {
	Message   string             `json:"message,omitempty"`
	Type      string             `json:"type,omitempty"`
	Code      string             `json:"code,omitempty"`
	Status    int                `json:"status,omitempty"`
	Chain     []ErrorReportFrame `json:"chain,omitempty"`
	Truncated bool               `json:"truncated,omitempty"`
}

// ErrorReportFrame describes one error value in an unwrap chain.
type ErrorReportFrame struct {
	Message string `json:"message,omitempty"`
	Type    string `json:"type,omitempty"`
	Code    string `json:"code,omitempty"`
	Status  int    `json:"status,omitempty"`
}

type errorReportCodeProvider interface {
	Code() string
}

type errorReportStatusProvider interface {
	Status() int
}

type errorReportUnwrapOne interface {
	Unwrap() error
}

type errorReportUnwrapMany interface {
	Unwrap() []error
}

// BuildErrorReport builds a JSON-friendly report for err.
//
// Code and Status are copied from the first error in the chain that exposes
// them through a Code/Status method or exported Code/Status field. A nil error
// returns the zero report.
func BuildErrorReport(err error) ErrorReport {
	var report ErrorReport
	if err == nil {
		return report
	}

	report.Truncated = errorReportAppendChain(&report.Chain, err)
	if len(report.Chain) == 0 {
		return report
	}

	report.Message = report.Chain[0].Message
	report.Type = report.Chain[0].Type
	for _, frame := range report.Chain {
		if report.Code == "" {
			report.Code = frame.Code
		}
		if report.Status == 0 {
			report.Status = frame.Status
		}
		if report.Code != "" && report.Status != 0 {
			break
		}
	}
	return report
}

func errorReportAppendChain(chain *[]ErrorReportFrame, err error) bool {
	if err == nil {
		return false
	}
	if len(*chain) >= errorReportMaxChain {
		return true
	}

	*chain = append(*chain, ErrorReportFrame{
		Message: errorReportMessage(err),
		Type:    errorReportTypeName(err),
		Code:    errorReportCode(err),
		Status:  errorReportStatus(err),
	})

	if errorReportIsNil(reflect.ValueOf(err)) {
		return false
	}

	if many, ok := err.(errorReportUnwrapMany); ok {
		for _, child := range errorReportUnwrapManyErrors(many) {
			if errorReportAppendChain(chain, child) {
				return true
			}
		}
		return false
	}
	if one, ok := err.(errorReportUnwrapOne); ok {
		return errorReportAppendChain(chain, errorReportUnwrapError(one))
	}
	return false
}

func errorReportMessage(err error) (message string) {
	if err == nil {
		return ""
	}
	if errorReportIsNil(reflect.ValueOf(err)) {
		return "<nil " + errorReportTypeName(err) + ">"
	}
	defer func() {
		if recover() != nil {
			message = "<error message unavailable>"
		}
	}()
	return err.Error()
}

func errorReportTypeName(err error) string {
	if err == nil {
		return ""
	}
	return reflect.TypeOf(err).String()
}

func errorReportCode(err error) string {
	if err == nil {
		return ""
	}
	if provider, ok := err.(errorReportCodeProvider); ok && !errorReportIsNil(reflect.ValueOf(err)) {
		if code, ok := errorReportCallString(provider.Code); ok && code != "" {
			return code
		}
	}
	return errorReportStringField(err, "Code")
}

func errorReportStatus(err error) int {
	if err == nil {
		return 0
	}
	if provider, ok := err.(errorReportStatusProvider); ok && !errorReportIsNil(reflect.ValueOf(err)) {
		if status, ok := errorReportCallInt(provider.Status); ok && status != 0 {
			return status
		}
	}
	return errorReportIntField(err, "Status")
}

func errorReportCallString(fn func() string) (value string, ok bool) {
	defer func() {
		if recover() != nil {
			value = ""
			ok = false
		}
	}()
	return fn(), true
}

func errorReportCallInt(fn func() int) (value int, ok bool) {
	defer func() {
		if recover() != nil {
			value = 0
			ok = false
		}
	}()
	return fn(), true
}

func errorReportUnwrapError(err errorReportUnwrapOne) (unwrapped error) {
	defer func() {
		if recover() != nil {
			unwrapped = nil
		}
	}()
	return err.Unwrap()
}

func errorReportUnwrapManyErrors(err errorReportUnwrapMany) (unwrapped []error) {
	defer func() {
		if recover() != nil {
			unwrapped = nil
		}
	}()
	return err.Unwrap()
}

func errorReportStringField(err error, name string) string {
	field, ok := errorReportField(err, name)
	if !ok || field.Kind() != reflect.String {
		return ""
	}
	return field.String()
}

func errorReportIntField(err error, name string) int {
	field, ok := errorReportField(err, name)
	if !ok {
		return 0
	}
	switch field.Kind() {
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		value := field.Int()
		if value < errorReportMinInt || value > errorReportMaxInt {
			return 0
		}
		return int(value)
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
		value := field.Uint()
		if value > uint64(errorReportMaxInt) {
			return 0
		}
		return int(value)
	default:
		return 0
	}
}

func errorReportField(err error, name string) (reflect.Value, bool) {
	value := reflect.ValueOf(err)
	for value.IsValid() && (value.Kind() == reflect.Interface || value.Kind() == reflect.Pointer) {
		if value.IsNil() {
			return reflect.Value{}, false
		}
		value = value.Elem()
	}
	if !value.IsValid() || value.Kind() != reflect.Struct {
		return reflect.Value{}, false
	}

	if field, ok := errorReportStructField(value, name); ok {
		return field, true
	}

	base := value.FieldByName("Base")
	for base.IsValid() && (base.Kind() == reflect.Interface || base.Kind() == reflect.Pointer) {
		if base.IsNil() {
			return reflect.Value{}, false
		}
		base = base.Elem()
	}
	if base.IsValid() && base.Kind() == reflect.Struct {
		return errorReportStructField(base, name)
	}
	return reflect.Value{}, false
}

func errorReportStructField(value reflect.Value, name string) (reflect.Value, bool) {
	field := value.FieldByName(name)
	if !field.IsValid() || !field.CanInterface() {
		return reflect.Value{}, false
	}
	return field, true
}

func errorReportIsNil(value reflect.Value) bool {
	if !value.IsValid() {
		return true
	}
	switch value.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Pointer, reflect.Slice:
		return value.IsNil()
	default:
		return false
	}
}

var (
	errorReportMaxInt = int64(^uint(0) >> 1)
	errorReportMinInt = -errorReportMaxInt - 1
)
