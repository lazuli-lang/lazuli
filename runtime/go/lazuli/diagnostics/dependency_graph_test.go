package diagnostics_test

import (
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/diagnostics"
)

func TestDependencyGraphOrdersDependenciesFirst(t *testing.T) {
	t.Parallel()

	graph := diagnostics.NewDependencyGraph(
		[]diagnostics.DependencyNode{
			{Kind: diagnostics.DependencyFeature, Name: " billing "},
			{Kind: diagnostics.DependencyResource, Name: "invoice"},
			{Kind: diagnostics.DependencyAdapter, Name: "postgres"},
		},
		[]diagnostics.DependencyEdge{
			{
				From:   dependencyGraphTestRef(diagnostics.DependencyFeature, "billing"),
				To:     dependencyGraphTestRef(diagnostics.DependencyResource, "invoice"),
				Reason: "commands write invoices",
			},
			{
				From: dependencyGraphTestRef(diagnostics.DependencyResource, "invoice"),
				To:   dependencyGraphTestRef(diagnostics.DependencyAdapter, "postgres"),
			},
		},
	)

	if diagnostics := graph.Diagnostics(); len(diagnostics) != 0 {
		t.Fatalf("Diagnostics() = %#v, want none", diagnostics)
	}

	if got := dependencyGraphTestNodeRefs(graph.Nodes()); !reflect.DeepEqual(got, []string{
		"adapter:postgres",
		"feature:billing",
		"resource:invoice",
	}) {
		t.Fatalf("Nodes() refs = %v", got)
	}

	order, ok := graph.TopologicalOrder()
	if !ok {
		t.Fatal("TopologicalOrder() ok = false, want true")
	}
	if got := dependencyGraphTestNodeRefs(order); !reflect.DeepEqual(got, []string{
		"adapter:postgres",
		"resource:invoice",
		"feature:billing",
	}) {
		t.Fatalf("TopologicalOrder() refs = %v", got)
	}
}

func TestDependencyGraphDetectsCycles(t *testing.T) {
	t.Parallel()

	graph := diagnostics.NewDependencyGraph(
		[]diagnostics.DependencyNode{
			{Kind: diagnostics.DependencyFeature, Name: "billing"},
			{Kind: diagnostics.DependencyResource, Name: "invoice"},
			{Kind: diagnostics.DependencyAdapter, Name: "postgres"},
		},
		[]diagnostics.DependencyEdge{
			{
				From: diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyFeature, Name: "billing"},
				To:   diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyResource, Name: "invoice"},
				Path: "features/billing.lzi",
				Line: 4,
			},
			{
				From: diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyResource, Name: "invoice"},
				To:   diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyAdapter, Name: "postgres"},
				Path: "features/billing.lzi",
				Line: 5,
			},
			{
				From: diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyAdapter, Name: "postgres"},
				To:   diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyFeature, Name: "billing"},
				Path: "adapters/postgres.lzi",
				Line: 2,
			},
		},
	)

	if _, ok := graph.TopologicalOrder(); ok {
		t.Fatal("TopologicalOrder() ok = true, want false for cycle")
	}

	cycles := graph.Cycles()
	if got := dependencyGraphTestCycleStrings(cycles); !reflect.DeepEqual(got, []string{
		"adapter:postgres -> feature:billing -> resource:invoice -> adapter:postgres",
	}) {
		t.Fatalf("Cycles() = %v", got)
	}

	wantDiagnostics := []diagnostics.Diagnostic{
		{
			Code:     diagnostics.CodeDependencyGraphCycle,
			Severity: diagnostics.SeverityError,
			Message:  "dependency cycle: adapter:postgres -> feature:billing -> resource:invoice -> adapter:postgres",
			Path:     "adapters/postgres.lzi",
			Line:     2,
		},
	}
	if got := graph.Diagnostics(); !reflect.DeepEqual(got, wantDiagnostics) {
		t.Fatalf("Diagnostics() = %#v, want %#v", got, wantDiagnostics)
	}
}

