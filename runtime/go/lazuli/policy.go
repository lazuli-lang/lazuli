package lazuli

import (
	"errors"
)

// Policy is the resolved authorisation contract attached to a command, query,
// API endpoint, agent, or notification. The DSL author writes
// `policy @policy.<name>`; codegen resolves the named policy to its atom list
// from the feature's `policies` block and emits this struct.
type Policy struct {
	// Name is the original `@policy.<name>` reference, kept for diagnostics
	// and audit logs.
	Name string

	// Atoms is the resolved list of `@role.*`, `@scope.*`, `@actor.*`, and
	// (for structured `policy <expr>` forms) `rbac.role`, `rbac.permission`,
	// `predicate` atoms.
	//
	// Two evaluation modes coexist:
	//
	//  1. Legacy OR-of-atoms — atoms carry namespaces `actor` / `role` /
	//     `scope`; the runtime accepts the call when any atom matches the
	//     active actor.
	//
	//  2. Structured expression — atoms include `predicate` markers
	//     (`(`, `)`, `and`, `or`, `not`, `authenticated`) plus `rbac.role`
	//     / `rbac.permission` atoms emitted by `policy_expr` codegen. The
	//     evaluator walks the slice as a flattened AST.
	//
	// The evaluator (`EvalPolicy`) auto-detects which mode applies by
	// scanning for any `predicate`-namespaced atom.
	Atoms []PolicyAtom
}

// PolicyAtom is one resolved authorisation atom.
type PolicyAtom struct {
	// Namespace: "actor" | "role" | "scope" | "rbac.role" |
	// "rbac.permission" | "predicate".
	Namespace string

	// Name within the namespace. For `predicate`, one of
	// "authenticated" | "and" | "or" | "not" | "(" | ")".
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

// ---------------------------------------------------------------------------
// RBAC bridge (RB.S6.R)
// ---------------------------------------------------------------------------

// RbacRoleChecker reports whether the actor's resolved role `roleName`
// matches `name`. The generated `dist/go/rbac/rbac.gen.go` installs the
// closed-catalog implementation via `RegisterRbac`; tests may install a
// stub.
type RbacRoleChecker func(roleName, name string) bool

// RbacPermissionChecker reports whether the actor's resolved role
// `roleName` carries permission `perm`. Closure semantics live in the
// generated catalog; this hook is a single name lookup at request time.
type RbacPermissionChecker func(roleName, perm string) bool

var (
	rbacHasRole       RbacRoleChecker       = func(string, string) bool { return false }
	rbacHasPermission RbacPermissionChecker = func(string, string) bool { return false }
)

// RegisterRbac installs the RBAC catalog hooks. The generated
// `rbac.gen.go` package calls this from its `init()` block so the
// runtime can evaluate `has_role` / `has_permission` policy predicates
// without the `lazuli` package importing the generated `rbac` package
// (which would invert the dependency direction).
//
// Either checker may be nil — the no-op default stays installed and the
// affected predicate evaluates to false (fail-closed).
func RegisterRbac(role RbacRoleChecker, perm RbacPermissionChecker) {
	if role != nil {
		rbacHasRole = role
	}
	if perm != nil {
		rbacHasPermission = perm
	}
}

// ---------------------------------------------------------------------------
// Policy evaluator
// ---------------------------------------------------------------------------

// errPolicyDenied is a sentinel returned by the evaluator when the
// active actor satisfies no branch. Callers wrap it in a typed
// `*Error` with status 403 so the response envelope stays consistent.
var errPolicyDenied = errors.New("policy denied")

// EvalPolicy walks `policy.Atoms` and returns nil when the active
// context satisfies it, an error otherwise. Empty atom lists are a
// codegen invariant violation — fail closed with an internal error.
//
// Auto-detects two modes:
//
//   - Structured (any atom has Namespace == "predicate"): walks the
//     flattened expression tree emitted by `walk_policy_expr_atoms`.
//
//   - Legacy OR-of-atoms: returns nil when any atom matches via
//     `atomMatches`.
//
// Both modes share `atomMatches` for the leaf evaluation step — the
// structured form just adds combinator handling on top.
func EvalPolicy(ctx *Ctx, p Policy) error {
	if len(p.Atoms) == 0 {
		return &Error{Status: 500, Code: CodeInternal,
			Message: "command/query registered with empty policy: " + p.Name}
	}
	if hasPredicateAtom(p.Atoms) {
		ok, _ := evalExpr(ctx, p.Atoms, 0)
		if ok {
			return nil
		}
		return &Error{Status: 403, Code: CodePolicyDenied,
			Message: "no policy branch matches the active actor for " + p.Name}
	}
	for _, atom := range p.Atoms {
		if atomMatches(ctx, atom) {
			return nil
		}
	}
	return &Error{Status: 403, Code: CodePolicyDenied,
		Message:    "no policy atom matches the active actor for " + p.Name,
		MessageKey: "policy_denied"}
}

// Allow is the boolean form of EvalPolicy. Useful for call sites that
// want a yes/no answer without unpacking the typed envelope (e.g. the
// report-route mount, doctor introspection).
func Allow(ctx *Ctx, p Policy) bool { return EvalPolicy(ctx, p) == nil }

// hasPredicateAtom reports whether the atom slice carries any
// `predicate`-namespaced marker, indicating the structured form.
func hasPredicateAtom(atoms []PolicyAtom) bool {
	for _, a := range atoms {
		if a.Namespace == "predicate" {
			return true
		}
	}
	return false
}

// evalExpr walks the flattened atom slice as a recursive-descent
// expression evaluator. The atoms encode an OR-of-AND tree shape with
// explicit `(` / `)` group markers and `and` / `or` / `not` operators
// between operands. Returns (value, next-index).
//
// Grammar:
//
//	expr   := or
//	or     := and ("or" and)*
//	and    := unary ("and" unary)*
//	unary  := "not" unary | atom | "(" expr ")"
//	atom   := <non-predicate atom> | predicate.authenticated
//
// The codegen walker (`walk_policy_expr_atoms` in command.rs) emits a
// matching shape so a linear walk over the slice reconstructs the tree.
func evalExpr(ctx *Ctx, atoms []PolicyAtom, i int) (bool, int) {
	return evalOr(ctx, atoms, i)
}

func evalOr(ctx *Ctx, atoms []PolicyAtom, i int) (bool, int) {
	left, i := evalAnd(ctx, atoms, i)
	for i < len(atoms) && isPredicate(atoms[i], "or") {
		var right bool
		right, i = evalAnd(ctx, atoms, i+1)
		left = left || right
	}
	return left, i
}

func evalAnd(ctx *Ctx, atoms []PolicyAtom, i int) (bool, int) {
	left, i := evalUnary(ctx, atoms, i)
	for i < len(atoms) && isPredicate(atoms[i], "and") {
		var right bool
		right, i = evalUnary(ctx, atoms, i+1)
		left = left && right
	}
	return left, i
}

func evalUnary(ctx *Ctx, atoms []PolicyAtom, i int) (bool, int) {
	if i >= len(atoms) {
		return false, i
	}
	if isPredicate(atoms[i], "not") {
		v, j := evalUnary(ctx, atoms, i+1)
		return !v, j
	}
	if isPredicate(atoms[i], "(") {
		v, j := evalOr(ctx, atoms, i+1)
		// consume matching ')'
		if j < len(atoms) && isPredicate(atoms[j], ")") {
			j++
		}
		return v, j
	}
	return atomMatches(ctx, atoms[i]), i + 1
}

func isPredicate(a PolicyAtom, name string) bool {
	return a.Namespace == "predicate" && a.Name == name
}
