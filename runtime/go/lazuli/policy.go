package lazuli

import (
	"errors"
	"fmt"
	"reflect"
	"strings"
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

	// When (GAP-09) is an optional input-value predicate guarding this
	// atom. When non-nil the atom only participates in policy evaluation
	// if the predicate holds against the command input — otherwise the
	// atom is skipped (neither grant nor deny). Emitted by codegen from
	// `@policy.<x> when input.<field> <op> <literal>`. Evaluated by
	// EvalPolicyInput; the input-less EvalPolicy fails such an atom
	// closed (treats it as not-matching) because it has no input to test.
	When *PolicyWhen
}

// PolicyWhen (GAP-09) is a single closed comparison over a command-input
// field: `input.scope = "production"`. The runtime resolves Path against
// the request input (reflectively, via readPath) and compares to Value
// using Op. Only the simple `<input.path> <op> <literal>` form is
// modelled — richer predicates are rejected upstream by the analyzer +
// doctor (POLICY-PREDICATE-001), so this wire shape stays flat.
type PolicyWhen struct {
	// Path is the dotted input path, e.g. "input.scope".
	Path string
	// Op is the comparison token: "=", "!=", "<", "<=", ">", ">=".
	Op string
	// Value is the right-hand literal (string / int64 / bool / nil).
	Value any
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
	// Input-less entry point — no command input is available, so any
	// GAP-09 `When`-guarded atom evaluates fail-closed (its predicate
	// cannot be tested). Query / report / api call sites that have no
	// per-request struct keep using this form.
	return EvalPolicyInput(ctx, p, nil)
}