func TestDependencyGraphReportsInvalidMetadataDeterministically(t *testing.T) {
	t.Parallel()

	graph := diagnostics.NewDependencyGraph(
		[]diagnostics.DependencyNode{
			{Kind: diagnostics.DependencyResource, Name: "invoice", Path: "features/billing.lzi", Line: 2, Column: 3},
			{Kind: diagnostics.DependencyResource, Name: "invoice", Path: "features/billing.lzi", Line: 3, Column: 3},
			{Kind: diagnostics.DependencyNodeKind("slot"), Name: "primary", Path: "features/billing.lzi", Line: 4, Column: 3},
			{Kind: diagnostics.DependencyFeature, Name: "billing", Path: "features/billing.lzi", Line: 5, Column: 3},
		},
		[]diagnostics.DependencyEdge{
			{
				From: diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyFeature, Name: "billing"},
				To:   diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyAdapter, Name: "postgres"},
				Path: "features/billing.lzi",
				Line: 6,
			},
			{
				From: diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyFeature, Name: ""},
				To:   diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyResource, Name: "invoice"},
				Path: "features/billing.lzi",
				Line: 7,
			},
		},
	)

	if _, ok := graph.TopologicalOrder(); ok {
		t.Fatal("TopologicalOrder() ok = true, want false for invalid graph")
	}

	wantMessages := []string{
		"dependency graph has duplicate node resource:invoice",
		"dependency graph node slot:primary has invalid kind",
		"dependency graph edge feature:billing -> adapter:postgres references unknown target node adapter:postgres",
		"dependency graph edge feature: -> resource:invoice has empty source name",
	}
	if got := dependencyGraphTestDiagnosticMessages(graph.Diagnostics()); !reflect.DeepEqual(got, wantMessages) {
		t.Fatalf("diagnostic messages = %v, want %v", got, wantMessages)
	}
}

func TestDependencyGraphReportReturnsCopies(t *testing.T) {
	t.Parallel()

	graph := diagnostics.NewDependencyGraph(
		[]diagnostics.DependencyNode{
			{Kind: diagnostics.DependencyFeature, Name: "billing"},
			{Kind: diagnostics.DependencyResource, Name: "invoice"},
		},
		[]diagnostics.DependencyEdge{
			{
				From: diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyFeature, Name: "billing"},
				To:   diagnostics.DependencyNodeRef{Kind: diagnostics.DependencyResource, Name: "invoice"},
			},
		},
	)

	report := graph.Report()
	report.Nodes[0].Name = "changed"
	report.Edges[0].Reason = "changed"
	report.TopologicalOrder[0].Name = "changed"

	if got := dependencyGraphTestNodeRefs(graph.Nodes()); !reflect.DeepEqual(got, []string{
		"feature:billing",
		"resource:invoice",
	}) {
		t.Fatalf("Nodes() after caller mutation = %v", got)
	}
	if got := graph.Edges()[0].Reason; got != "" {
		t.Fatalf("Edges()[0].Reason after caller mutation = %q, want empty", got)
	}
	order, ok := graph.TopologicalOrder()
	if !ok {
		t.Fatal("TopologicalOrder() ok = false, want true")
	}
	if got := dependencyGraphTestNodeRefs(order); !reflect.DeepEqual(got, []string{
		"resource:invoice",
		"feature:billing",
	}) {
		t.Fatalf("TopologicalOrder() after caller mutation = %v", got)
	}
}

func dependencyGraphTestRef(kind diagnostics.DependencyNodeKind, name string) diagnostics.DependencyNodeRef {
	return diagnostics.DependencyNodeRef{Kind: kind, Name: name}
}

func dependencyGraphTestNodeRefs(nodes []diagnostics.DependencyNode) []string {
	refs := make([]string, 0, len(nodes))
	for _, node := range nodes {
		refs = append(refs, node.Ref().String())
	}
	return refs
}

func dependencyGraphTestCycleStrings(cycles []diagnostics.DependencyCycle) []string {
	out := make([]string, 0, len(cycles))
	for _, cycle := range cycles {
		parts := make([]string, 0, len(cycle.Nodes))
		for _, node := range cycle.Nodes {
			parts = append(parts, node.String())
		}
		out = append(out, strings.Join(parts, " -> "))
	}
	return out
}

func dependencyGraphTestDiagnosticMessages(diagnostics []diagnostics.Diagnostic) []string {
	messages := make([]string, 0, len(diagnostics))
	for _, diagnostic := range diagnostics {
		messages = append(messages, diagnostic.Message)
	}
	return messages
}
