package testkit

import (
	"bufio"
	"bytes"
	"errors"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"path"
	"sort"
	"strconv"
	"strings"
)

const (
	// DefaultCoverageReportTitle is used by coverage renderers when no title is
	// provided.
	DefaultCoverageReportTitle = "Coverage Report"

	coverageScannerMaxTokenSize = 1024 * 1024
)

// ErrInvalidCoverageProfile is returned when a Go cover profile or referenced
// source file cannot be parsed.
var ErrInvalidCoverageProfile = errors.New("lazuli/testkit: invalid coverage profile")

// CoverageProfile is a parsed Go cover profile.
type CoverageProfile struct {
	Mode   string
	Blocks []CoverageBlock
}

// CoverageBlock describes one statement block from a Go cover profile.
type CoverageBlock struct {
	File        string
	StartLine   int
	StartColumn int
	EndLine     int
	EndColumn   int
	Statements  int
	Count       int64
}

// Covered reports whether the block executed at least once.
func (b CoverageBlock) Covered() bool {
	return b.Count > 0
}

// CoverageSummaryOptions configures coverage summary aggregation.
type CoverageSummaryOptions struct {
	// Source reads Go source files for function-level totals. When nil,
	// function summaries are omitted.
	Source fs.FS

	// SourceRoot is an optional prefix stripped from profile file paths before
	// reading Source. It is useful when profiles contain absolute paths and
	// Source is rooted under that directory.
	SourceRoot string
}

// CoverageTotal counts covered and total statements for a coverage scope.
type CoverageTotal struct {
	CoveredStatements int
	Statements        int
}

// Percent returns covered statements as a 0-100 percentage. Empty totals are
// treated as fully covered so zero-statement scopes do not fail thresholds.
func (t CoverageTotal) Percent() float64 {
	if t.Statements <= 0 {
		return 100
	}
	return float64(t.CoveredStatements) * 100 / float64(t.Statements)
}

// CoverageSummary contains deterministic package, file, and function totals.
type CoverageSummary struct {
	Mode      string
	Total     CoverageTotal
	Packages  []CoveragePackageSummary
	Files     []CoverageFileSummary
	Functions []CoverageFunctionSummary
}

// CoveragePackageSummary summarizes one package directory from a cover profile.
type CoveragePackageSummary struct {
	Package string
	Total   CoverageTotal
}

// CoverageFileSummary summarizes one source file from a cover profile.
type CoverageFileSummary struct {
	Package string
	File    string
	Total   CoverageTotal
}

// CoverageFunctionSummary summarizes one Go function or method.
type CoverageFunctionSummary struct {
	Package   string
	File      string
	Name      string
	StartLine int
	Total     CoverageTotal
}

// FullName returns the stable threshold/rendering key for the function.
func (f CoverageFunctionSummary) FullName() string {
	return coverageFunctionFullName(f.Package, f.Name)
}

// CoverageScope names a threshold evaluation scope.
type CoverageScope string

const (
	CoverageScopeTotal    CoverageScope = "total"
	CoverageScopePackage  CoverageScope = "package"
	CoverageScopeFile     CoverageScope = "file"
	CoverageScopeFunction CoverageScope = "function"
)

// CoverageThreshold configures minimum coverage percentages on a 0-100 scale.
// Zero and negative minimums are ignored. Package, file, and function keys
// match the corresponding summary fields; function keys use FullName.
type CoverageThreshold struct {
	Total     float64
	Packages  map[string]float64
	Files     map[string]float64
	Functions map[string]float64
}

// CoverageThresholdFailure describes one missing or under-covered scope.
type CoverageThresholdFailure struct {
	Scope    CoverageScope
	Name     string
	Minimum  float64
	Actual   float64
	Coverage CoverageTotal
	Missing  bool
}

// CoverageThresholdResult reports whether coverage met the configured
// thresholds and lists deterministic failures.
type CoverageThresholdResult struct {
	Passed   bool
	Failures []CoverageThresholdFailure
}

// CoverageReportOptions configures Markdown and text report rendering.
type CoverageReportOptions struct {
	// Title is rendered as the report title. Empty uses
	// DefaultCoverageReportTitle.
	Title string

	// Threshold, when non-zero, adds a threshold result section to reports.
	Threshold CoverageThreshold
}

