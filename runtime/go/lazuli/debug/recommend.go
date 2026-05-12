package debug

import "strings"

// RecommendationRoute is the stable concise route emitted for an IA debug
// recommendation.
type RecommendationRoute string

const (
	// DebugRouteReadLZI means the next debug read should be the authored .lzi.
	DebugRouteReadLZI RecommendationRoute = "read .lzi"
	// DebugRouteFileIssue means the failure should be reported to Lazuli core.
	DebugRouteFileIssue RecommendationRoute = "file issue"
	// DebugRouteContactCodegenOwner means the failure belongs to codegen-go.
	DebugRouteContactCodegenOwner RecommendationRoute = "contact codegen-go owner"
	// DebugRouteContactAdapterAuthor means the failure belongs to an adapter.
	DebugRouteContactAdapterAuthor RecommendationRoute = "contact adapter author"
)

// RecommendationHints carries the already-extracted debug context used to pick
// a concise next action. Origin, Code, and Source usually come from an error
// envelope; Profile carries lazuli profile attribution when the debug loop is
// performance-driven instead of error-driven.
type RecommendationHints struct {
	Origin  string
	Code    string
	Source  string
	Feature string
	Kind    string
	Op      string
	Profile ProfileHints
}

// RecommendationHint is an alias for callers that pass a single hint bundle.
type RecommendationHint = RecommendationHints

// ProfileHints carries decoded lazuli profile attribution.
type ProfileHints struct {
	Axis           string
	PatternID      string
	PatternVersion string
	OpCount        int
	Snippet        string
}

// ProfileHint is an alias for callers that pass a single profile hint bundle.
type ProfileHint = ProfileHints

// Recommendation is the route and immediate next action for the IA debug loop.
type Recommendation struct {
	DebugRoute RecommendationRoute `json:"debug_route,omitempty"`
	NextAction string              `json:"next_action,omitempty"`
}

// Recommend returns a concise route and next action for the supplied hints.
func Recommend(hints RecommendationHints) Recommendation {
	hints = recommendNormalizeHints(hints)
	route := recommendRoute(hints)
	return Recommendation{
		DebugRoute: route,
		NextAction: recommendNextAction(route, hints),
	}
}

// RecommendDebugRoute returns only the concise route for the supplied hints.
func RecommendDebugRoute(hints RecommendationHints) RecommendationRoute {
	return Recommend(hints).DebugRoute
}

func recommendNormalizeHints(hints RecommendationHints) RecommendationHints {
	hints.Origin = strings.TrimSpace(hints.Origin)
	hints.Code = strings.TrimSpace(hints.Code)
	hints.Source = strings.TrimSpace(hints.Source)
	hints.Feature = strings.TrimSpace(hints.Feature)
	hints.Kind = strings.TrimSpace(hints.Kind)
	hints.Op = strings.TrimSpace(hints.Op)
	hints.Profile.Axis = strings.TrimSpace(hints.Profile.Axis)
	hints.Profile.PatternID = strings.TrimSpace(hints.Profile.PatternID)
	hints.Profile.PatternVersion = strings.TrimSpace(hints.Profile.PatternVersion)
	hints.Profile.Snippet = strings.TrimSpace(hints.Profile.Snippet)
	if hints.Profile.OpCount < 0 {
		hints.Profile.OpCount = 0
	}
	return hints
}

func recommendRoute(hints RecommendationHints) RecommendationRoute {
	if recommendKey(hints.Code) == "uncataloguedsentinel" {
		return DebugRouteContactCodegenOwner
	}
	if recommendProfileSuggestsCodegen(hints.Profile) {
		return DebugRouteContactCodegenOwner
	}
	if route, ok := recommendRouteForOrigin(hints.Origin); ok {
		return route
	}
	if route, ok := recommendRouteForCode(hints.Code); ok {
		return route
	}
	if recommendHasProfile(hints.Profile) {
		return DebugRouteReadLZI
	}
	return DebugRouteReadLZI
}

