package diagnostics

import (
	"sort"
	"strings"
)

const (
	// CodeDependencyGraphInvalid reports malformed or unresolved dependency
	// graph metadata.
	CodeDependencyGraphInvalid Code = "DEPENDENCY-GRAPH-001"

	// CodeDependencyGraphCycle reports a dependency cycle.
	CodeDependencyGraphCycle Code = "DEPENDENCY-GRAPH-002"
)

const (
	// DependencyFeature identifies a Lazuli feature node.
	DependencyFeature DependencyNodeKind = "feature"
	// DependencyAdapter identifies a Lazuli adapter node.
	DependencyAdapter DependencyNodeKind = "adapter"
	// DependencyResource identifies a Lazuli resource node.
	DependencyResource DependencyNodeKind = "resource"
)

// DependencyNodeKind is the closed set of graph node kinds understood by the
// framework diagnostics helper.
type DependencyNodeKind string

// String returns the stable lowercase node kind token.
func (k DependencyNodeKind) String() string {
	return string(k)
}

// DependencyNodeRef is the stable identity of a dependency graph node.
type DependencyNodeRef struct {
	Kind DependencyNodeKind `json:"kind"`
	Name string             `json:"name"`
}

// String renders ref as "kind:name" for diagnostics and snapshots.
func (r DependencyNodeRef) String() string {
	clean := r.normalized()
	return string(clean.Kind) + ":" + clean.Name
}

// DependencyNode describes a feature, adapter, or resource in a framework
// dependency graph. Path, Line, and Column are optional caller-owned source
// context used only for diagnostics.
type DependencyNode struct {
	Kind   DependencyNodeKind `json:"kind"`
	Name   string             `json:"name"`
	Path   string             `json:"path,omitempty"`
	Line   int                `json:"line,omitempty"`
	Column int                `json:"column,omitempty"`
}

// Ref returns the normalized identity for node.
func (n DependencyNode) Ref() DependencyNodeRef {
	return DependencyNodeRef{Kind: n.Kind, Name: n.Name}.normalized()
}

// DependencyEdge declares that From depends on To. Topological order therefore
// places To before From.
type DependencyEdge struct {
	From   DependencyNodeRef `json:"from"`
	To     DependencyNodeRef `json:"to"`
	Reason string            `json:"reason,omitempty"`
	Path   string            `json:"path,omitempty"`
	Line   int               `json:"line,omitempty"`
	Column int               `json:"column,omitempty"`
}

// DependencyCycle is one deterministic cycle path. Nodes includes the repeated
// start node at the end so callers can render the cycle directly.
type DependencyCycle struct {
	Nodes []DependencyNodeRef `json:"nodes"`
	Edges []DependencyEdge    `json:"edges"`
}

// DependencyGraph is a validated, deterministic snapshot of dependency nodes
// and edges.
type DependencyGraph struct {
	nodes            []DependencyNode
	edges            []DependencyEdge
	cycles           []DependencyCycle
	topologicalOrder []DependencyNode
	diagnostics      []Diagnostic
}

// DependencyGraphReport is the full analysis result for a graph.
type DependencyGraphReport struct {
	Nodes            []DependencyNode  `json:"nodes"`
	Edges            []DependencyEdge  `json:"edges"`
	Cycles           []DependencyCycle `json:"cycles"`
	TopologicalOrder []DependencyNode  `json:"topological_order,omitempty"`
	Diagnostics      []Diagnostic      `json:"diagnostics"`
}

