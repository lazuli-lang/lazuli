package security

import (
	"errors"
	"strings"
	"testing"
)

func TestSBOMManifestSPDXJSONDeterministic(t *testing.T) {
	t.Parallel()

	sha256A := strings.Repeat("a", 64)
	sha256C := strings.Repeat("c", 64)
	sha512B := strings.Repeat("b", 128)
	manifest := SBOMManifest{
		Name:              "demo",
		DocumentNamespace: "https://example.test/sbom/demo",
		Created:           "2026-05-12T10:00:00-03:00",
		Creators:          []string{"Tool: Lazuli", "Organization: Example"},
		Components: []SBOMComponent{
			{
				Name:            "zeta",
				Version:         "2.0.0",
				PackageURL:      "pkg:generic/zeta@2.0.0",
				LicenseDeclared: "Apache-2.0",
				Hashes: []SBOMHash{
					{Algorithm: "sha-512", Value: sha512B},
					{Algorithm: "sha256", Value: sha256A},
				},
			},
			{
				Name:             "alpha",
				Version:          "1.0.0",
				Supplier:         "Organization: Example",
				LicenseConcluded: "MIT",
				LicenseDeclared:  "MIT",
				Hashes: []SBOMHash{
					{Algorithm: "SHA256", Value: sha256C},
				},
			},
		},
	}

	got, err := manifest.SPDXJSON()
	if err != nil {
		t.Fatalf("SPDXJSON() error = %v", err)
	}

	reversed := manifest
	reversed.Components = []SBOMComponent{manifest.Components[1], manifest.Components[0]}
	gotReversed, err := reversed.SPDXJSON()
	if err != nil {
		t.Fatalf("SPDXJSON(reversed) error = %v", err)
	}
	if string(gotReversed) != string(got) {
		t.Fatalf("SPDXJSON() is not deterministic across component order\nfirst:\n%s\nsecond:\n%s", got, gotReversed)
	}

	want := `{
  "spdxVersion": "SPDX-2.3",
  "dataLicense": "CC0-1.0",
  "SPDXID": "SPDXRef-DOCUMENT",
  "name": "demo",
  "documentNamespace": "https://example.test/sbom/demo",
  "creationInfo": {
    "created": "2026-05-12T13:00:00Z",
    "creators": [
      "Organization: Example",
      "Tool: Lazuli"
    ]
  },
  "packages": [
    {
      "SPDXID": "SPDXRef-Package-alpha-1.0.0",
      "name": "alpha",
      "versionInfo": "1.0.0",
      "supplier": "Organization: Example",
      "downloadLocation": "NOASSERTION",
      "filesAnalyzed": false,
      "licenseConcluded": "MIT",
      "licenseDeclared": "MIT",
      "copyrightText": "NOASSERTION",
      "checksums": [
        {
          "algorithm": "SHA256",
          "checksumValue": "` + sha256C + `"
        }
      ]
    },
    {
      "SPDXID": "SPDXRef-Package-zeta-2.0.0",
      "name": "zeta",
      "versionInfo": "2.0.0",
      "downloadLocation": "NOASSERTION",
      "filesAnalyzed": false,
      "licenseConcluded": "NOASSERTION",
      "licenseDeclared": "Apache-2.0",
      "copyrightText": "NOASSERTION",
      "checksums": [
        {
          "algorithm": "SHA256",
          "checksumValue": "` + sha256A + `"
        },
        {
          "algorithm": "SHA512",
          "checksumValue": "` + sha512B + `"
        }
      ],
      "externalRefs": [
        {
          "referenceCategory": "PACKAGE-MANAGER",
          "referenceType": "purl",
          "referenceLocator": "pkg:generic/zeta@2.0.0"
        }
      ]
    }
  ],
  "relationships": [
    {
      "spdxElementId": "SPDXRef-DOCUMENT",
      "relationshipType": "DESCRIBES",
      "relatedSpdxElement": "SPDXRef-Package-alpha-1.0.0"
    },
    {
      "spdxElementId": "SPDXRef-DOCUMENT",
      "relationshipType": "DESCRIBES",
      "relatedSpdxElement": "SPDXRef-Package-zeta-2.0.0"
    }
  ]
}`
	if string(got) != want {
		t.Fatalf("SPDXJSON() =\n%s\nwant:\n%s", got, want)
	}
}

