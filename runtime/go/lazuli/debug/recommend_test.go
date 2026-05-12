package debug

import (
	"strings"
	"testing"
)

func TestRecommendRoutesFromOriginCodeSourceAndProfile(t *testing.T) {
	tests := []struct {
		name string
		hint RecommendationHints
		want Recommendation
	}{
		{
			name: "user dsl source",
			hint: RecommendationHints{
				Origin: " user_dsl ",
				Source: "features/customer.lzi:42:8",
			},
			want: Recommendation{
				DebugRoute: DebugRouteReadLZI,
				NextAction: "Open features/customer.lzi:42:8 and inspect the authored operation.",
			},
		},
		{
			name: "lib internal origin",
			hint: RecommendationHints{
				Origin: "lib_internal",
				Code:   "internal",
			},
			want: Recommendation{
				DebugRoute: DebugRouteFileIssue,
				NextAction: "File a Lazuli core issue with code internal and the envelope.",
			},
		},
		{
			name: "codegen origin with profile pattern",
			hint: RecommendationHints{
				Origin: "codegen_bug",
				Profile: ProfileHints{
					PatternID:      "command_pgx_insert",
					PatternVersion: "v1",
				},
			},
			want: Recommendation{
				DebugRoute: DebugRouteContactCodegenOwner,
				NextAction: "Contact the codegen-go owner with pattern command_pgx_insert v1 and the envelope.",
			},
		},
		{
			name: "adapter origin",
			hint: RecommendationHints{
				Origin: "adapter_runtime",
				Code:   "integration_error",
			},
			want: Recommendation{
				DebugRoute: DebugRouteContactAdapterAuthor,
				NextAction: "Contact the adapter author with code integration_error and the envelope.",
			},
		},
		{
			name: "uncatalogued sentinel routes to codegen",
			hint: RecommendationHints{
				Origin: "lib_internal",
				Code:   "uncatalogued_sentinel",
			},
			want: Recommendation{
				DebugRoute: DebugRouteContactCodegenOwner,
				NextAction: "Contact the codegen-go owner to catalog the sentinel wrap.",
			},
		},
		{
			name: "code without origin",
			hint: RecommendationHints{
				Code:    "validation_failed",
				Feature: "customer",
				Kind:    "command",
				Op:      "create",
			},
			want: Recommendation{
				DebugRoute: DebugRouteReadLZI,
				NextAction: "Inspect the .lzi block for customer.command.create.",
			},
		},
		{
			name: "profile pattern across ops routes to codegen",
			hint: RecommendationHints{
				Profile: ProfileHints{
					Axis:           "alloc",
					PatternID:      "query_pgx_list",
					PatternVersion: "v2",
					OpCount:        4,
				},
			},
			want: Recommendation{
				DebugRoute: DebugRouteContactCodegenOwner,
				NextAction: "Contact the codegen-go owner with pattern query_pgx_list v2 and the envelope.",
			},
		},
		{
			name: "numeric origin",
			hint: RecommendationHints{
				Origin: "3",
			},
			want: Recommendation{
				DebugRoute: DebugRouteContactAdapterAuthor,
				NextAction: "Contact the adapter author with the envelope and retry context.",
			},
		},
		{
			name: "empty hints default to source",
			hint: RecommendationHints{},
			want: Recommendation{
				DebugRoute: DebugRouteReadLZI,
				NextAction: "Inspect the .lzi source for the failing operation.",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := Recommend(tt.hint)
			if got != tt.want {
				t.Fatalf("Recommend() = %#v, want %#v", got, tt.want)
			}
			if gotRoute := RecommendDebugRoute(tt.hint); gotRoute != tt.want.DebugRoute {
				t.Fatalf("RecommendDebugRoute() = %q, want %q", gotRoute, tt.want.DebugRoute)
			}
		})
	}
}

func TestRecommendDoesNotPointAtGeneratedGo(t *testing.T) {
	got := Recommend(RecommendationHints{
		Origin: "user_dsl",
		Source: "dist/go/customer/command.gen.go:380",
	})

	if got.DebugRoute != DebugRouteReadLZI {
		t.Fatalf("DebugRoute = %q, want %q", got.DebugRoute, DebugRouteReadLZI)
	}
	if strings.Contains(got.NextAction, ".gen.go") {
		t.Fatalf("NextAction = %q, want no generated Go path", got.NextAction)
	}
	const want = "Resolve the generated frame to .lzi, then inspect the authored operation."
	if got.NextAction != want {
		t.Fatalf("NextAction = %q, want %q", got.NextAction, want)
	}
}