// NewDependencyGraph analyzes nodes and edges into a deterministic graph
// snapshot. Invalid nodes and edges are reported as diagnostics; valid nodes and
// edges remain available for inspection.
func NewDependencyGraph(nodes []DependencyNode, edges []DependencyEdge) DependencyGraph {
	normalizedNodes, byRef, diagnostics := normalizeDependencyNodes(nodes)
	normalizedEdges, edgeDiagnostics := normalizeDependencyEdges(edges, byRef)
	diagnostics = append(diagnostics, edgeDiagnostics...)

	cycles := dependencyGraphCycles(normalizedNodes, normalizedEdges)
	for _, cycle := range cycles {
		diagnostics = append(diagnostics, dependencyCycleDiagnostic(cycle, byRef))
	}

	sortDependencyGraphDiagnostics(diagnostics)

	var order []DependencyNode
	if len(diagnostics) == 0 {
		if sorted, ok := dependencyGraphTopologicalOrder(normalizedNodes, normalizedEdges); ok {
			order = sorted
		}
	}

	return DependencyGraph{
		nodes:            normalizedNodes,
		edges:            normalizedEdges,
		cycles:           cycles,
		topologicalOrder: order,
		diagnostics:      diagnostics,
	}
}

// AnalyzeDependencyGraph returns a full deterministic report for nodes and
// edges.
func AnalyzeDependencyGraph(nodes []DependencyNode, edges []DependencyEdge) DependencyGraphReport {
	return NewDependencyGraph(nodes, edges).Report()
}

// DiagnoseDependencyGraph returns deterministic diagnostics for nodes and
// edges.
func DiagnoseDependencyGraph(nodes []DependencyNode, edges []DependencyEdge) []Diagnostic {
	return NewDependencyGraph(nodes, edges).Diagnostics()
}

// Nodes returns graph nodes sorted by kind and name.
func (g DependencyGraph) Nodes() []DependencyNode {
	return append([]DependencyNode(nil), g.nodes...)
}

// Edges returns graph edges sorted by from, to, reason, and source location.
func (g DependencyGraph) Edges() []DependencyEdge {
	return append([]DependencyEdge(nil), g.edges...)
}

// Cycles returns deterministic dependency cycles.
func (g DependencyGraph) Cycles() []DependencyCycle {
	return copyDependencyCycles(g.cycles)
}

// TopologicalOrder returns nodes in dependency-first order. The boolean is
// false when the graph has structural diagnostics or dependency cycles.
func (g DependencyGraph) TopologicalOrder() ([]DependencyNode, bool) {
	if len(g.diagnostics) != 0 || len(g.topologicalOrder) != len(g.nodes) {
		return nil, false
	}
	return append([]DependencyNode(nil), g.topologicalOrder...), true
}

// Diagnostics returns deterministic diagnostics for invalid graph metadata and
// dependency cycles.
func (g DependencyGraph) Diagnostics() []Diagnostic {
	return append([]Diagnostic(nil), g.diagnostics...)
}

// Report returns the full graph analysis snapshot.
func (g DependencyGraph) Report() DependencyGraphReport {
	order, _ := g.TopologicalOrder()
	return DependencyGraphReport{
		Nodes:            g.Nodes(),
		Edges:            g.Edges(),
		Cycles:           g.Cycles(),
		TopologicalOrder: order,
		Diagnostics:      g.Diagnostics(),
	}
}

func normalizeDependencyNodes(nodes []DependencyNode) ([]DependencyNode, map[string]DependencyNode, []Diagnostic) {
	out := make([]DependencyNode, 0, len(nodes))
	byRef := make(map[string]DependencyNode, len(nodes))
	seen := make(map[string]struct{}, len(nodes))
	diagnostics := make([]Diagnostic, 0)

	for _, node := range nodes {
		clean := normalizeDependencyNode(node)
		ref := clean.Ref()
		if diagnostic, ok := dependencyNodeDiagnostic(clean); ok {
			diagnostics = append(diagnostics, diagnostic)
			continue
		}

		key := ref.key()
		if _, exists := seen[key]; exists {
			diagnostics = append(diagnostics, dependencyInvalidDiagnostic(
				clean.Path,
				clean.Line,
				clean.Column,
				"dependency graph has duplicate node "+ref.String(),
			))
			continue
		}
		seen[key] = struct{}{}
		byRef[key] = clean
		out = append(out, clean)
	}

	sort.SliceStable(out, func(i, j int) bool {
		return dependencyNodeLess(out[i], out[j])
	})
	return out, byRef, diagnostics
}

