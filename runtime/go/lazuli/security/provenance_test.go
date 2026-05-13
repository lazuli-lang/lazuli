package security

import (
	"errors"
	"math"
	"strings"
	"testing"
)

func TestProvenanceStatementSLSAJSONDeterministic(t *testing.T) {
	t.Parallel()

	sha256A := strings.Repeat("a", 64)
	sha256B := strings.Repeat("b", 64)
	gitCommit := strings.Repeat("c", 40)

	predicate := NewProvenancePredicate("https://lazuli.dev/build/go/v1", "https://lazuli.dev/builders/go")
	predicate.BuildDefinition.ExternalParameters = map[string]any{
		"repository": "https://example.test/lazuli",
		"ref":        "refs/heads/main",
	}
	predicate.BuildDefinition.InternalParameters = map[string]any{
		"goVersion": "1.25.0",
	}
	predicate.BuildDefinition.ResolvedDependencies = []ProvenanceResource{
		ProvenanceMaterialSource("pkg:generic/lazuli/runtime-go@1.0.0", "SHA-256", strings.ToUpper(sha256B)),
		ProvenanceMaterialSource("git+https://example.test/lazuli@refs/heads/main", "git_commit", strings.ToUpper(gitCommit)),
	}
	predicate.RunDetails.Builder.Version = map[string]string{
		"lazuli": "0.3.4",
		"go":     "1.25.0",
	}
	predicate.RunDetails.Metadata = &ProvenanceBuildMetadata{
		InvocationID: "build-123",
		StartedOn:    "2026-05-12T10:00:00-03:00",
		FinishedOn:   "2026-05-12T10:03:00-03:00",
	}

	statement := NewProvenanceStatement([]ProvenanceResource{
		ProvenanceSubjectDigest("zeta.tar.gz", "sha-256", strings.ToUpper(sha256B)),
		ProvenanceSubjectDigest("alpha.tar.gz", "SHA256", strings.ToUpper(sha256A)),
	}, predicate)

	got, err := statement.SLSAJSON()
	if err != nil {
		t.Fatalf("SLSAJSON() error = %v", err)
	}

	reversed := statement
	reversed.Subject = []ProvenanceResource{statement.Subject[1], statement.Subject[0]}
	reversed.Predicate.BuildDefinition.ResolvedDependencies = []ProvenanceResource{
		statement.Predicate.BuildDefinition.ResolvedDependencies[1],
		statement.Predicate.BuildDefinition.ResolvedDependencies[0],
	}
	gotReversed, err := reversed.SLSAJSON()
	if err != nil {
		t.Fatalf("SLSAJSON(reversed) error = %v", err)
	}
	if string(gotReversed) != string(got) {
		t.Fatalf("SLSAJSON() is not deterministic across resource order\nfirst:\n%s\nsecond:\n%s", got, gotReversed)
	}

	want := `{
  "_type": "https://in-toto.io/Statement/v1",
  "subject": [
    {
      "digest": {
        "sha256": "` + sha256A + `"
      },
      "name": "alpha.tar.gz"
    },
    {
      "digest": {
        "sha256": "` + sha256B + `"
      },
      "name": "zeta.tar.gz"
    }
  ],
  "predicateType": "https://slsa.dev/provenance/v1",
  "predicate": {
    "buildDefinition": {
      "buildType": "https://lazuli.dev/build/go/v1",
      "externalParameters": {
        "ref": "refs/heads/main",
        "repository": "https://example.test/lazuli"
      },
      "internalParameters": {
        "goVersion": "1.25.0"
      },
      "resolvedDependencies": [
        {
          "uri": "git+https://example.test/lazuli@refs/heads/main",
          "digest": {
            "gitCommit": "` + gitCommit + `"
          }
        },
        {
          "uri": "pkg:generic/lazuli/runtime-go@1.0.0",
          "digest": {
            "sha256": "` + sha256B + `"
          }
        }
      ]
    },
    "runDetails": {
      "builder": {
        "id": "https://lazuli.dev/builders/go",
        "version": {
          "go": "1.25.0",
          "lazuli": "0.3.4"
        }
      },
      "metadata": {
        "invocationId": "build-123",
        "startedOn": "2026-05-12T13:00:00Z",
        "finishedOn": "2026-05-12T13:03:00Z"
      }
    }
  }
}`
	if string(got) != want {
		t.Fatalf("SLSAJSON() =\n%s\nwant:\n%s", got, want)
	}
}