// ParseCoverageProfile parses a Go cover profile.
func ParseCoverageProfile(data []byte) (CoverageProfile, error) {
	if len(bytes.TrimSpace(data)) == 0 {
		return CoverageProfile{}, fmt.Errorf("%w: empty profile", ErrInvalidCoverageProfile)
	}

	scanner := bufio.NewScanner(bytes.NewReader(data))
	scanner.Buffer(make([]byte, 0, 64*1024), coverageScannerMaxTokenSize)

	var profile CoverageProfile
	lineNumber := 0
	for scanner.Scan() {
		lineNumber++
		line := strings.TrimSpace(scanner.Text())
		if lineNumber == 1 {
			mode, err := parseCoverageMode(line)
			if err != nil {
				return CoverageProfile{}, err
			}
			profile.Mode = mode
			continue
		}
		if line == "" {
			continue
		}
		block, err := parseCoverageBlock(line)
		if err != nil {
			return CoverageProfile{}, fmt.Errorf("%w: line %d: %v", ErrInvalidCoverageProfile, lineNumber, err)
		}
		profile.Blocks = append(profile.Blocks, block)
	}
	if err := scanner.Err(); err != nil {
		return CoverageProfile{}, fmt.Errorf("%w: scan profile: %v", ErrInvalidCoverageProfile, err)
	}
	return profile, nil
}

// SummarizeCoverageProfile parses data and returns package, file, function,
// and total coverage summaries.
func SummarizeCoverageProfile(data []byte, options CoverageSummaryOptions) (CoverageSummary, error) {
	profile, err := ParseCoverageProfile(data)
	if err != nil {
		return CoverageSummary{}, err
	}
	return SummarizeCoverage(profile, options)
}

// SummarizeCoverage returns package, file, function, and total coverage
// summaries for a parsed profile.
func SummarizeCoverage(profile CoverageProfile, options CoverageSummaryOptions) (CoverageSummary, error) {
	blocks := make([]CoverageBlock, 0, len(profile.Blocks))
	for i, block := range profile.Blocks {
		normalized, err := normalizeCoverageBlock(block, i)
		if err != nil {
			return CoverageSummary{}, err
		}
		blocks = append(blocks, normalized)
	}

	functions, err := buildCoverageFunctionRanges(blocks, options)
	if err != nil {
		return CoverageSummary{}, err
	}

	summary := CoverageSummary{Mode: strings.TrimSpace(profile.Mode)}
	packages := make(map[string]*CoveragePackageSummary)
	files := make(map[string]*CoverageFileSummary)
	functionTotals := make(map[string]*CoverageFunctionSummary)

	for _, block := range blocks {
		pkg := coveragePackageForFile(block.File)
		coverageAddBlock(&summary.Total, block)

		packageSummary := packages[pkg]
		if packageSummary == nil {
			packageSummary = &CoveragePackageSummary{Package: pkg}
			packages[pkg] = packageSummary
		}
		coverageAddBlock(&packageSummary.Total, block)

		fileSummary := files[block.File]
		if fileSummary == nil {
			fileSummary = &CoverageFileSummary{Package: pkg, File: block.File}
			files[block.File] = fileSummary
		}
		coverageAddBlock(&fileSummary.Total, block)

		if function, ok := matchCoverageFunction(functions[block.File], block); ok {
			key := coverageFunctionKey(function.summary)
			functionSummary := functionTotals[key]
			if functionSummary == nil {
				value := function.summary
				functionSummary = &value
				functionTotals[key] = functionSummary
			}
			coverageAddBlock(&functionSummary.Total, block)
		}
	}

	for _, value := range packages {
		summary.Packages = append(summary.Packages, *value)
	}
	sortCoveragePackages(summary.Packages)

	for _, value := range files {
		summary.Files = append(summary.Files, *value)
	}
	sortCoverageFiles(summary.Files)

	for _, value := range functionTotals {
		summary.Functions = append(summary.Functions, *value)
	}
	sortCoverageFunctions(summary.Functions)

	return summary, nil
}

// EvaluateThreshold returns missing or under-covered scopes for threshold.
func (s CoverageSummary) EvaluateThreshold(threshold CoverageThreshold) CoverageThresholdResult {
	var failures []CoverageThresholdFailure
	if coverageThresholdActive(threshold.Total) && coverageBelowThreshold(s.Total.Percent(), threshold.Total) {
		failures = append(failures, CoverageThresholdFailure{
			Scope:    CoverageScopeTotal,
			Name:     "total",
			Minimum:  threshold.Total,
			Actual:   s.Total.Percent(),
			Coverage: s.Total,
		})
	}

	failures = append(failures, evaluateCoveragePackageThresholds(s.Packages, threshold.Packages)...)
	failures = append(failures, evaluateCoverageFileThresholds(s.Files, threshold.Files)...)
	failures = append(failures, evaluateCoverageFunctionThresholds(s.Functions, threshold.Functions)...)
	sortCoverageThresholdFailures(failures)

	return CoverageThresholdResult{
		Passed:   len(failures) == 0,
		Failures: failures,
	}
}

