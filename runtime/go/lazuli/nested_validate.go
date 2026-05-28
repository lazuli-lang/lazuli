package lazuli

import "reflect"

// GAP-R2 — nested value-object validation for `Many<Record>` JSONB
// collections (and any record/struct with `Validate()`-bearing fields).
//
// The W1 semantic-scalar carriers (`Percentage`, `HexColor`) validate at the
// wire decode boundary via their `UnmarshalJSON` hook, which stdlib fires
// recursively when a request body decodes into a `[]Record` slice. This
// helper is the EXPLICIT, construction-time counterpart: when a record value
// is built in Go (not decoded from JSON), or when re-validation is wanted on
// a struct scanned trustingly from a JSONB column, `Validate()` on the
// generated record struct delegates here to walk the value and run every
// nested `Validate() error`.
//
// Founding principle: this is a thin reflection walk into `reflect` (stdlib),
// not a homegrown validator engine. Constraint keywords (`min`/`max`/…) stay
// the responsibility of the `go-playground/validator` tags emitted on the
// struct; this path only chains the type-carried `Validate()` methods.

// Validatable is anything that can validate itself. The W1 semantic-scalar
// carriers (`Percentage`, `HexColor`) and every generated record struct
// satisfy it.
type Validatable interface {
	Validate() error
}

// validatableType is the cached reflect.Type of the Validatable interface,
// used to test whether a field's type implements `Validate() error`.
var validatableType = reflect.TypeOf((*Validatable)(nil)).Elem()

// ValidateValue walks v and runs `Validate()` on every nested value that
// implements [Validatable] — struct fields, slice/array elements, map
// values, and the pointee of pointers. Returns the first error encountered.
//
// The top-level value's own `Validate()` is intentionally NOT called at the
// root (the generated record method that delegates here IS that `Validate()`;
// re-invoking it would recurse). Nested values, however, ARE checked: a
// `Many<Record>` field whose element type carries an `@semantic.Percentage`
// is validated per row. When a nested value is itself [Validatable], its own
// `Validate()` owns the recursion into its fields, so this walk does not
// descend further into it.
func ValidateValue(v any) error {
	return walkChildren(reflect.ValueOf(v))
}

// walkChildren validates the immediate children of rv (without calling rv's
// own Validate — that's the caller's job at the root, and validateNode's job
// for nested nodes).
func walkChildren(rv reflect.Value) error {
	if !rv.IsValid() {
		return nil
	}
	switch rv.Kind() {
	case reflect.Pointer, reflect.Interface:
		if rv.IsNil() {
			return nil
		}
		return walkChildren(rv.Elem())
	case reflect.Struct:
		for i := 0; i < rv.NumField(); i++ {
			if !rv.Type().Field(i).IsExported() {
				continue
			}
			if err := validateNode(rv.Field(i)); err != nil {
				return err
			}
		}
	case reflect.Slice, reflect.Array:
		for i := 0; i < rv.Len(); i++ {
			if err := validateNode(rv.Index(i)); err != nil {
				return err
			}
		}
	case reflect.Map:
		for _, key := range rv.MapKeys() {
			if err := validateNode(rv.MapIndex(key)); err != nil {
				return err
			}
		}
	}
	return nil
}

// validateNode validates a single nested node: if it is [Validatable], run
// its own `Validate()` (which owns its sub-tree); otherwise recurse into its
// children so deeper carriers are still reached.
func validateNode(rv reflect.Value) error {
	if err, handled := callValidate(rv); handled {
		return err
	}
	return walkChildren(rv)
}

// callValidate runs `Validate()` on rv if its type (or pointer-to-it)
// implements [Validatable]. The second return reports whether a validator was
// found and run, so the caller can skip the generic child walk for that node.
func callValidate(rv reflect.Value) (error, bool) {
	if !rv.IsValid() || !rv.CanInterface() {
		return nil, false
	}
	t := rv.Type()
	if t.Implements(validatableType) {
		if (rv.Kind() == reflect.Pointer || rv.Kind() == reflect.Interface) && rv.IsNil() {
			return nil, true
		}
		return rv.Interface().(Validatable).Validate(), true
	}
	// Value type doesn't implement it directly, but a pointer-to-it might
	// (pointer-receiver methods). Addressable values can be promoted.
	if rv.CanAddr() && reflect.PointerTo(t).Implements(validatableType) {
		return rv.Addr().Interface().(Validatable).Validate(), true
	}
	return nil, false
}
