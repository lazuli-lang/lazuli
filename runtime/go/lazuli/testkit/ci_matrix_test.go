package testkit_test

import (
	"errors"
	"reflect"
	"sort"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/testkit"
)

func TestNewCITestMatrixBuildsBalancedDeterministicEntries(t *testing.T) {
	t.Parallel()

	options := testkit.CITestMatrixOptions{
		OperatingSystems: []string{" ubuntu-latest ", "windows-latest", "ubuntu-latest"},
		GoVersions:       []string{"1.25.x", "1.24.x"},
		Packages: []string{
			"./lazuli/jobs",
			" ./lazuli/cache ",
			"./lazuli/auth",
			"./lazuli/storage",
			"./lazuli/admin",
			"./lazuli/cache",
		},
		ShardCount: 2,
	}

	matrix, err := testkit.NewCITestMatrix(options)
	if err != nil {
		t.Fatalf("NewCITestMatrix() error = %v", err)
	}
	reordered, err := testkit.NewCITestMatrix(testkit.CITestMatrixOptions{
		OperatingSystems: []string{" ubuntu-latest ", "windows-latest", "ubuntu-latest"},
		GoVersions:       []string{"1.25.x", "1.24.x"},
		Packages: []string{
			"./lazuli/cache",
			"./lazuli/admin",
			"./lazuli/storage",
			"./lazuli/auth",
			"./lazuli/jobs",
		},
		ShardCount: 2,
	})
	if err != nil {
		t.Fatalf("NewCITestMatrix(reordered) error = %v", err)
	}

	if !reflect.DeepEqual(matrix, reordered) {
		t.Fatalf("NewCITestMatrix() is input-order dependent:\nfirst:  %#v\nsecond: %#v", matrix, reordered)
	}
	if got := strings.Join(matrix.OperatingSystems, ","); got != "ubuntu-latest,windows-latest" {
		t.Fatalf("OperatingSystems = %q, want trimmed/deduplicated order", got)
	}

	entries := matrix.Entries()
	if got, want := len(entries), 8; got != want {
		t.Fatalf("len(Entries()) = %d, want %d", got, want)
	}
	if entries[0].OperatingSystem != "ubuntu-latest" || entries[0].GoVersion != "1.25.x" || entries[0].Shard.Index != 1 {
		t.Fatalf("Entries()[0] = %#v, want first OS, first Go version, first shard", entries[0])
	}

	summary := matrix.PackageShardSummary()
	if summary.TotalShards != 2 || summary.TotalPackages != 5 || summary.MinPackages != 2 || summary.MaxPackages != 3 || summary.Spread != 1 {
		t.Fatalf("PackageShardSummary() = %#v, want balanced 3/2 package split", summary)
	}
	assertPackagesCoveredOnce(t, summary.Shards, []string{
		"./lazuli/admin",
		"./lazuli/auth",
		"./lazuli/cache",
		"./lazuli/jobs",
		"./lazuli/storage",
	})
}

func TestShardPackagesHandlesMoreShardsThanPackages(t *testing.T) {
	t.Parallel()

	shards, err := testkit.ShardPackages([]string{"./lazuli/cache", "./lazuli/auth"}, 4)
	if err != nil {
		t.Fatalf("ShardPackages() error = %v", err)
	}

	summary := testkit.SummarizePackageShards(shards)
	if summary.TotalShards != 4 || summary.TotalPackages != 2 || summary.MinPackages != 0 || summary.MaxPackages != 1 || summary.Spread != 1 {
		t.Fatalf("SummarizePackageShards() = %#v, want four balanced shards with empty tails", summary)
	}
	for i, shard := range shards {
		if shard.Index != i+1 || shard.Total != 4 {
			t.Fatalf("shards[%d] = %#v, want one-based index and total", i, shard)
		}
		if shard.Hash == "" {
			t.Fatalf("shards[%d].Hash is empty", i)
		}
	}
}

func TestStablePackageHashTrimsInput(t *testing.T) {
	t.Parallel()

	const want = "56f7d956f19004bfc279b30dc8bf732214e7bc2f5fd6f7eb3fe4d8922cd287cb"
	if got := testkit.StablePackageHash(" ./lazuli/cache "); got != want {
		t.Fatalf("StablePackageHash() = %q, want %q", got, want)
	}
}

func TestCITestMatrixMetadataIsDeterministicYAMLish(t *testing.T) {
	t.Parallel()

	matrix, err := testkit.NewCITestMatrix(testkit.CITestMatrixOptions{
		OperatingSystems: []string{"ubuntu-latest"},
		GoVersions:       []string{"1.25.x"},
		Packages:         []string{"./lazuli/cache", "./lazuli/auth"},
		ShardCount:       2,
	})
	if err != nil {
		t.Fatalf("NewCITestMatrix() error = %v", err)
	}

	first := matrix.Metadata()
	second := testkit.RenderCITestMatrixMetadata(matrix)
	if first != second {
		t.Fatalf("metadata render is not deterministic:\nfirst:\n%s\nsecond:\n%s", first, second)
	}
	for _, fragment := range []string{
		"ci_test_matrix:\n",
		"  operating_systems:\n    - \"ubuntu-latest\"\n",
		"  go_versions:\n    - \"1.25.x\"\n",
		"  entry_count: 2\n",
		"  package_count: 2\n",
		"  package_shard_balance:\n    total_shards: 2\n    min_packages: 1\n    max_packages: 1\n    spread: 0\n",
		"  package_shards:\n",
		"  entries:\n",
		"      shard: \"1/2\"\n",
		"      packages:\n        - \"./lazuli/",
	} {
		if !strings.Contains(first, fragment) {
			t.Fatalf("metadata does not contain %q:\n%s", fragment, first)
		}
	}
}

func TestNewCITestMatrixRejectsInvalidOptions(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		options testkit.CITestMatrixOptions
	}{
		{
			name: "empty os",
			options: testkit.CITestMatrixOptions{
				OperatingSystems: []string{""},
				GoVersions:       []string{"1.25.x"},
				Packages:         []string{"./lazuli/cache"},
			},
		},
		{
			name: "control go version",
			options: testkit.CITestMatrixOptions{
				OperatingSystems: []string{"ubuntu-latest"},
				GoVersions:       []string{"1.25\nx"},
				Packages:         []string{"./lazuli/cache"},
			},
		},
		{
			name: "negative shard count",
			options: testkit.CITestMatrixOptions{
				OperatingSystems: []string{"ubuntu-latest"},
				GoVersions:       []string{"1.25.x"},
				Packages:         []string{"./lazuli/cache"},
				ShardCount:       -1,
			},
		},
		{
			name: "empty package",
			options: testkit.CITestMatrixOptions{
				OperatingSystems: []string{"ubuntu-latest"},
				GoVersions:       []string{"1.25.x"},
				Packages:         []string{"./lazuli/cache", " "},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			_, err := testkit.NewCITestMatrix(tt.options)
			if !errors.Is(err, testkit.ErrInvalidCITestMatrix) {
				t.Fatalf("NewCITestMatrix() error = %v, want ErrInvalidCITestMatrix", err)
			}
		})
	}
}

func assertPackagesCoveredOnce(t *testing.T, shards []testkit.PackageShard, want []string) {
	t.Helper()

	var got []string
	for _, shard := range shards {
		got = append(got, shard.Packages...)
	}
	sort.Strings(got)
	sort.Strings(want)
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("sharded packages = %#v, want %#v", got, want)
	}
}