func recommendRouteForCode(code string) (RecommendationRoute, bool) {
	switch recommendKey(code) {
	case "uncataloguedsentinel":
		return DebugRouteContactCodegenOwner, true
	case "internal":
		return DebugRouteFileIssue, true
	case "integrationerror":
		return DebugRouteContactAdapterAuthor, true
	case "badrequest", "methodnotallowed", "notfound", "policydenied",
		"ratelimited", "tenantmismatch", "validationfailed":
		return DebugRouteReadLZI, true
	default:
		return "", false
	}
}

func recommendRouteForOrigin(origin string) (RecommendationRoute, bool) {
	switch recommendKey(origin) {
	case "0", "originuserdsl", "userdsl":
		return DebugRouteReadLZI, true
	case "1", "originlibinternal", "libinternal":
		return DebugRouteFileIssue, true
	case "2", "origincodegenbug", "codegenbug":
		return DebugRouteContactCodegenOwner, true
	case "3", "originadapterruntime", "adapterruntime":
		return DebugRouteContactAdapterAuthor, true
	default:
		return "", false
	}
}

func recommendProfileSuggestsCodegen(profile ProfileHints) bool {
	return profile.PatternID != "" && profile.OpCount > 1
}

func recommendHasProfile(profile ProfileHints) bool {
	return profile.Axis != "" || profile.PatternID != "" || profile.PatternVersion != "" || profile.Snippet != ""
}

func recommendNextAction(route RecommendationRoute, hints RecommendationHints) string {
	switch route {
	case DebugRouteReadLZI:
		return recommendReadLZIAction(hints)
	case DebugRouteFileIssue:
		if hints.Code != "" {
			return "File a Lazuli core issue with code " + hints.Code + " and the envelope."
		}
		return "File a Lazuli core issue with the envelope and stack trace."
	case DebugRouteContactCodegenOwner:
		if recommendKey(hints.Code) == "uncataloguedsentinel" {
			return "Contact the codegen-go owner to catalog the sentinel wrap."
		}
		if hints.Profile.PatternID != "" {
			return "Contact the codegen-go owner with pattern " + recommendPatternLabel(hints.Profile) + " and the envelope."
		}
		return "Contact the codegen-go owner with the envelope and source hint."
	case DebugRouteContactAdapterAuthor:
		if hints.Code != "" {
			return "Contact the adapter author with code " + hints.Code + " and the envelope."
		}
		return "Contact the adapter author with the envelope and retry context."
	default:
		return "Inspect the .lzi source for the failing operation."
	}
}

func recommendReadLZIAction(hints RecommendationHints) string {
	if hints.Source != "" {
		if recommendGeneratedGoSource(hints.Source) {
			return "Resolve the generated frame to .lzi, then inspect the authored operation."
		}
		return "Open " + hints.Source + " and inspect the authored operation."
	}
	if op := recommendOpLabel(hints); op != "" {
		return "Inspect the .lzi block for " + op + "."
	}
	if recommendHasProfile(hints.Profile) {
		return "Inspect the .lzi op behind the hot profile row."
	}
	return "Inspect the .lzi source for the failing operation."
}

func recommendPatternLabel(profile ProfileHints) string {
	if profile.PatternVersion == "" {
		return profile.PatternID
	}
	return profile.PatternID + " " + profile.PatternVersion
}

func recommendOpLabel(hints RecommendationHints) string {
	parts := make([]string, 0, 3)
	for _, part := range []string{hints.Feature, hints.Kind, hints.Op} {
		if part != "" {
			parts = append(parts, part)
		}
	}
	return strings.Join(parts, ".")
}

func recommendGeneratedGoSource(source string) bool {
	source = strings.ToLower(strings.TrimSpace(source))
	return strings.Contains(source, ".gen.go")
}

func recommendKey(value string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(strings.TrimSpace(value)) {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') {
			b.WriteRune(r)
		}
	}
	return b.String()
}