func normalizeDependencyEdges(edges []DependencyEdge, nodes map[string]DependencyNode) ([]DependencyEdge, []Diagnostic) {
	out := make([]DependencyEdge, 0, len(edges))
	diagnostics := make([]Diagnostic, 0)

	for _, edge := range edges {
		clean := normalizeDependencyEdge(edge)
		valid := true
		if diagnostic, ok := dependencyEdgeEndpointDiagnostic(clean, clean.From, "source"); ok {
			diagnostics = append(diagnostics, diagnostic)
			valid = false
		}
		if diagnostic, ok := dependencyEdgeEndpointDiagnostic(clean, clean.To, "target"); ok {
			diagnostics = append(diagnostics, diagnostic)
			valid = false
		}
		if !valid {
			continue
		}

		if _, ok := nodes[clean.From.key()]; !ok {
			diagnostics = append(diagnostics, dependencyInvalidDiagnostic(
				clean.Path,
				clean.Line,
				clean.Column,
				"dependency graph edge "+clean.String()+" references unknown source node "+clean.From.String(),
			))
			valid = false
		}
		if _, ok := nodes[clean.To.key()]; !ok {
			diagnostics = append(diagnostics, dependencyInvalidDiagnostic(
				clean.Path,
				clean.Line,
				clean.Column,
				"dependency graph edge "+clean.String()+" references unknown target node "+clean.To.String(),
			))
			valid = false
		}
		if !valid {
			continue
		}

		out = append(out, clean)
	}

	sort.SliceStable(out, func(i, j int) bool {
		return dependencyEdgeLess(out[i], out[j])
	})
	return out, diagnostics
}

func dependencyNodeDiagnostic(node DependencyNode) (Diagnostic, bool) {
	ref := node.Ref()
	switch {
	case !dependencyNodeKindValid(ref.Kind):
		return dependencyInvalidDiagnostic(
			node.Path,
			node.Line,
			node.Column,
			"dependency graph node "+ref.String()+" has invalid kind",
		), true
	case ref.Name == "":
		return dependencyInvalidDiagnostic(
			node.Path,
			node.Line,
			node.Column,
			"dependency graph node "+ref.String()+" has empty name",
		), true
	default:
		return Diagnostic{}, false
	}
}

func dependencyEdgeEndpointDiagnostic(edge DependencyEdge, ref DependencyNodeRef, endpoint string) (Diagnostic, bool) {
	switch {
	case !dependencyNodeKindValid(ref.Kind):
		return dependencyInvalidDiagnostic(
			edge.Path,
			edge.Line,
			edge.Column,
			"dependency graph edge "+edge.String()+" has invalid "+endpoint+" kind",
		), true
	case ref.Name == "":
		return dependencyInvalidDiagnostic(
			edge.Path,
			edge.Line,
			edge.Column,
			"dependency graph edge "+edge.String()+" has empty "+endpoint+" name",
		), true
	default:
		return Diagnostic{}, false
	}
}

func dependencyInvalidDiagnostic(path string, line, column int, message string) Diagnostic {
	return Diagnostic{
		Code:     CodeDependencyGraphInvalid,
		Severity: SeverityError,
		Message:  message,
		Path:     path,
		Line:     line,
		Column:   column,
	}
}

func dependencyCycleDiagnostic(cycle DependencyCycle, nodes map[string]DependencyNode) Diagnostic {
	path, line, column := dependencyCycleLocation(cycle, nodes)
	return Diagnostic{
		Code:     CodeDependencyGraphCycle,
		Severity: SeverityError,
		Message:  "dependency cycle: " + dependencyCycleString(cycle),
		Path:     path,
		Line:     line,
		Column:   column,
	}
}

