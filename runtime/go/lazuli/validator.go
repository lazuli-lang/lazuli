package lazuli

// ValidatorFunc is the runtime contract a registered validator implementation
// must satisfy. The author writes the function in `<package>/extensions.go`
// (or wherever the extension lives) and registers it with the runtime via
// `lazuli.RegisterValidator("@validator.<name>", fn)`.
//
// Returning an error aborts the surrounding command. The error envelope
// flows back to the client.
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
