package authz

import (
	"reflect"
	"strings"
)

// Effect is the outcome of a matching policy rule.
type Effect string

const (
	// EffectAllow grants the request when its rule matches.
	EffectAllow Effect = "allow"

	// EffectDeny rejects the request when its rule matches.
	EffectDeny Effect = "deny"
)

// Subject is the active caller evaluated by a policy.
type Subject struct {
	ID    any
	Roles []string
}

// Request is the authorization question being evaluated.
type Request struct {
	Subject Subject

	Resource string
	Action   string

	// ResourceID is compared to Subject.ID by self rules.
	ResourceID any

	// OwnerID is compared to Subject.ID by owner rules.
	OwnerID any
}

// Rule is one ordered policy rule. Empty Resource, Action, and Roles fields
// match any request. Owner and Self add ownership predicates when set.
type Rule struct {
	Effect Effect

	Resource string
	Action   string
	Roles    []string

	Owner bool
	Self  bool

	Reason string
}

// Policy evaluates explicit allow/deny rules before falling back to RBAC
// permissions from Roles. Rules are checked in declaration order; the first
// matching allow or deny decides the request. If no rule matches, Roles.Can
// decides using the request subject roles, resource, and action.
type Policy struct {
	Roles *RoleGraph
	Rules []Rule
}

// Result is the policy decision for a request.
type Result struct {
	Allowed bool
	Effect  Effect

	// RuleIndex is the zero-based index of the rule that decided the request,
	// or -1 when RBAC fallback or default deny produced the result.
	RuleIndex int

	Reason string
}

// Evaluate returns the authorization decision for request.
func (p Policy) Evaluate(request Request) Result {
	request = normalizeRequest(request)

	for i, rule := range p.Rules {
		if !p.ruleMatches(rule, request) {
			continue
		}

		switch rule.Effect {
		case EffectAllow:
			return Result{Allowed: true, Effect: EffectAllow, RuleIndex: i, Reason: strings.TrimSpace(rule.Reason)}
		case EffectDeny:
			return Result{Allowed: false, Effect: EffectDeny, RuleIndex: i, Reason: strings.TrimSpace(rule.Reason)}
		default:
			return Result{Allowed: false, Effect: EffectDeny, RuleIndex: i, Reason: "invalid policy rule effect"}
		}
	}

	if p.Roles != nil && p.Roles.Can(request.Subject.Roles, request.Resource, request.Action) {
		return Result{Allowed: true, Effect: EffectAllow, RuleIndex: -1, Reason: "role permission"}
	}

	return Result{Allowed: false, Effect: EffectDeny, RuleIndex: -1}
}

// Allow reports whether Evaluate permits request.
func (p Policy) Allow(request Request) bool {
	return p.Evaluate(request).Allowed
}

func (p Policy) ruleMatches(rule Rule, request Request) bool {
	resource := strings.TrimSpace(rule.Resource)
	if resource != "" && resource != request.Resource {
		return false
	}

	action := strings.TrimSpace(rule.Action)
	if action != "" && action != request.Action {
		return false
	}

	roles := cleanRoleNames(rule.Roles)
	if len(roles) > 0 && !p.hasAnyRole(request.Subject.Roles, roles) {
		return false
	}

	if rule.Owner && !sameNonZeroID(request.Subject.ID, request.OwnerID) {
		return false
	}
	if rule.Self && !sameNonZeroID(request.Subject.ID, request.ResourceID) {
		return false
	}

	return true
}

func (p Policy) hasAnyRole(activeRoles, requiredRoles []string) bool {
	activeRoles = cleanRoleNames(activeRoles)
	if len(activeRoles) == 0 {
		return false
	}

	for _, active := range activeRoles {
		for _, required := range requiredRoles {
			if active == required {
				return true
			}
			if p.Roles != nil && p.Roles.Inherits(active, required) {
				return true
			}
		}
	}
	return false
}

func normalizeRequest(request Request) Request {
	request.Subject.ID = normalizeID(request.Subject.ID)
	request.Subject.Roles = cleanRoleNames(request.Subject.Roles)
	request.Resource = strings.TrimSpace(request.Resource)
	request.Action = strings.TrimSpace(request.Action)
	request.ResourceID = normalizeID(request.ResourceID)
	request.OwnerID = normalizeID(request.OwnerID)
	return request
}

func normalizeID(value any) any {
	if text, ok := value.(string); ok {
		return strings.TrimSpace(text)
	}
	return value
}

func sameNonZeroID(a, b any) bool {
	a = normalizeID(a)
	b = normalizeID(b)
	if isZeroID(a) || isZeroID(b) {
		return false
	}
	return reflect.DeepEqual(a, b)
}

func isZeroID(value any) bool {
	if value == nil {
		return true
	}
	return reflect.ValueOf(value).IsZero()
}
