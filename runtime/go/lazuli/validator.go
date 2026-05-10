package lazuli

import "reflect"

// ValidatorFunc is the runtime contract a registered validator implementation
// must satisfy. The author writes the function in `<feature>/extensions.go`
// (next to the generated `<feature>.gen.go`) and registers it with the
// runtime via `lazuli.RegisterValidator("@validator.<name>", fn)`.
//
// Returning an error aborts the surrounding command. A `*lazuli.Error`
// flows back to the client unchanged; any other error is wrapped in a
// `validation_failed` envelope with status 400.
type ValidatorFunc func(ctx *Ctx, input any) error

// validatorRegistry holds all registered validator implementations keyed by
// the canonical `@validator.<name>` reference.
var validatorRegistry = make(map[string]ValidatorFunc)

// RegisterValidator binds a `@validator.<name>` reference to a concrete
// implementation. Authors call this at package init for every validator
// extension declared in the DSL.
func RegisterValidator(ref string, fn ValidatorFunc) {
	if _, exists := validatorRegistry[ref]; exists {
		panic("lazuli: validator " + ref + " registered twice")
	}
	validatorRegistry[ref] = fn
}

// LookupValidator returns the registered implementation or nil.
func LookupValidator(ref string) ValidatorFunc {
	return validatorRegistry[ref]
}

// Field extracts a typed field value from a generic `input any` by exact
// PascalCase name. Use it inside validator implementations to read input
// fields without writing per-command type assertions:
//
//	email, ok := lazuli.Field[string](input, "Email")
//	if !ok { return errors.New("input missing Email") }
//
// Returns the zero value of T and false when the field is missing or the
// type does not match.
func Field[T any](input any, name string) (T, bool) {
	var zero T
	rv := reflect.ValueOf(input)
	if rv.Kind() == reflect.Pointer {
		rv = rv.Elem()
	}
	if rv.Kind() != reflect.Struct {
		return zero, false
	}
	f := rv.FieldByName(name)
	if !f.IsValid() {
		return zero, false
	}
	v, ok := f.Interface().(T)
	if !ok {
		return zero, false
	}
	return v, true
}