func dependencyCycleLocation(cycle DependencyCycle, nodes map[string]DependencyNode) (string, int, int) {
	for _, edge := range cycle.Edges {
		if edge.Path != "" || edge.Line != 0 || edge.Column != 0 {
			return edge.Path, edge.Line, edge.Column
		}
	}
	for _, ref := range cycle.Nodes {
		if node, ok := nodes[ref.key()]; ok && (node.Path != "" || node.Line != 0 || node.Column != 0) {
			return node.Path, node.Line, node.Column
		}
	}
	return "", 0, 0
}

func dependencyGraphCycles(nodes []DependencyNode, edges []DependencyEdge) []DependencyCycle {
	nodeRefs := make([]DependencyNodeRef, 0, len(nodes))
	for _, node := range nodes {
		nodeRefs = append(nodeRefs, node.Ref())
	}

	adjacency := dependencyGraphAdjacency(edges)
	components := dependencyGraphStrongComponents(nodeRefs, adjacency)
	cycles := make([]DependencyCycle, 0)

	for _, component := range components {
		if !dependencyGraphComponentCyclic(component, adjacency) {
			continue
		}
		if cycle, ok := dependencyGraphCycleForComponent(component, adjacency, edges); ok {
			cycles = append(cycles, cycle)
		}
	}

	sort.SliceStable(cycles, func(i, j int) bool {
		return dependencyCycleString(cycles[i]) < dependencyCycleString(cycles[j])
	})
	return cycles
}

func dependencyGraphStrongComponents(nodes []DependencyNodeRef, adjacency map[string][]DependencyNodeRef) [][]DependencyNodeRef {
	index := 0
	indexes := map[string]int{}
	lowlinks := map[string]int{}
	onStack := map[string]bool{}
	var stack []DependencyNodeRef
	var components [][]DependencyNodeRef

	var strongConnect func(DependencyNodeRef)
	strongConnect = func(node DependencyNodeRef) {
		key := node.key()
		indexes[key] = index
		lowlinks[key] = index
		index++
		stack = append(stack, node)
		onStack[key] = true

		for _, next := range adjacency[key] {
			nextKey := next.key()
			if _, ok := indexes[nextKey]; !ok {
				strongConnect(next)
				if lowlinks[nextKey] < lowlinks[key] {
					lowlinks[key] = lowlinks[nextKey]
				}
			} else if onStack[nextKey] && indexes[nextKey] < lowlinks[key] {
				lowlinks[key] = indexes[nextKey]
			}
		}

		if lowlinks[key] != indexes[key] {
			return
		}

		var component []DependencyNodeRef
		for {
			last := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			onStack[last.key()] = false
			component = append(component, last)
			if last.key() == key {
				break
			}
		}
		sort.SliceStable(component, func(i, j int) bool {
			return dependencyNodeRefLess(component[i], component[j])
		})
		components = append(components, component)
	}

	for _, node := range nodes {
		if _, ok := indexes[node.key()]; !ok {
			strongConnect(node)
		}
	}

	sort.SliceStable(components, func(i, j int) bool {
		return dependencyNodeRefLess(components[i][0], components[j][0])
	})
	return components
}

func dependencyGraphComponentCyclic(component []DependencyNodeRef, adjacency map[string][]DependencyNodeRef) bool {
	if len(component) > 1 {
		return true
	}
	if len(component) == 0 {
		return false
	}
	node := component[0]
	for _, next := range adjacency[node.key()] {
		if next.key() == node.key() {
			return true
		}
	}
	return false
}

func dependencyGraphCycleForComponent(component []DependencyNodeRef, adjacency map[string][]DependencyNodeRef, edges []DependencyEdge) (DependencyCycle, bool) {
	componentKeys := make(map[string]struct{}, len(component))
	for _, ref := range component {
		componentKeys[ref.key()] = struct{}{}
	}

	start := component[0]
	for _, next := range adjacency[start.key()] {
		if _, ok := componentKeys[next.key()]; !ok {
			continue
		}
		if next.key() == start.key() {
			return dependencyCycleFromRefs([]DependencyNodeRef{start, start}, edges), true
		}
		path, ok := dependencyGraphPath(next, start, adjacency, componentKeys)
		if ok {
			refs := append([]DependencyNodeRef{start}, path...)
			return dependencyCycleFromRefs(refs, edges), true
		}
	}
	return DependencyCycle{}, false
}