func TestProvenanceHelpersCopyDigestInputs(t *testing.T) {
	t.Parallel()

	digests := map[string]string{"SHA-256": strings.Repeat("a", 64)}
	subject := ProvenanceSubject("app", digests)
	digests["SHA-256"] = "not-hex"

	statement := NewProvenanceStatement([]ProvenanceResource{subject}, validProvenancePredicate())
	if err := statement.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	statement.Subject[0].Digest["SHA-256"] = "not-hex"
	if err := subjectDigestStatement(subject).Validate(); err != nil {
		t.Fatalf("copied subject Validate() error = %v", err)
	}
}

func TestProvenanceValidateRejectsInvalidInput(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		mutate func(*ProvenanceStatement)
	}{
		{
			name: "missing subject",
			mutate: func(statement *ProvenanceStatement) {
				statement.Subject = nil
			},
		},
		{
			name: "invalid subject digest",
			mutate: func(statement *ProvenanceStatement) {
				statement.Subject[0].Digest["sha256"] = "not-hex"
			},
		},
		{
			name: "invalid builder id",
			mutate: func(statement *ProvenanceStatement) {
				statement.Predicate.RunDetails.Builder.ID = "lazuli-builder"
			},
		},
		{
			name: "missing build type",
			mutate: func(statement *ProvenanceStatement) {
				statement.Predicate.BuildDefinition.BuildType = ""
			},
		},
		{
			name: "empty material",
			mutate: func(statement *ProvenanceStatement) {
				statement.Predicate.BuildDefinition.ResolvedDependencies = []ProvenanceResource{{}}
			},
		},
		{
			name: "metadata finishes before start",
			mutate: func(statement *ProvenanceStatement) {
				statement.Predicate.RunDetails.Metadata = &ProvenanceBuildMetadata{
					StartedOn:  "2026-05-12T10:00:00Z",
					FinishedOn: "2026-05-12T09:59:59Z",
				}
			},
		},
		{
			name: "non finite parameter",
			mutate: func(statement *ProvenanceStatement) {
				statement.Predicate.BuildDefinition.ExternalParameters["bad"] = math.Inf(1)
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			statement := validProvenanceStatement()
			tt.mutate(&statement)

			if err := statement.Validate(); !errors.Is(err, ErrInvalidProvenance) {
				t.Fatalf("Validate() error = %v, want ErrInvalidProvenance", err)
			}
			if _, err := statement.SLSAJSON(); !errors.Is(err, ErrInvalidProvenance) {
				t.Fatalf("SLSAJSON() error = %v, want ErrInvalidProvenance", err)
			}
		})
	}
}

func TestValidateProvenancePredicate(t *testing.T) {
	t.Parallel()

	predicate := validProvenancePredicate()
	if err := ValidateProvenancePredicate(predicate); err != nil {
		t.Fatalf("ValidateProvenancePredicate() error = %v", err)
	}

	predicate.BuildDefinition.ResolvedDependencies = []ProvenanceResource{
		ProvenanceMaterialSource("git+https://example.test/lazuli", "gitCommit", strings.Repeat("g", 40)),
	}
	if err := predicate.Validate(); !errors.Is(err, ErrInvalidProvenance) {
		t.Fatalf("predicate.Validate() error = %v, want ErrInvalidProvenance", err)
	}
}

func validProvenanceStatement() ProvenanceStatement {
	return NewProvenanceStatement([]ProvenanceResource{
		ProvenanceSubjectDigest("app", "sha256", strings.Repeat("a", 64)),
	}, validProvenancePredicate())
}

func validProvenancePredicate() ProvenancePredicate {
	return NewProvenancePredicate("https://lazuli.dev/build/go/v1", "https://lazuli.dev/builders/go")
}

func subjectDigestStatement(subject ProvenanceResource) ProvenanceStatement {
	return NewProvenanceStatement([]ProvenanceResource{subject}, validProvenancePredicate())
}
