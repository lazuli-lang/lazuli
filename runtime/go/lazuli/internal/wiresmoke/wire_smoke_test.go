// Package wiresmoke holds meta-regression tests that scan the
// `runtime/go/lazuli/{notifications,webhooks}/` source for "501 /
// not yet implemented" strings. The Tier 4 + Ship runtime-stubs
// cycle (commit f4d1149) eliminated those stubs; this guard makes
// sure they don't silently reappear in future refactors.
//
// Per the Prove cycle handoff (regression-coverage-2026-05-17 §G6/G7).
package wiresmoke

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// disallowedLiterals are substrings that MUST NOT appear in the
// scanned packages' impl files. The check excludes test files so the
// regression tests themselves can mention the literals.
var disallowedLiterals = []string{
	"StatusNotImplemented",
	"not yet implemented",
}

// scannedRoots are the package directories whose impl files must be
// free of the literals above. Tests + comments are excluded.
var scannedRoots = []string{
	"../../notifications",
	"../../webhooks",
}

func TestNo501ReintroducedInDispatchOrReceive(t *testing.T) {
	t.Parallel()

	for _, root := range scannedRoots {
		root := root
		t.Run(root, func(t *testing.T) {
			t.Parallel()

			err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
				if err != nil {
					return err
				}
				if info.IsDir() {
					return nil
				}
				name := info.Name()
				if !strings.HasSuffix(name, ".go") {
					return nil
				}
				if strings.HasSuffix(name, "_test.go") {
					return nil
				}

				body, readErr := os.ReadFile(path)
				if readErr != nil {
					return readErr
				}
				text := string(body)
				for _, needle := range disallowedLiterals {
					if strings.Contains(text, needle) {
						t.Errorf("%s contains forbidden literal %q — the runtime stub appears to have been reintroduced. "+
							"If the 501/not-yet-implemented path is intentional in this file, add a // wiresmoke:allow comment explaining why and update wire_smoke_test.go to honor it.",
							path, needle)
					}
				}
				return nil
			})
			if err != nil {
				t.Fatalf("walk %s: %v", root, err)
			}
		})
	}
}