// MeetsThreshold reports whether all active thresholds pass.
func (s CoverageSummary) MeetsThreshold(threshold CoverageThreshold) bool {
	return s.EvaluateThreshold(threshold).Passed
}

// Markdown renders the summary as deterministic Markdown.
func (s CoverageSummary) Markdown(options CoverageReportOptions) string {
	return RenderCoverageMarkdown(s, options)
}

// Text renders the summary as deterministic plain text.
func (s CoverageSummary) Text(options CoverageReportOptions) string {
	return RenderCoverageText(s, options)
}

// RenderCoverageMarkdown renders the summary as deterministic Markdown.
func RenderCoverageMarkdown(summary CoverageSummary, options CoverageReportOptions) string {
	title := strings.TrimSpace(options.Title)
	if title == "" {
		title = DefaultCoverageReportTitle
	}

	packages := append([]CoveragePackageSummary(nil), summary.Packages...)
	files := append([]CoverageFileSummary(nil), summary.Files...)
	functions := append([]CoverageFunctionSummary(nil), summary.Functions...)
	sortCoveragePackages(packages)
	sortCoverageFiles(files)
	sortCoverageFunctions(functions)

	var b strings.Builder
	b.WriteString("# ")
	b.WriteString(coverageMarkdownText(title))
	b.WriteString("\n\n")

	if mode := strings.TrimSpace(summary.Mode); mode != "" {
		b.WriteString("Mode: `")
		b.WriteString(coverageMarkdownText(mode))
		b.WriteString("`\n\n")
	}

	b.WriteString("| Scope | Covered | Statements | Coverage |\n")
	b.WriteString("| --- | ---: | ---: | ---: |\n")
	writeCoverageMarkdownTotalRow(&b, "Total", summary.Total)
	b.WriteByte('\n')

	if len(packages) > 0 {
		b.WriteString("## Packages\n\n")
		b.WriteString("| Package | Covered | Statements | Coverage |\n")
		b.WriteString("| --- | ---: | ---: | ---: |\n")
		for _, pkg := range packages {
			writeCoverageMarkdownTotalRow(&b, pkg.Package, pkg.Total)
		}
		b.WriteByte('\n')
	}

	if len(files) > 0 {
		b.WriteString("## Files\n\n")
		b.WriteString("| File | Package | Covered | Statements | Coverage |\n")
		b.WriteString("| --- | --- | ---: | ---: | ---: |\n")
		for _, file := range files {
			b.WriteString("| ")
			b.WriteString(coverageMarkdownCell(file.File))
			b.WriteString(" | ")
			b.WriteString(coverageMarkdownCell(file.Package))
			b.WriteString(" | ")
			writeCoverageMarkdownCounts(&b, file.Total)
			b.WriteString(" |\n")
		}
		b.WriteByte('\n')
	}

	if len(functions) > 0 {
		b.WriteString("## Functions\n\n")
		b.WriteString("| Function | File | Covered | Statements | Coverage |\n")
		b.WriteString("| --- | --- | ---: | ---: | ---: |\n")
		for _, function := range functions {
			b.WriteString("| ")
			b.WriteString(coverageMarkdownCell(function.FullName()))
			b.WriteString(" | ")
			b.WriteString(coverageMarkdownCell(function.File))
			b.WriteString(" | ")
			writeCoverageMarkdownCounts(&b, function.Total)
			b.WriteString(" |\n")
		}
		b.WriteByte('\n')
	}

	if coverageHasThreshold(options.Threshold) {
		writeCoverageMarkdownThresholds(&b, summary.EvaluateThreshold(options.Threshold))
	}

	return b.String()
}

