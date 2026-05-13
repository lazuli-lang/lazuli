package testkit

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"unicode"
)

// ErrInvalidCITestMatrix is returned when a CI test matrix or package shard
// request cannot be normalized safely.
var ErrInvalidCITestMatrix = errors.New("testkit: invalid ci test matrix")

// CITestMatrixOptions configures a provider-neutral test matrix.
type CITestMatrixOptions struct {
	// OperatingSystems names the OS axis values, such as "ubuntu-latest" or
	// "windows-latest". Values are trimmed and de-duplicated in input order.
	OperatingSystems []string

	// GoVersions names the Go version axis values. Values are trimmed and
	// de-duplicated in input order.
	GoVersions []string

	// Packages is the package list to split into deterministic shards. Values
	// are trimmed, de-duplicated, and balanced independently of input order.
	Packages []string

	// ShardCount is the requested number of package shards. Zero means one.
	ShardCount int
}

// CITestMatrix is a provider-neutral OS x Go version x package-shard matrix.
type CITestMatrix struct {
	OperatingSystems []string
	GoVersions       []string
	PackageShards    []PackageShard
}

// CITestMatrixEntry is one OS, Go version, and package shard combination.
type CITestMatrixEntry struct {
	OperatingSystem string
	GoVersion       string
	Shard           PackageShard
}

// PackageShard is a deterministic subset of packages for one test shard.
type PackageShard struct {
	// Index is one-based.
	Index int

	// Total is the total number of package shards in the set.
	Total int

	// Packages is the sorted package list assigned to this shard.
	Packages []string

	// Hash is a stable SHA-256 digest of the shard identity and packages.
	Hash string
}

// PackageShardSummary describes the balance of a shard set.
type PackageShardSummary struct {
	TotalShards   int
	TotalPackages int
	MinPackages   int
	MaxPackages   int
	Spread        int
	Shards        []PackageShard
}

type ciHashedPackage struct {
	name string
	hash string
}

// NewCITestMatrix returns a normalized CI test matrix.
func NewCITestMatrix(options CITestMatrixOptions) (CITestMatrix, error) {
	operatingSystems, err := normalizeCIAxis("operating system", options.OperatingSystems)
	if err != nil {
		return CITestMatrix{}, err
	}
	goVersions, err := normalizeCIAxis("go version", options.GoVersions)
	if err != nil {
		return CITestMatrix{}, err
	}
	shards, err := ShardPackages(options.Packages, options.ShardCount)
	if err != nil {
		return CITestMatrix{}, err
	}

	return CITestMatrix{
		OperatingSystems: cloneCIStrings(operatingSystems),
		GoVersions:       cloneCIStrings(goVersions),
		PackageShards:    clonePackageShards(shards),
	}, nil
}

// ShardPackages splits packages into deterministic, count-balanced shards.
func ShardPackages(packages []string, shardCount int) ([]PackageShard, error) {
	if shardCount == 0 {
		shardCount = 1
	}
	if shardCount < 0 {
		return nil, fmt.Errorf("%w: shard count must be non-negative", ErrInvalidCITestMatrix)
	}

	normalized, err := normalizeCIPackages(packages)
	if err != nil {
		return nil, err
	}

	hashed := make([]ciHashedPackage, 0, len(normalized))
	for _, pkg := range normalized {
		sum := sha256.Sum256([]byte(pkg))
		hashed = append(hashed, ciHashedPackage{
			name: pkg,
			hash: hex.EncodeToString(sum[:]),
		})
	}
	sort.Slice(hashed, func(i, j int) bool {
		cmp := strings.Compare(hashed[i].hash, hashed[j].hash)
		if cmp == 0 {
			return hashed[i].name < hashed[j].name
		}
		return cmp < 0
	})

	shards := make([]PackageShard, shardCount)
	for i := range shards {
		shards[i] = PackageShard{Index: i + 1, Total: shardCount}
	}
	for i, pkg := range hashed {
		shardIndex := i % shardCount
		shards[shardIndex].Packages = append(shards[shardIndex].Packages, pkg.name)
	}
	for i := range shards {
		sort.Strings(shards[i].Packages)
		shards[i].Hash = hashPackageShard(shards[i])
	}

	return shards, nil
}