func TestSBOMManifestPackagesReturnsNormalizedCopies(t *testing.T) {
	t.Parallel()

	manifest := validSBOMManifest()
	manifest.Components[0].Hashes[0].Algorithm = "sha-256"
	manifest.Components[0].Hashes[0].Value = strings.ToUpper(manifest.Components[0].Hashes[0].Value)

	packages, err := manifest.Packages()
	if err != nil {
		t.Fatalf("Packages() error = %v", err)
	}
	if got := packages[0].Checksums[0].Algorithm; got != "SHA256" {
		t.Fatalf("Packages()[0].Checksums[0].Algorithm = %q, want SHA256", got)
	}
	if got := packages[0].Checksums[0].Value; got != strings.Repeat("a", 64) {
		t.Fatalf("Packages()[0].Checksums[0].Value = %q, want normalized lowercase hash", got)
	}

	packages[0].Checksums[0].Value = strings.Repeat("b", 64)
	again, err := manifest.Packages()
	if err != nil {
		t.Fatalf("Packages() again error = %v", err)
	}
	if got := again[0].Checksums[0].Value; got != strings.Repeat("a", 64) {
		t.Fatalf("Packages() returned mutable checksum state = %q", got)
	}
}

func TestSBOMManifestValidateRejectsInvalidInput(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		mutate func(*SBOMManifest)
	}{
		{
			name: "missing manifest name",
			mutate: func(manifest *SBOMManifest) {
				manifest.Name = ""
			},
		},
		{
			name: "invalid created time",
			mutate: func(manifest *SBOMManifest) {
				manifest.Created = "2026-05-12"
			},
		},
		{
			name: "duplicate generated ids",
			mutate: func(manifest *SBOMManifest) {
				manifest.Components = append(manifest.Components, manifest.Components[0])
			},
		},
		{
			name: "invalid package id",
			mutate: func(manifest *SBOMManifest) {
				manifest.Components[0].ID = "SPDXRef-Package_app"
			},
		},
		{
			name: "padded license",
			mutate: func(manifest *SBOMManifest) {
				manifest.Components[0].LicenseDeclared = " MIT "
			},
		},
		{
			name: "invalid hash value",
			mutate: func(manifest *SBOMManifest) {
				manifest.Components[0].Hashes[0].Value = "not-hex"
			},
		},
		{
			name: "duplicate hash algorithm",
			mutate: func(manifest *SBOMManifest) {
				manifest.Components[0].Hashes = append(manifest.Components[0].Hashes, SBOMHash{
					Algorithm: "SHA-256",
					Value:     strings.Repeat("b", 64),
				})
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			manifest := validSBOMManifest()
			tt.mutate(&manifest)

			if err := manifest.Validate(); !errors.Is(err, ErrInvalidSBOM) {
				t.Fatalf("Validate() error = %v, want ErrInvalidSBOM", err)
			}
			if _, err := manifest.SPDXJSON(); !errors.Is(err, ErrInvalidSBOM) {
				t.Fatalf("SPDXJSON() error = %v, want ErrInvalidSBOM", err)
			}
		})
	}
}

func validSBOMManifest() SBOMManifest {
	return SBOMManifest{
		Name:              "demo",
		DocumentNamespace: "https://example.test/sbom/demo",
		Created:           "2026-05-12T13:00:00Z",
		Components: []SBOMComponent{
			{
				Name:            "app",
				Version:         "1.0.0",
				LicenseDeclared: "MIT",
				Hashes: []SBOMHash{
					{Algorithm: "SHA256", Value: strings.Repeat("a", 64)},
				},
			},
		},
	}
}