// RenderCoverageText renders the summary as deterministic plain text.
func RenderCoverageText(summary CoverageSummary, options CoverageReportOptions) string {
	title := strings.TrimSpace(options.Title)
	if title == "" {
		title = DefaultCoverageReportTitle
	}

	packages := append([]CoveragePackageSummary(nil), summary.Packages...)
	files := append([]CoverageFileSummary(nil), summary.Files...)
	functions := append([]CoverageFunctionSummary(nil), summary.Functions...)
	sortCoveragePackages(packages)
	sortCoverageFiles(files)
	sortCoverageFunctions(functions)

	var b strings.Builder
	b.WriteString(coverageTextLine(title))
	b.WriteByte('\n')
	if mode := strings.TrimSpace(summary.Mode); mode != "" {
		b.WriteString("Mode: ")
		b.WriteString(coverageTextLine(mode))
		b.WriteByte('\n')
	}
	b.WriteString("Total: ")
	writeCoverageTextTotal(&b, summary.Total)
	b.WriteString("\n\n")

	if len(packages) > 0 {
		b.WriteString("Packages:\n")
		for _, pkg := range packages {
			b.WriteString("- ")
			b.WriteString(coverageTextLine(pkg.Package))
			b.WriteString(": ")
			writeCoverageTextTotal(&b, pkg.Total)
			b.WriteByte('\n')
		}
		b.WriteByte('\n')
	}

	if len(files) > 0 {
		b.WriteString("Files:\n")
		for _, file := range files {
			b.WriteString("- ")
			b.WriteString(coverageTextLine(file.File))
			b.WriteString(" [")
			b.WriteString(coverageTextLine(file.Package))
			b.WriteString("]: ")
			writeCoverageTextTotal(&b, file.Total)
			b.WriteByte('\n')
		}
		b.WriteByte('\n')
	}

	if len(functions) > 0 {
		b.WriteString("Functions:\n")
		for _, function := range functions {
			b.WriteString("- ")
			b.WriteString(coverageTextLine(function.FullName()))
			b.WriteString(" [")
			b.WriteString(coverageTextLine(function.File))
			b.WriteString("]: ")
			writeCoverageTextTotal(&b, function.Total)
			b.WriteByte('\n')
		}
		b.WriteByte('\n')
	}

	if coverageHasThreshold(options.Threshold) {
		writeCoverageTextThresholds(&b, summary.EvaluateThreshold(options.Threshold))
	}

	return b.String()
}

func parseCoverageMode(line string) (string, error) {
	if !strings.HasPrefix(line, "mode:") {
		return "", fmt.Errorf("%w: first line must declare mode", ErrInvalidCoverageProfile)
	}
	mode := strings.TrimSpace(strings.TrimPrefix(line, "mode:"))
	switch mode {
	case "set", "count", "atomic":
		return mode, nil
	default:
		return "", fmt.Errorf("%w: unsupported mode %q", ErrInvalidCoverageProfile, mode)
	}
}

func parseCoverageBlock(line string) (CoverageBlock, error) {
	colon := strings.LastIndex(line, ":")
	if colon <= 0 || colon == len(line)-1 {
		return CoverageBlock{}, errors.New("expected file:range statements count")
	}

	fields := strings.Fields(line[colon+1:])
	if len(fields) != 3 {
		return CoverageBlock{}, errors.New("expected range, statement count, and execution count")
	}

	startLine, startColumn, endLine, endColumn, err := parseCoverageRange(fields[0])
	if err != nil {
		return CoverageBlock{}, err
	}
	statements, err := strconv.Atoi(fields[1])
	if err != nil || statements <= 0 {
		return CoverageBlock{}, fmt.Errorf("invalid statement count %q", fields[1])
	}
	count, err := strconv.ParseInt(fields[2], 10, 64)
	if err != nil || count < 0 {
		return CoverageBlock{}, fmt.Errorf("invalid execution count %q", fields[2])
	}

	block := CoverageBlock{
		File:        cleanCoverageFile(line[:colon]),
		StartLine:   startLine,
		StartColumn: startColumn,
		EndLine:     endLine,
		EndColumn:   endColumn,
		Statements:  statements,
		Count:       count,
	}
	return normalizeCoverageBlock(block, 0)
}

func parseCoverageRange(raw string) (int, int, int, int, error) {
	parts := strings.Split(raw, ",")
	if len(parts) != 2 {
		return 0, 0, 0, 0, fmt.Errorf("invalid range %q", raw)
	}
	startLine, startColumn, err := parseCoveragePosition(parts[0])
	if err != nil {
		return 0, 0, 0, 0, fmt.Errorf("invalid start position: %w", err)
	}
	endLine, endColumn, err := parseCoveragePosition(parts[1])
	if err != nil {
		return 0, 0, 0, 0, fmt.Errorf("invalid end position: %w", err)
	}
	if compareCoveragePosition(coveragePosition{line: endLine, column: endColumn}, coveragePosition{line: startLine, column: startColumn}) < 0 {
		return 0, 0, 0, 0, fmt.Errorf("end position precedes start position")
	}
	return startLine, startColumn, endLine, endColumn, nil
}