// StablePackageHash returns the SHA-256 digest used to order package shards.
func StablePackageHash(pkg string) string {
	pkg = strings.TrimSpace(pkg)
	sum := sha256.Sum256([]byte(pkg))
	return hex.EncodeToString(sum[:])
}

// Entries returns the matrix cross-product in deterministic axis order.
func (m CITestMatrix) Entries() []CITestMatrixEntry {
	if len(m.OperatingSystems) == 0 || len(m.GoVersions) == 0 || len(m.PackageShards) == 0 {
		return nil
	}

	entries := make([]CITestMatrixEntry, 0, len(m.OperatingSystems)*len(m.GoVersions)*len(m.PackageShards))
	for _, os := range m.OperatingSystems {
		for _, version := range m.GoVersions {
			for _, shard := range m.PackageShards {
				entries = append(entries, CITestMatrixEntry{
					OperatingSystem: os,
					GoVersion:       version,
					Shard:           clonePackageShard(shard),
				})
			}
		}
	}
	return entries
}

// PackageShardSummary returns package counts and shard metadata for matrix
// package shards.
func (m CITestMatrix) PackageShardSummary() PackageShardSummary {
	return SummarizePackageShards(m.PackageShards)
}

// SummarizePackageShards returns package counts and a cloned shard set.
func SummarizePackageShards(shards []PackageShard) PackageShardSummary {
	summary := PackageShardSummary{
		TotalShards: len(shards),
		Shards:      clonePackageShards(shards),
	}
	if len(shards) == 0 {
		return summary
	}

	summary.MinPackages = len(shards[0].Packages)
	for _, shard := range shards {
		count := len(shard.Packages)
		summary.TotalPackages += count
		if count < summary.MinPackages {
			summary.MinPackages = count
		}
		if count > summary.MaxPackages {
			summary.MaxPackages = count
		}
	}
	summary.Spread = summary.MaxPackages - summary.MinPackages
	return summary
}

// Metadata renders the matrix as deterministic YAML-ish metadata. It does not
// emit provider-specific CI syntax or write any files.
func (m CITestMatrix) Metadata() string {
	return RenderCITestMatrixMetadata(m)
}

// RenderCITestMatrixMetadata renders deterministic YAML-ish metadata for a
// provider-neutral CI test matrix.
func RenderCITestMatrixMetadata(matrix CITestMatrix) string {
	var b strings.Builder
	summary := matrix.PackageShardSummary()
	entries := matrix.Entries()

	b.WriteString("ci_test_matrix:\n")
	writeCIMetadataStringList(&b, 2, "operating_systems", matrix.OperatingSystems)
	writeCIMetadataStringList(&b, 2, "go_versions", matrix.GoVersions)
	writeCIMetadataInt(&b, 2, "entry_count", len(entries))
	writeCIMetadataInt(&b, 2, "package_count", summary.TotalPackages)
	b.WriteString("  package_shard_balance:\n")
	writeCIMetadataInt(&b, 4, "total_shards", summary.TotalShards)
	writeCIMetadataInt(&b, 4, "min_packages", summary.MinPackages)
	writeCIMetadataInt(&b, 4, "max_packages", summary.MaxPackages)
	writeCIMetadataInt(&b, 4, "spread", summary.Spread)
	writeCIMetadataShards(&b, 2, "package_shards", matrix.PackageShards)
	writeCIMetadataEntries(&b, entries)
	return b.String()
}

func normalizeCIAxis(name string, values []string) ([]string, error) {
	normalized := make([]string, 0, len(values))
	seen := make(map[string]struct{}, len(values))
	for i, value := range values {
		clean := strings.TrimSpace(value)
		if clean == "" {
			return nil, fmt.Errorf("%w: %s %d is empty", ErrInvalidCITestMatrix, name, i)
		}
		if containsControlRune(clean) {
			return nil, fmt.Errorf("%w: %s %q contains control characters", ErrInvalidCITestMatrix, name, clean)
		}
		if _, ok := seen[clean]; ok {
			continue
		}
		seen[clean] = struct{}{}
		normalized = append(normalized, clean)
	}
	if len(normalized) == 0 {
		return nil, fmt.Errorf("%w: at least one %s is required", ErrInvalidCITestMatrix, name)
	}
	return normalized, nil
}

