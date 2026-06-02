package lazuli

import (
	"reflect"
	"strconv"
	"strings"
)

// pathParamNames extracts the `{name}` segments from an `api` route
// pattern (e.g. "/attachments/{id}/url" -> ["id"]). The Go 1.22+
// `http.ServeMux` wildcard syntax also admits a trailing `{name...}`
// catch-all; the trailing `...` is stripped so the bound field name
// matches the declared input field.
func pathParamNames(pattern string) []string {
	var names []string
	for _, seg := range strings.Split(pattern, "/") {
		if len(seg) >= 2 && seg[0] == '{' && seg[len(seg)-1] == '}' {
			name := seg[1 : len(seg)-1]
			name = strings.TrimSuffix(name, "...")
			if name != "" {
				names = append(names, name)
			}
		}
	}
	return names
}

// bindPathParams injects each matched `{name}=value` route variable into
// the typed api input (SEC-API-PATHARG-UNBOUND). The handler input for a
// path-keyed `api ... path "/x/{id}/y"` previously received the zero
// value for `id` because the dispatch surface only decoded the JSON
// body; path variables were silently dropped.
//
// The mapping from `{name}` to a struct field is the field's JSON tag —
// exactly the contract the codegen emits (`ID lazuli.ID `+"`"+`json:"id"`+"`"+`).
// No separate codegen metadata is required: the JSON tag already names
// the slot. Path params bind AFTER the body is decoded so a path value
// takes its declared slot (and overrides any same-named body field —
// the path is the authoritative source for `{id}`). Query-string params
// and the body keep binding through their normal channels.
//
// Values arrive as strings (URL path segments); each is coerced to the
// target field's kind (int/uint/float/bool/string and any named type
// whose underlying kind is one of those, so `lazuli.ID = int64` and
// string-newtype UUIDs both bind). A value that does not fit the field
// type returns a 400 rather than silently leaving the field zero.
func bindPathParams(input any, params map[string]string) error {
	if len(params) == 0 || input == nil {
		return nil
	}
	v := reflect.ValueOf(input)
	if v.Kind() != reflect.Pointer || v.IsNil() {
		return nil
	}
	v = v.Elem()
	if v.Kind() != reflect.Struct {
		return nil
	}
	t := v.Type()
	for name, raw := range params {
		field, ok := fieldByJSONName(t, name)
		if !ok {
			// No declared slot for this `{name}` — skip rather than
			// fail; the route can still bind the remaining params.
			continue
		}
		fv := v.FieldByIndex(field.Index)
		if !fv.CanSet() {
			continue
		}
		if err := setStringInto(fv, raw); err != nil {
			return &Error{
				Status:     400,
				Code:       CodeBadRequest,
				Message:    "invalid path parameter " + name + ": " + err.Error(),
				MessageKey: CodeBadRequest,
			}
		}
	}
	return nil
}

// fieldByJSONName resolves an exported struct field whose JSON tag (or,
// absent a tag, whose field name) matches name. The tag's options
// (`,omitempty` etc.) are ignored; a tag of "-" excludes the field.
func fieldByJSONName(t reflect.Type, name string) (reflect.StructField, bool) {
	for i := 0; i < t.NumField(); i++ {
		f := t.Field(i)
		if f.PkgPath != "" {
			continue // unexported
		}
		tag := f.Tag.Get("json")
		if tag == "-" {
			continue
		}
		jsonName := tag
		if comma := strings.IndexByte(jsonName, ','); comma >= 0 {
			jsonName = jsonName[:comma]
		}
		if jsonName == "" {
			jsonName = f.Name
		}
		if jsonName == name {
			return f, true
		}
	}
	return reflect.StructField{}, false
}

// setStringInto coerces a raw path-segment string into dst, honoring the
// destination's underlying kind so named scalar types (e.g.
// `type ID = int64`, string-backed UUID newtypes) bind correctly.
func setStringInto(dst reflect.Value, raw string) error {
	// Unwrap a pointer field, allocating as needed.
	if dst.Kind() == reflect.Pointer {
		if dst.IsNil() {
			dst.Set(reflect.New(dst.Type().Elem()))
		}
		dst = dst.Elem()
	}
	switch dst.Kind() {
	case reflect.String:
		dst.SetString(raw)
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		n, err := strconv.ParseInt(raw, 10, 64)
		if err != nil {
			return err
		}
		if dst.OverflowInt(n) {
			return strconv.ErrRange
		}
		dst.SetInt(n)
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		n, err := strconv.ParseUint(raw, 10, 64)
		if err != nil {
			return err
		}
		if dst.OverflowUint(n) {
			return strconv.ErrRange
		}
		dst.SetUint(n)
	case reflect.Float32, reflect.Float64:
		f, err := strconv.ParseFloat(raw, 64)
		if err != nil {
			return err
		}
		dst.SetFloat(f)
	case reflect.Bool:
		b, err := strconv.ParseBool(raw)
		if err != nil {
			return err
		}
		dst.SetBool(b)
	default:
		// Unsupported kind: leave a structured error so the gap is
		// visible rather than silently dropping the path value.
		return errUnsupportedPathParamKind{kind: dst.Kind().String()}
	}
	return nil
}

type errUnsupportedPathParamKind struct{ kind string }

func (e errUnsupportedPathParamKind) Error() string {
	return "unsupported path-parameter field kind " + e.kind
}