func parseCoveragePosition(raw string) (int, int, error) {
	parts := strings.Split(raw, ".")
	if len(parts) != 2 {
		return 0, 0, fmt.Errorf("expected line.column")
	}
	line, err := strconv.Atoi(parts[0])
	if err != nil || line <= 0 {
		return 0, 0, fmt.Errorf("invalid line %q", parts[0])
	}
	column, err := strconv.Atoi(parts[1])
	if err != nil || column <= 0 {
		return 0, 0, fmt.Errorf("invalid column %q", parts[1])
	}
	return line, column, nil
}

func normalizeCoverageBlock(block CoverageBlock, index int) (CoverageBlock, error) {
	block.File = cleanCoverageFile(block.File)
	if block.File == "" {
		return CoverageBlock{}, invalidCoverageBlock(index, "file is required")
	}
	if block.StartLine <= 0 || block.StartColumn <= 0 || block.EndLine <= 0 || block.EndColumn <= 0 {
		return CoverageBlock{}, invalidCoverageBlock(index, "positions must be positive")
	}
	if compareCoveragePosition(
		coveragePosition{line: block.EndLine, column: block.EndColumn},
		coveragePosition{line: block.StartLine, column: block.StartColumn},
	) < 0 {
		return CoverageBlock{}, invalidCoverageBlock(index, "end position precedes start position")
	}
	if block.Statements <= 0 {
		return CoverageBlock{}, invalidCoverageBlock(index, "statement count must be positive")
	}
	if block.Count < 0 {
		return CoverageBlock{}, invalidCoverageBlock(index, "execution count must be non-negative")
	}
	return block, nil
}

func invalidCoverageBlock(index int, reason string) error {
	return fmt.Errorf("%w: block[%d]: %s", ErrInvalidCoverageProfile, index, reason)
}

func cleanCoverageFile(file string) string {
	file = strings.TrimSpace(file)
	if file == "" {
		return ""
	}
	file = strings.ReplaceAll(file, "\\", "/")
	cleaned := path.Clean(file)
	if cleaned == "." {
		return ""
	}
	return cleaned
}

func coveragePackageForFile(file string) string {
	dir := path.Dir(cleanCoverageFile(file))
	if dir == "" || dir == "." || dir == "/" {
		return "."
	}
	return dir
}

func coverageAddBlock(total *CoverageTotal, block CoverageBlock) {
	total.Statements += block.Statements
	if block.Covered() {
		total.CoveredStatements += block.Statements
	}
}

type coveragePosition struct {
	line   int
	column int
}

type coverageFunctionRange struct {
	summary CoverageFunctionSummary
	start   coveragePosition
	end     coveragePosition
}

func buildCoverageFunctionRanges(blocks []CoverageBlock, options CoverageSummaryOptions) (map[string][]coverageFunctionRange, error) {
	if options.Source == nil {
		return nil, nil
	}

	fileSet := make(map[string]struct{})
	for _, block := range blocks {
		fileSet[block.File] = struct{}{}
	}
	files := make([]string, 0, len(fileSet))
	for file := range fileSet {
		files = append(files, file)
	}
	sort.Strings(files)

	ranges := make(map[string][]coverageFunctionRange, len(files))
	for _, file := range files {
		sourcePath, err := coverageSourcePath(file, options.SourceRoot)
		if err != nil {
			return nil, err
		}
		data, err := fs.ReadFile(options.Source, sourcePath)
		if err != nil {
			return nil, fmt.Errorf("%w: source %q: %v", ErrInvalidCoverageProfile, file, err)
		}
		fileRanges, err := parseCoverageFunctions(sourcePath, file, data)
		if err != nil {
			return nil, err
		}
		ranges[file] = fileRanges
	}
	return ranges, nil
}

func coverageSourcePath(file, root string) (string, error) {
	sourcePath := cleanCoverageFile(file)
	root = cleanCoverageFile(root)
	if root != "" {
		if sourcePath == root {
			return "", fmt.Errorf("%w: source path %q has no file below source root", ErrInvalidCoverageProfile, file)
		}
		if strings.HasPrefix(sourcePath, root+"/") {
			sourcePath = strings.TrimPrefix(sourcePath, root+"/")
		}
	}
	if !fs.ValidPath(sourcePath) {
		return "", fmt.Errorf("%w: source path %q is not valid in fs", ErrInvalidCoverageProfile, sourcePath)
	}
	return sourcePath, nil
}