func normalizeCIPackages(packages []string) ([]string, error) {
	normalized := make([]string, 0, len(packages))
	seen := make(map[string]struct{}, len(packages))
	for i, pkg := range packages {
		clean := strings.TrimSpace(pkg)
		if clean == "" {
			return nil, fmt.Errorf("%w: package %d is empty", ErrInvalidCITestMatrix, i)
		}
		if containsControlRune(clean) {
			return nil, fmt.Errorf("%w: package %q contains control characters", ErrInvalidCITestMatrix, clean)
		}
		if _, ok := seen[clean]; ok {
			continue
		}
		seen[clean] = struct{}{}
		normalized = append(normalized, clean)
	}
	if len(normalized) == 0 {
		return nil, fmt.Errorf("%w: at least one package is required", ErrInvalidCITestMatrix)
	}
	return normalized, nil
}

func containsControlRune(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func hashPackageShard(shard PackageShard) string {
	h := sha256.New()
	_, _ = fmt.Fprintf(h, "shard:%d/%d\n", shard.Index, shard.Total)
	for _, pkg := range shard.Packages {
		_, _ = fmt.Fprintf(h, "%d:%s\n", len(pkg), pkg)
	}
	return hex.EncodeToString(h.Sum(nil))
}

func cloneCIStrings(values []string) []string {
	return append([]string(nil), values...)
}

func clonePackageShard(shard PackageShard) PackageShard {
	shard.Packages = cloneCIStrings(shard.Packages)
	return shard
}

func clonePackageShards(shards []PackageShard) []PackageShard {
	if len(shards) == 0 {
		return nil
	}
	out := make([]PackageShard, len(shards))
	for i, shard := range shards {
		out[i] = clonePackageShard(shard)
	}
	return out
}

func writeCIMetadataStringList(b *strings.Builder, indent int, key string, values []string) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	if len(values) == 0 {
		b.WriteString(": []\n")
		return
	}
	b.WriteString(":\n")
	for _, value := range values {
		b.WriteString(strings.Repeat(" ", indent+2))
		b.WriteString("- ")
		b.WriteString(quoteCIMetadataValue(value))
		b.WriteByte('\n')
	}
}

func writeCIMetadataInt(b *strings.Builder, indent int, key string, value int) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(": ")
	b.WriteString(strconv.Itoa(value))
	b.WriteByte('\n')
}

func writeCIMetadataScalar(b *strings.Builder, indent int, key, value string) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(": ")
	b.WriteString(quoteCIMetadataValue(value))
	b.WriteByte('\n')
}

func writeCIMetadataShards(b *strings.Builder, indent int, key string, shards []PackageShard) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	if len(shards) == 0 {
		b.WriteString(": []\n")
		return
	}
	b.WriteString(":\n")
	for _, shard := range shards {
		b.WriteString(strings.Repeat(" ", indent+2))
		b.WriteString("- index: ")
		b.WriteString(strconv.Itoa(shard.Index))
		b.WriteByte('\n')
		writeCIMetadataInt(b, indent+4, "total", shard.Total)
		writeCIMetadataInt(b, indent+4, "package_count", len(shard.Packages))
		writeCIMetadataScalar(b, indent+4, "hash", shard.Hash)
		writeCIMetadataStringList(b, indent+4, "packages", shard.Packages)
	}
}

func writeCIMetadataEntries(b *strings.Builder, entries []CITestMatrixEntry) {
	if len(entries) == 0 {
		b.WriteString("  entries: []\n")
		return
	}
	b.WriteString("  entries:\n")
	for _, entry := range entries {
		b.WriteString("    - os: ")
		b.WriteString(quoteCIMetadataValue(entry.OperatingSystem))
		b.WriteByte('\n')
		writeCIMetadataScalar(b, 6, "go", entry.GoVersion)
		writeCIMetadataScalar(b, 6, "shard", fmt.Sprintf("%d/%d", entry.Shard.Index, entry.Shard.Total))
		writeCIMetadataScalar(b, 6, "shard_hash", entry.Shard.Hash)
		writeCIMetadataStringList(b, 6, "packages", entry.Shard.Packages)
	}
}

func quoteCIMetadataValue(value string) string {
	return strconv.Quote(value)
}