// EvalPolicyInput (GAP-09) is EvalPolicy with the command input threaded
// in so input-value-predicate atoms (`@policy.x when input.y = "z"`) can
// test their `When` guard. An atom whose guard does not hold against the
// input is skipped — it neither grants nor denies — so a category may
// carry several mutually-exclusive predicate-gated atoms. Unguarded atoms
// behave exactly as before. Command.Handle passes the typed input here;
// EvalPolicy passes nil.
func EvalPolicyInput(ctx *Ctx, p Policy, input any) error {
	if len(p.Atoms) == 0 {
		return &Error{Status: 500, Code: CodeInternal,
			Message: "contract registered with empty policy: " + p.Name}
	}
	if hasPredicateAtom(p.Atoms) {
		ok, _ := evalExpr(ctx, p.Atoms, 0, input)
		if ok {
			return nil
		}
		return &Error{Status: 403, Code: CodePolicyDenied,
			Message: "no policy branch matches the active actor for " + p.Name}
	}
	for _, atom := range p.Atoms {
		if atomMatchesInput(ctx, atom, input) {
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
func evalExpr(ctx *Ctx, atoms []PolicyAtom, i int, input any) (bool, int) {
	return evalOr(ctx, atoms, i, input)
}

func evalOr(ctx *Ctx, atoms []PolicyAtom, i int, input any) (bool, int) {
	left, i := evalAnd(ctx, atoms, i, input)
	for i < len(atoms) && isPredicate(atoms[i], "or") {
		var right bool
		right, i = evalAnd(ctx, atoms, i+1, input)
		left = left || right
	}
	return left, i
}

func evalAnd(ctx *Ctx, atoms []PolicyAtom, i int, input any) (bool, int) {
	left, i := evalUnary(ctx, atoms, i, input)
	for i < len(atoms) && isPredicate(atoms[i], "and") {
		var right bool
		right, i = evalUnary(ctx, atoms, i+1, input)
		left = left && right
	}
	return left, i
}

func evalUnary(ctx *Ctx, atoms []PolicyAtom, i int, input any) (bool, int) {
	if i >= len(atoms) {
		return false, i
	}
	if isPredicate(atoms[i], "not") {
		v, j := evalUnary(ctx, atoms, i+1, input)
		return !v, j
	}
	if isPredicate(atoms[i], "(") {
		v, j := evalOr(ctx, atoms, i+1, input)
		// consume matching ')'
		if j < len(atoms) && isPredicate(atoms[j], ")") {
			j++
		}
		return v, j
	}
	return atomMatchesInput(ctx, atoms[i], input), i + 1
}

func isPredicate(a PolicyAtom, name string) bool {
	return a.Namespace == "predicate" && a.Name == name
}

// holds evaluates the input-value predicate against the request input.
// Returns false (fail-closed) when input is nil, the path does not
// resolve, or the operator is unknown. The leading `input.` segment of
// Path is stripped before resolving against the input struct — codegen
// emits `input.scope` but readPath walks the struct from `scope`.
func (w *PolicyWhen) holds(input any) bool {
	if w == nil {
		return true
	}
	if input == nil {
		return false
	}
	path := strings.TrimPrefix(w.Path, "input.")
	got, err := readPath(reflect.ValueOf(input), path)
	if err != nil {
		return false
	}
	return compareWhen(got, w.Op, w.Value)
}

// compareWhen applies the closed comparison catalog to the resolved
// input value and the policy literal. String and numeric ordering are
// supported; equality / inequality work for any comparable pair via
// normalised comparison. Unknown ops fail closed.
func compareWhen(got any, op string, want any) bool {
	switch op {
	case "=":
		return whenEqual(got, want)
	case "!=":
		return !whenEqual(got, want)
	case "<", "<=", ">", ">=":
		c, ok := whenOrder(got, want)
		if !ok {
			return false
		}
		switch op {
		case "<":
			return c < 0
		case "<=":
			return c <= 0
		case ">":
			return c > 0
		case ">=":
			return c >= 0
		}
	}
	return false
}

// whenEqual compares two policy-predicate operands for equality with
// light numeric coercion (int kinds compare by int64 value; everything
// else falls back to %v string form so an int64 literal matches an int
// field).
func whenEqual(got, want any) bool {
	if gi, ok := toInt64(got); ok {
		if wi, ok := toInt64(want); ok {
			return gi == wi
		}
	}
	if gb, ok := got.(bool); ok {
		if wb, ok := want.(bool); ok {
			return gb == wb
		}
	}
	return fmt.Sprintf("%v", got) == fmt.Sprintf("%v", want)
}

// whenOrder returns -1/0/1 for ordered comparison of numeric or string
// operands; ok=false when the pair is not order-comparable.
func whenOrder(got, want any) (int, bool) {
	if gi, ok := toInt64(got); ok {
		if wi, ok := toInt64(want); ok {
			switch {
			case gi < wi:
				return -1, true
			case gi > wi:
				return 1, true
			default:
				return 0, true
			}
		}
	}
	gs, gok := got.(string)
	ws, wok := want.(string)
	if gok && wok {
		return strings.Compare(gs, ws), true
	}
	return 0, false
}

// toInt64 coerces the common integer kinds to int64 for comparison.
func toInt64(v any) (int64, bool) {
	switch n := v.(type) {
	case int:
		return int64(n), true
	case int8:
		return int64(n), true
	case int16:
		return int64(n), true
	case int32:
		return int64(n), true
	case int64:
		return n, true
	}
	return 0, false
}

// atomMatchesInput (GAP-09) wraps atomMatches with the optional `When`
// input-value guard. An atom with a guard that does not hold against the
// request input is treated as not-matching (skipped), so it neither
// grants the call nor forces a deny — the OR walk simply moves on. A
// guarded atom evaluated with nil input (input-less EvalPolicy) also
// fails, because there is no value to test. Atoms without a guard fall
// straight through to the unchanged atomMatches.
func atomMatchesInput(ctx *Ctx, atom PolicyAtom, input any) bool {
	if atom.When != nil && !atom.When.holds(input) {
		return false
	}
	return atomMatches(ctx, atom)
}