func parseCoverageFunctions(sourcePath, profileFile string, data []byte) ([]coverageFunctionRange, error) {
	fset := token.NewFileSet()
	parsed, err := parser.ParseFile(fset, sourcePath, data, 0)
	if err != nil {
		return nil, fmt.Errorf("%w: parse source %q: %v", ErrInvalidCoverageProfile, profileFile, err)
	}

	pkg := coveragePackageForFile(profileFile)
	var ranges []coverageFunctionRange
	for _, decl := range parsed.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Body == nil {
			continue
		}
		bodyStart := fset.Position(fn.Body.Pos())
		bodyEnd := fset.Position(fn.Body.End())
		functionStart := fset.Position(fn.Pos())
		ranges = append(ranges, coverageFunctionRange{
			summary: CoverageFunctionSummary{
				Package:   pkg,
				File:      profileFile,
				Name:      coverageFunctionName(fn),
				StartLine: functionStart.Line,
			},
			start: coveragePosition{line: bodyStart.Line, column: bodyStart.Column},
			end:   coveragePosition{line: bodyEnd.Line, column: bodyEnd.Column},
		})
	}
	sort.Slice(ranges, func(i, j int) bool {
		cmp := compareCoveragePosition(ranges[i].start, ranges[j].start)
		if cmp != 0 {
			return cmp < 0
		}
		return ranges[i].summary.Name < ranges[j].summary.Name
	})
	return ranges, nil
}

func coverageFunctionName(fn *ast.FuncDecl) string {
	name := fn.Name.Name
	if fn.Recv == nil || len(fn.Recv.List) == 0 {
		return name
	}
	receiver := coverageReceiverName(fn.Recv.List[0].Type)
	if receiver == "" {
		return name
	}
	return receiver + "." + name
}

func coverageReceiverName(expr ast.Expr) string {
	switch typed := expr.(type) {
	case *ast.Ident:
		return typed.Name
	case *ast.StarExpr:
		name := coverageReceiverName(typed.X)
		if name == "" {
			return ""
		}
		return "(*" + name + ")"
	case *ast.IndexExpr:
		return coverageReceiverName(typed.X)
	case *ast.IndexListExpr:
		return coverageReceiverName(typed.X)
	case *ast.ParenExpr:
		return coverageReceiverName(typed.X)
	case *ast.SelectorExpr:
		return typed.Sel.Name
	default:
		return ""
	}
}

func matchCoverageFunction(ranges []coverageFunctionRange, block CoverageBlock) (coverageFunctionRange, bool) {
	if len(ranges) == 0 {
		return coverageFunctionRange{}, false
	}
	start := coveragePosition{line: block.StartLine, column: block.StartColumn}
	end := coveragePosition{line: block.EndLine, column: block.EndColumn}
	for _, function := range ranges {
		if compareCoveragePosition(function.start, start) <= 0 && compareCoveragePosition(end, function.end) <= 0 {
			return function, true
		}
	}
	return coverageFunctionRange{}, false
}

func compareCoveragePosition(a, b coveragePosition) int {
	if a.line != b.line {
		if a.line < b.line {
			return -1
		}
		return 1
	}
	if a.column != b.column {
		if a.column < b.column {
			return -1
		}
		return 1
	}
	return 0
}

func coverageFunctionFullName(pkg, name string) string {
	name = strings.TrimSpace(name)
	pkg = strings.TrimSpace(pkg)
	if pkg == "" || pkg == "." {
		return name
	}
	if name == "" {
		return pkg
	}
	return pkg + "." + name
}

func coverageFunctionKey(function CoverageFunctionSummary) string {
	return function.Package + "\x00" + function.File + "\x00" + function.Name + "\x00" + strconv.Itoa(function.StartLine)
}

func evaluateCoveragePackageThresholds(packages []CoveragePackageSummary, thresholds map[string]float64) []CoverageThresholdFailure {
	if len(thresholds) == 0 {
		return nil
	}
	byPackage := make(map[string]CoveragePackageSummary, len(packages))
	for _, pkg := range packages {
		byPackage[pkg.Package] = pkg
	}

	var failures []CoverageThresholdFailure
	for _, name := range sortedCoverageThresholdKeys(thresholds) {
		minimum := thresholds[name]
		if !coverageThresholdActive(minimum) {
			continue
		}
		pkg, ok := byPackage[name]
		if !ok {
			failures = append(failures, missingCoverageThresholdFailure(CoverageScopePackage, name, minimum))
			continue
		}
		if coverageBelowThreshold(pkg.Total.Percent(), minimum) {
			failures = append(failures, CoverageThresholdFailure{
				Scope:    CoverageScopePackage,
				Name:     name,
				Minimum:  minimum,
				Actual:   pkg.Total.Percent(),
				Coverage: pkg.Total,
			})
		}
	}
	return failures
}