func dependencyGraphPath(from DependencyNodeRef, to DependencyNodeRef, adjacency map[string][]DependencyNodeRef, allowed map[string]struct{}) ([]DependencyNodeRef, bool) {
	type pathStep struct {
		node DependencyNodeRef
		path []DependencyNodeRef
	}

	queue := []pathStep{{node: from, path: []DependencyNodeRef{from}}}
	visited := map[string]struct{}{from.key(): {}}

	for len(queue) > 0 {
		current := queue[0]
		queue = queue[1:]
		if current.node.key() == to.key() {
			return current.path, true
		}

		for _, next := range adjacency[current.node.key()] {
			if _, ok := allowed[next.key()]; !ok {
				continue
			}
			if _, ok := visited[next.key()]; ok {
				continue
			}
			visited[next.key()] = struct{}{}
			nextPath := append(append([]DependencyNodeRef(nil), current.path...), next)
			queue = append(queue, pathStep{node: next, path: nextPath})
		}
	}
	return nil, false
}

func dependencyCycleFromRefs(refs []DependencyNodeRef, edges []DependencyEdge) DependencyCycle {
	cycle := DependencyCycle{
		Nodes: append([]DependencyNodeRef(nil), refs...),
		Edges: make([]DependencyEdge, 0, len(refs)-1),
	}
	for i := 0; i+1 < len(refs); i++ {
		if edge, ok := dependencyGraphEdgeBetween(refs[i], refs[i+1], edges); ok {
			cycle.Edges = append(cycle.Edges, edge)
		}
	}
	return cycle
}

func dependencyGraphEdgeBetween(from DependencyNodeRef, to DependencyNodeRef, edges []DependencyEdge) (DependencyEdge, bool) {
	for _, edge := range edges {
		if edge.From.key() == from.key() && edge.To.key() == to.key() {
			return edge, true
		}
	}
	return DependencyEdge{}, false
}

func dependencyGraphTopologicalOrder(nodes []DependencyNode, edges []DependencyEdge) ([]DependencyNode, bool) {
	nodeByKey := make(map[string]DependencyNode, len(nodes))
	indegree := make(map[string]int, len(nodes))
	for _, node := range nodes {
		key := node.Ref().key()
		nodeByKey[key] = node
		indegree[key] = 0
	}

	dependents := make(map[string][]DependencyNodeRef)
	seenEdges := make(map[string]struct{}, len(edges))
	for _, edge := range edges {
		key := edge.From.key() + "\x00" + edge.To.key()
		if _, ok := seenEdges[key]; ok {
			continue
		}
		seenEdges[key] = struct{}{}
		indegree[edge.From.key()]++
		dependents[edge.To.key()] = append(dependents[edge.To.key()], edge.From)
	}
	for key := range dependents {
		sort.SliceStable(dependents[key], func(i, j int) bool {
			return dependencyNodeRefLess(dependents[key][i], dependents[key][j])
		})
	}

	ready := make([]DependencyNodeRef, 0)
	for _, node := range nodes {
		if indegree[node.Ref().key()] == 0 {
			ready = append(ready, node.Ref())
		}
	}
	sort.SliceStable(ready, func(i, j int) bool {
		return dependencyNodeRefLess(ready[i], ready[j])
	})

	order := make([]DependencyNode, 0, len(nodes))
	for len(ready) > 0 {
		current := ready[0]
		ready = ready[1:]
		order = append(order, nodeByKey[current.key()])

		for _, dependent := range dependents[current.key()] {
			dependentKey := dependent.key()
			indegree[dependentKey]--
			if indegree[dependentKey] == 0 {
				ready = append(ready, dependent)
			}
		}
		sort.SliceStable(ready, func(i, j int) bool {
			return dependencyNodeRefLess(ready[i], ready[j])
		})
	}

	if len(order) != len(nodes) {
		return nil, false
	}
	return order, true
}

