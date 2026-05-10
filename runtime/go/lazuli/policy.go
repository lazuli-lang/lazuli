package lazuli

// Policy is the resolved authorisation contract attached to a command, query,
// API endpoint, agent, or notification. The DSL author writes
// `policy @policy.<name>`; codegen resolves the named policy to its atom list
// from the feature's `policies` block and emits this struct.
type Policy struct {
	// Name is the original `@policy.<name>` reference, kept for diagnostics
	// and audit logs.
	Name string

	// Atoms is the resolved list of `@role.*`, `@scope.*`, and `@actor.*`
	// atoms that grant access. The runtime accepts the call when the active
	// actor satisfies any atom (OR semantics).
	Atoms []PolicyAtom
}

// PolicyAtom is one resolved authorisation atom.
type PolicyAtom struct {
	// Namespace: "role", "scope", "actor".
	Namespace string

	// Name within the namespace ("admin", "same_org", "system").
	Name string

	// Reason is a short human-readable explanation written next to atoms that
	// override defaults (e.g. `scope override` reasons). Optional.
	Reason string
}

// ValidatorRef points at a validator declared under the feature's
// `extensions.validator <name>` block.
type ValidatorRef struct {
	Name    string // identifier under @validator.<name>
	Feature string // owning feature (informational; not part of the registry key)
}

// Canonical returns the registry key for this validator: `@validator.<name>`.
// Author code uses this when calling `RegisterValidator(ref.Canonical(), fn)`.
func (r ValidatorRef) Canonical() string { return "@validator." + r.Name }

// V is shorthand for declaring a ValidatorRef from generated code:
//
//	Validators: []lazuli.ValidatorRef{lazuli.V("email_check")}
func V(name string) ValidatorRef { return ValidatorRef{Name: name} }