func evaluateCoverageFileThresholds(files []CoverageFileSummary, thresholds map[string]float64) []CoverageThresholdFailure {
	if len(thresholds) == 0 {
		return nil
	}
	byFile := make(map[string]CoverageFileSummary, len(files))
	for _, file := range files {
		byFile[file.File] = file
	}

	var failures []CoverageThresholdFailure
	for _, name := range sortedCoverageThresholdKeys(thresholds) {
		minimum := thresholds[name]
		if !coverageThresholdActive(minimum) {
			continue
		}
		file, ok := byFile[name]
		if !ok {
			failures = append(failures, missingCoverageThresholdFailure(CoverageScopeFile, name, minimum))
			continue
		}
		if coverageBelowThreshold(file.Total.Percent(), minimum) {
			failures = append(failures, CoverageThresholdFailure{
				Scope:    CoverageScopeFile,
				Name:     name,
				Minimum:  minimum,
				Actual:   file.Total.Percent(),
				Coverage: file.Total,
			})
		}
	}
	return failures
}

func evaluateCoverageFunctionThresholds(functions []CoverageFunctionSummary, thresholds map[string]float64) []CoverageThresholdFailure {
	if len(thresholds) == 0 {
		return nil
	}
	byFunction := make(map[string][]CoverageFunctionSummary, len(functions))
	for _, function := range functions {
		name := function.FullName()
		byFunction[name] = append(byFunction[name], function)
	}

	var failures []CoverageThresholdFailure
	for _, name := range sortedCoverageThresholdKeys(thresholds) {
		minimum := thresholds[name]
		if !coverageThresholdActive(minimum) {
			continue
		}
		matches := byFunction[name]
		if len(matches) == 0 {
			failures = append(failures, missingCoverageThresholdFailure(CoverageScopeFunction, name, minimum))
			continue
		}
		for _, function := range matches {
			if coverageBelowThreshold(function.Total.Percent(), minimum) {
				failures = append(failures, CoverageThresholdFailure{
					Scope:    CoverageScopeFunction,
					Name:     name,
					Minimum:  minimum,
					Actual:   function.Total.Percent(),
					Coverage: function.Total,
				})
			}
		}
	}
	return failures
}