func dependencyGraphAdjacency(edges []DependencyEdge) map[string][]DependencyNodeRef {
	adjacency := make(map[string][]DependencyNodeRef)
	seen := make(map[string]struct{}, len(edges))
	for _, edge := range edges {
		key := edge.From.key() + "\x00" + edge.To.key()
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		adjacency[edge.From.key()] = append(adjacency[edge.From.key()], edge.To)
	}
	for key := range adjacency {
		sort.SliceStable(adjacency[key], func(i, j int) bool {
			return dependencyNodeRefLess(adjacency[key][i], adjacency[key][j])
		})
	}
	return adjacency
}

func normalizeDependencyNode(node DependencyNode) DependencyNode {
	node.Kind = DependencyNodeKind(strings.TrimSpace(string(node.Kind)))
	node.Name = strings.TrimSpace(node.Name)
	node.Path = strings.TrimSpace(node.Path)
	return node
}

func normalizeDependencyEdge(edge DependencyEdge) DependencyEdge {
	edge.From = edge.From.normalized()
	edge.To = edge.To.normalized()
	edge.Reason = strings.TrimSpace(edge.Reason)
	edge.Path = strings.TrimSpace(edge.Path)
	return edge
}

func (r DependencyNodeRef) normalized() DependencyNodeRef {
	return DependencyNodeRef{
		Kind: DependencyNodeKind(strings.TrimSpace(string(r.Kind))),
		Name: strings.TrimSpace(r.Name),
	}
}

func (r DependencyNodeRef) key() string {
	return string(r.Kind) + "\x00" + r.Name
}

func (e DependencyEdge) String() string {
	return e.From.String() + " -> " + e.To.String()
}

func dependencyNodeKindValid(kind DependencyNodeKind) bool {
	switch kind {
	case DependencyFeature, DependencyAdapter, DependencyResource:
		return true
	default:
		return false
	}
}

func dependencyNodeLess(a, b DependencyNode) bool {
	return dependencyNodeRefLess(a.Ref(), b.Ref())
}

func dependencyNodeRefLess(a, b DependencyNodeRef) bool {
	if a.Kind != b.Kind {
		return a.Kind < b.Kind
	}
	return a.Name < b.Name
}

func dependencyEdgeLess(a, b DependencyEdge) bool {
	if !dependencyNodeRefEqual(a.From, b.From) {
		return dependencyNodeRefLess(a.From, b.From)
	}
	if !dependencyNodeRefEqual(a.To, b.To) {
		return dependencyNodeRefLess(a.To, b.To)
	}
	if a.Reason != b.Reason {
		return a.Reason < b.Reason
	}
	if a.Path != b.Path {
		return a.Path < b.Path
	}
	if a.Line != b.Line {
		return a.Line < b.Line
	}
	return a.Column < b.Column
}

func dependencyNodeRefEqual(a, b DependencyNodeRef) bool {
	return a.Kind == b.Kind && a.Name == b.Name
}

func dependencyCycleString(cycle DependencyCycle) string {
	parts := make([]string, 0, len(cycle.Nodes))
	for _, node := range cycle.Nodes {
		parts = append(parts, node.String())
	}
	return strings.Join(parts, " -> ")
}

func sortDependencyGraphDiagnostics(diagnostics []Diagnostic) {
	sort.SliceStable(diagnostics, func(i, j int) bool {
		left := diagnostics[i]
		right := diagnostics[j]
		if catalogLessDiagnostic(left, right) {
			return true
		}
		if catalogLessDiagnostic(right, left) {
			return false
		}
		return left.Message < right.Message
	})
}

func copyDependencyCycles(cycles []DependencyCycle) []DependencyCycle {
	out := make([]DependencyCycle, len(cycles))
	for i, cycle := range cycles {
		out[i] = DependencyCycle{
			Nodes: append([]DependencyNodeRef(nil), cycle.Nodes...),
			Edges: append([]DependencyEdge(nil), cycle.Edges...),
		}
	}
	return out
}