func sortedCoverageThresholdKeys(thresholds map[string]float64) []string {
	keys := make([]string, 0, len(thresholds))
	for key := range thresholds {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func missingCoverageThresholdFailure(scope CoverageScope, name string, minimum float64) CoverageThresholdFailure {
	return CoverageThresholdFailure{
		Scope:   scope,
		Name:    name,
		Minimum: minimum,
		Missing: true,
	}
}

func coverageThresholdActive(minimum float64) bool {
	return minimum > 0
}

func coverageBelowThreshold(actual, minimum float64) bool {
	const epsilon = 1e-9
	return actual+epsilon < minimum
}

func coverageHasThreshold(threshold CoverageThreshold) bool {
	if coverageThresholdActive(threshold.Total) {
		return true
	}
	for _, minimum := range threshold.Packages {
		if coverageThresholdActive(minimum) {
			return true
		}
	}
	for _, minimum := range threshold.Files {
		if coverageThresholdActive(minimum) {
			return true
		}
	}
	for _, minimum := range threshold.Functions {
		if coverageThresholdActive(minimum) {
			return true
		}
	}
	return false
}

func sortCoveragePackages(packages []CoveragePackageSummary) {
	sort.Slice(packages, func(i, j int) bool {
		return packages[i].Package < packages[j].Package
	})
}

func sortCoverageFiles(files []CoverageFileSummary) {
	sort.Slice(files, func(i, j int) bool {
		if files[i].File != files[j].File {
			return files[i].File < files[j].File
		}
		return files[i].Package < files[j].Package
	})
}

func sortCoverageFunctions(functions []CoverageFunctionSummary) {
	sort.Slice(functions, func(i, j int) bool {
		left := functions[i]
		right := functions[j]
		if left.Package != right.Package {
			return left.Package < right.Package
		}
		if left.File != right.File {
			return left.File < right.File
		}
		if left.StartLine != right.StartLine {
			return left.StartLine < right.StartLine
		}
		return left.Name < right.Name
	})
}

func sortCoverageThresholdFailures(failures []CoverageThresholdFailure) {
	sort.Slice(failures, func(i, j int) bool {
		left := failures[i]
		right := failures[j]
		if coverageScopeRank(left.Scope) != coverageScopeRank(right.Scope) {
			return coverageScopeRank(left.Scope) < coverageScopeRank(right.Scope)
		}
		if left.Name != right.Name {
			return left.Name < right.Name
		}
		return left.Minimum < right.Minimum
	})
}

func coverageScopeRank(scope CoverageScope) int {
	switch scope {
	case CoverageScopeTotal:
		return 0
	case CoverageScopePackage:
		return 1
	case CoverageScopeFile:
		return 2
	case CoverageScopeFunction:
		return 3
	default:
		return 4
	}
}

func coverageScopeTitle(scope CoverageScope) string {
	switch scope {
	case CoverageScopeTotal:
		return "Total"
	case CoverageScopePackage:
		return "Package"
	case CoverageScopeFile:
		return "File"
	case CoverageScopeFunction:
		return "Function"
	default:
		return string(scope)
	}
}

func writeCoverageMarkdownTotalRow(b *strings.Builder, name string, total CoverageTotal) {
	b.WriteString("| ")
	b.WriteString(coverageMarkdownCell(name))
	b.WriteString(" | ")
	writeCoverageMarkdownCounts(b, total)
	b.WriteString(" |\n")
}

func writeCoverageMarkdownCounts(b *strings.Builder, total CoverageTotal) {
	b.WriteString(strconv.Itoa(total.CoveredStatements))
	b.WriteString(" | ")
	b.WriteString(strconv.Itoa(total.Statements))
	b.WriteString(" | ")
	b.WriteString(coverageFormatPercent(total.Percent()))
}

func writeCoverageMarkdownThresholds(b *strings.Builder, result CoverageThresholdResult) {
	b.WriteString("## Thresholds\n\n")
	if result.Passed {
		b.WriteString("Passed.\n")
		return
	}
	b.WriteString("Failed.\n\n")
	b.WriteString("| Scope | Name | Actual | Minimum |\n")
	b.WriteString("| --- | --- | ---: | ---: |\n")
	for _, failure := range result.Failures {
		b.WriteString("| ")
		b.WriteString(coverageMarkdownCell(coverageScopeTitle(failure.Scope)))
		b.WriteString(" | ")
		b.WriteString(coverageMarkdownCell(failure.Name))
		b.WriteString(" | ")
		if failure.Missing {
			b.WriteString("missing")
		} else {
			b.WriteString(coverageFormatPercent(failure.Actual))
		}
		b.WriteString(" | ")
		b.WriteString(coverageFormatPercent(failure.Minimum))
		b.WriteString(" |\n")
	}
}

func writeCoverageTextTotal(b *strings.Builder, total CoverageTotal) {
	b.WriteString(strconv.Itoa(total.CoveredStatements))
	b.WriteByte('/')
	b.WriteString(strconv.Itoa(total.Statements))
	b.WriteString(" statements (")
	b.WriteString(coverageFormatPercent(total.Percent()))
	b.WriteByte(')')
}

func writeCoverageTextThresholds(b *strings.Builder, result CoverageThresholdResult) {
	b.WriteString("Thresholds:\n")
	if result.Passed {
		b.WriteString("Passed.\n")
		return
	}
	b.WriteString("Failed.\n")
	for _, failure := range result.Failures {
		b.WriteString("- ")
		if failure.Scope == CoverageScopeTotal {
			b.WriteString("Total")
		} else {
			b.WriteString(coverageScopeTitle(failure.Scope))
			b.WriteByte(' ')
			b.WriteString(coverageTextLine(failure.Name))
		}
		b.WriteString(": ")
		if failure.Missing {
			b.WriteString("missing")
		} else {
			b.WriteString(coverageFormatPercent(failure.Actual))
		}
		b.WriteString(" below ")
		b.WriteString(coverageFormatPercent(failure.Minimum))
		b.WriteByte('\n')
	}
}

func coverageFormatPercent(value float64) string {
	return fmt.Sprintf("%.1f%%", value)
}

func coverageMarkdownText(value string) string {
	return strings.Join(strings.Fields(value), " ")
}

func coverageMarkdownCell(value string) string {
	value = coverageMarkdownText(value)
	value = strings.ReplaceAll(value, "|", `\|`)
	return value
}

func coverageTextLine(value string) string {
	return strings.Join(strings.Fields(value), " ")
}
