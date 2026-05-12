package security

import (
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"
	"unicode"
)

const (
	spdxVersion     = "SPDX-2.3"
	spdxDataLicense = "CC0-1.0"
	spdxDocumentID  = "SPDXRef-DOCUMENT"
)

// ErrInvalidSBOM is returned when an SBOM manifest cannot be normalized or
// rendered as SPDX-ish JSON.
var ErrInvalidSBOM = errors.New("lazuli/security: invalid_sbom")

// SBOMManifest describes generated application components in a provider-neutral
// shape that can be rendered as deterministic SPDX-ish JSON.
type SBOMManifest struct {
	Name              string
	DocumentNamespace string
	Created           string
	Creators          []string
	Components        []SBOMComponent
}

// SBOMComponent describes one generated or bundled package/component in an
// SBOM manifest. Empty license and location fields render as NOASSERTION.
type SBOMComponent struct {
	ID               string
	Name             string
	Version          string
	Supplier         string
	DownloadLocation string
	PackageURL       string
	LicenseConcluded string
	LicenseDeclared  string
	CopyrightText    string
	Hashes           []SBOMHash
}

// SBOMHash records a package checksum. Algorithm is normalized to SPDX-style
// names such as SHA256 before rendering.
type SBOMHash struct {
	Algorithm string `json:"algorithm"`
	Value     string `json:"checksumValue"`
}

// SBOMPackage is the SPDX-ish package shape emitted for a component.
type SBOMPackage struct {
	SPDXID           string            `json:"SPDXID"`
	Name             string            `json:"name"`
	VersionInfo      string            `json:"versionInfo,omitempty"`
	Supplier         string            `json:"supplier,omitempty"`
	DownloadLocation string            `json:"downloadLocation"`
	FilesAnalyzed    bool              `json:"filesAnalyzed"`
	LicenseConcluded string            `json:"licenseConcluded"`
	LicenseDeclared  string            `json:"licenseDeclared"`
	CopyrightText    string            `json:"copyrightText"`
	Checksums        []SBOMHash        `json:"checksums,omitempty"`
	ExternalRefs     []SBOMExternalRef `json:"externalRefs,omitempty"`
}

// SBOMExternalRef is an SPDX external package reference. PackageURL fields on
// components render as PACKAGE-MANAGER purl references.
type SBOMExternalRef struct {
	ReferenceCategory string `json:"referenceCategory"`
	ReferenceType     string `json:"referenceType"`
	ReferenceLocator  string `json:"referenceLocator"`
}

// Validate checks that manifest can be normalized and rendered.
func (m SBOMManifest) Validate() error {
	_, err := normalizeSBOMManifest(m)
	return err
}

// Packages returns the normalized SPDX-ish package entries for manifest.
func (m SBOMManifest) Packages() ([]SBOMPackage, error) {
	normalized, err := normalizeSBOMManifest(m)
	if err != nil {
		return nil, err
	}
	return cloneSBOMPackages(normalized.packages), nil
}

// SPDXJSON renders manifest as deterministic, indented SPDX-ish JSON.
func (m SBOMManifest) SPDXJSON() ([]byte, error) {
	return RenderSPDXJSON(m)
}

// RenderSPDXJSON renders manifest as deterministic, indented SPDX-ish JSON.
func RenderSPDXJSON(manifest SBOMManifest) ([]byte, error) {
	normalized, err := normalizeSBOMManifest(manifest)
	if err != nil {
		return nil, err
	}
	return json.MarshalIndent(normalized.document(), "", "  ")
}

type normalizedSBOMManifest struct {
	name              string
	documentNamespace string
	created           string
	creators          []string
	packages          []SBOMPackage
}

type spdxCreationInfo struct {
	Created  string   `json:"created"`
	Creators []string `json:"creators"`
}

type spdxRelationship struct {
	SPDXElementID      string `json:"spdxElementId"`
	RelationshipType   string `json:"relationshipType"`
	RelatedSPDXElement string `json:"relatedSpdxElement"`
}

type spdxDocument struct {
	SPDXVersion       string             `json:"spdxVersion"`
	DataLicense       string             `json:"dataLicense"`
	SPDXID            string             `json:"SPDXID"`
	Name              string             `json:"name"`
	DocumentNamespace string             `json:"documentNamespace"`
	CreationInfo      spdxCreationInfo   `json:"creationInfo"`
	Packages          []SBOMPackage      `json:"packages"`
	Relationships     []spdxRelationship `json:"relationships"`
}

func (m normalizedSBOMManifest) document() spdxDocument {
	relationships := make([]spdxRelationship, 0, len(m.packages))
	for _, pkg := range m.packages {
		relationships = append(relationships, spdxRelationship{
			SPDXElementID:      spdxDocumentID,
			RelationshipType:   "DESCRIBES",
			RelatedSPDXElement: pkg.SPDXID,
		})
	}

	return spdxDocument{
		SPDXVersion:       spdxVersion,
		DataLicense:       spdxDataLicense,
		SPDXID:            spdxDocumentID,
		Name:              m.name,
		DocumentNamespace: m.documentNamespace,
		CreationInfo: spdxCreationInfo{
			Created:  m.created,
			Creators: cloneStrings(m.creators),
		},
		Packages:      cloneSBOMPackages(m.packages),
		Relationships: relationships,
	}
}

func normalizeSBOMManifest(manifest SBOMManifest) (normalizedSBOMManifest, error) {
	name, err := normalizeSBOMText("name", manifest.Name, true)
	if err != nil {
		return normalizedSBOMManifest{}, err
	}
	namespace, err := normalizeSBOMText("documentNamespace", manifest.DocumentNamespace, true)
	if err != nil {
		return normalizedSBOMManifest{}, err
	}
	created, err := normalizeSBOMCreated(manifest.Created)
	if err != nil {
		return normalizedSBOMManifest{}, err
	}
	creators, err := normalizeSBOMCreators(manifest.Creators)
	if err != nil {
		return normalizedSBOMManifest{}, err
	}
	packages, err := normalizeSBOMPackages(manifest.Components)
	if err != nil {
		return normalizedSBOMManifest{}, err
	}

	return normalizedSBOMManifest{
		name:              name,
		documentNamespace: namespace,
		created:           created,
		creators:          creators,
		packages:          packages,
	}, nil
}

func normalizeSBOMCreated(created string) (string, error) {
	created = strings.TrimSpace(created)
	if created == "" {
		return "", invalidSBOM("created is required")
	}
	parsed, err := time.Parse(time.RFC3339, created)
	if err != nil {
		return "", invalidSBOM("created must be RFC3339: %v", err)
	}
	return parsed.UTC().Format(time.RFC3339), nil
}

func normalizeSBOMCreators(creators []string) ([]string, error) {
	if len(creators) == 0 {
		return []string{"Tool: lazuli"}, nil
	}

	normalized := make([]string, 0, len(creators))
	seen := map[string]struct{}{}
	for i, creator := range creators {
		value, err := normalizeSBOMText(fmt.Sprintf("creators[%d]", i), creator, true)
		if err != nil {
			return nil, err
		}
		if _, ok := seen[value]; ok {
			return nil, invalidSBOM("duplicate creator %q", value)
		}
		seen[value] = struct{}{}
		normalized = append(normalized, value)
	}
	sort.Strings(normalized)
	return normalized, nil
}

func normalizeSBOMPackages(components []SBOMComponent) ([]SBOMPackage, error) {
	packages := make([]SBOMPackage, 0, len(components))
	for i, component := range components {
		pkg, err := normalizeSBOMPackage(component)
		if err != nil {
			return nil, invalidSBOM("components[%d]: %v", i, err)
		}
		packages = append(packages, pkg)
	}
	sort.SliceStable(packages, func(i, j int) bool {
		return packages[i].SPDXID < packages[j].SPDXID
	})

	seen := map[string]struct{}{}
	for _, pkg := range packages {
		if _, ok := seen[pkg.SPDXID]; ok {
			return nil, invalidSBOM("duplicate package id %s", pkg.SPDXID)
		}
		seen[pkg.SPDXID] = struct{}{}
	}
	return packages, nil
}

func normalizeSBOMPackage(component SBOMComponent) (SBOMPackage, error) {
	name, err := normalizeSBOMText("name", component.Name, true)
	if err != nil {
		return SBOMPackage{}, err
	}
	version, err := normalizeSBOMText("version", component.Version, false)
	if err != nil {
		return SBOMPackage{}, err
	}
	id, err := normalizeSPDXID(component.ID, name, version)
	if err != nil {
		return SBOMPackage{}, err
	}
	supplier, err := normalizeSBOMText("supplier", component.Supplier, false)
	if err != nil {
		return SBOMPackage{}, err
	}
	downloadLocation, err := normalizeSBOMAssertion("downloadLocation", component.DownloadLocation)
	if err != nil {
		return SBOMPackage{}, err
	}
	licenseConcluded, err := normalizeSBOMAssertion("licenseConcluded", component.LicenseConcluded)
	if err != nil {
		return SBOMPackage{}, err
	}
	licenseDeclared, err := normalizeSBOMAssertion("licenseDeclared", component.LicenseDeclared)
	if err != nil {
		return SBOMPackage{}, err
	}
	copyrightText, err := normalizeSBOMAssertion("copyrightText", component.CopyrightText)
	if err != nil {
		return SBOMPackage{}, err
	}
	checksums, err := normalizeSBOMHashes(component.Hashes)
	if err != nil {
		return SBOMPackage{}, err
	}

	var externalRefs []SBOMExternalRef
	if component.PackageURL != "" {
		purl, err := normalizeSBOMText("packageURL", component.PackageURL, true)
		if err != nil {
			return SBOMPackage{}, err
		}
		externalRefs = []SBOMExternalRef{{
			ReferenceCategory: "PACKAGE-MANAGER",
			ReferenceType:     "purl",
			ReferenceLocator:  purl,
		}}
	}

	return SBOMPackage{
		SPDXID:           id,
		Name:             name,
		VersionInfo:      version,
		Supplier:         supplier,
		DownloadLocation: downloadLocation,
		FilesAnalyzed:    false,
		LicenseConcluded: licenseConcluded,
		LicenseDeclared:  licenseDeclared,
		CopyrightText:    copyrightText,
		Checksums:        checksums,
		ExternalRefs:     externalRefs,
	}, nil
}

func normalizeSBOMHashes(hashes []SBOMHash) ([]SBOMHash, error) {
	normalized := make([]SBOMHash, 0, len(hashes))
	seen := map[string]struct{}{}
	for i, hash := range hashes {
		algorithm := strings.ReplaceAll(strings.ToUpper(strings.TrimSpace(hash.Algorithm)), "-", "")
		if algorithm == "" {
			return nil, invalidSBOM("hashes[%d].algorithm is required", i)
		}
		if !isSBOMHashAlgorithm(algorithm) {
			return nil, invalidSBOM("hashes[%d].algorithm %q is not supported", i, hash.Algorithm)
		}
		if _, ok := seen[algorithm]; ok {
			return nil, invalidSBOM("duplicate hash algorithm %s", algorithm)
		}
		seen[algorithm] = struct{}{}

		value := strings.ToLower(strings.TrimSpace(hash.Value))
		if err := validateSBOMHashValue(algorithm, value); err != nil {
			return nil, invalidSBOM("hashes[%d].value: %v", i, err)
		}
		normalized = append(normalized, SBOMHash{
			Algorithm: algorithm,
			Value:     value,
		})
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		if normalized[i].Algorithm != normalized[j].Algorithm {
			return normalized[i].Algorithm < normalized[j].Algorithm
		}
		return normalized[i].Value < normalized[j].Value
	})
	return normalized, nil
}

func normalizeSPDXID(id, name, version string) (string, error) {
	id = strings.TrimSpace(id)
	if id == "" {
		id = "SPDXRef-Package-" + spdxIDPart(name)
		if version != "" {
			id += "-" + spdxIDPart(version)
		}
	}
	if !strings.HasPrefix(id, "SPDXRef-") {
		return "", invalidSBOM("id %q must start with SPDXRef-", id)
	}
	suffix := strings.TrimPrefix(id, "SPDXRef-")
	if suffix == "" {
		return "", invalidSBOM("id must include an SPDXRef suffix")
	}
	for _, r := range suffix {
		if !isSPDXIDRune(r) {
			return "", invalidSBOM("id %q contains unsupported character %q", id, r)
		}
	}
	return id, nil
}

func spdxIDPart(value string) string {
	var builder strings.Builder
	lastDash := false
	for _, r := range value {
		if isSPDXIDRune(r) {
			builder.WriteRune(r)
			lastDash = false
			continue
		}
		if !lastDash {
			builder.WriteByte('-')
			lastDash = true
		}
	}
	part := strings.Trim(builder.String(), "-.")
	if part == "" {
		return "component"
	}
	return part
}

func normalizeSBOMAssertion(field, value string) (string, error) {
	normalized, err := normalizeSBOMText(field, value, false)
	if err != nil {
		return "", err
	}
	if normalized == "" {
		return "NOASSERTION", nil
	}
	return normalized, nil
}

func normalizeSBOMText(field, value string, required bool) (string, error) {
	normalized := strings.TrimSpace(value)
	if normalized == "" {
		if required {
			return "", invalidSBOM("%s is required", field)
		}
		return "", nil
	}
	if normalized != value {
		return "", invalidSBOM("%s must not have leading or trailing whitespace", field)
	}
	for _, r := range normalized {
		if unicode.IsControl(r) {
			return "", invalidSBOM("%s contains control characters", field)
		}
	}
	return normalized, nil
}

func validateSBOMHashValue(algorithm, value string) error {
	if value == "" {
		return errors.New("required")
	}
	for _, r := range value {
		switch {
		case r >= '0' && r <= '9':
		case r >= 'a' && r <= 'f':
		default:
			return errors.New("must be lowercase hex")
		}
	}
	if len(value)%2 != 0 {
		return errors.New("must have even hex length")
	}
	if want := sbomHashHexLength(algorithm); want > 0 && len(value) != want {
		return fmt.Errorf("length %d, want %d for %s", len(value), want, algorithm)
	}
	return nil
}

func sbomHashHexLength(algorithm string) int {
	switch algorithm {
	case "MD5":
		return 32
	case "SHA1":
		return 40
	case "SHA224":
		return 56
	case "SHA256":
		return 64
	case "SHA384":
		return 96
	case "SHA512":
		return 128
	default:
		return 0
	}
}

func isSBOMHashAlgorithm(algorithm string) bool {
	switch algorithm {
	case "MD5", "SHA1", "SHA224", "SHA256", "SHA384", "SHA512":
		return true
	default:
		return false
	}
}

func isSPDXIDRune(r rune) bool {
	return r == '-' || r == '.' || (r >= '0' && r <= '9') || (r >= 'A' && r <= 'Z') || (r >= 'a' && r <= 'z')
}

func cloneSBOMPackages(packages []SBOMPackage) []SBOMPackage {
	if len(packages) == 0 {
		return []SBOMPackage{}
	}
	out := make([]SBOMPackage, len(packages))
	for i, pkg := range packages {
		out[i] = pkg
		out[i].Checksums = append([]SBOMHash(nil), pkg.Checksums...)
		out[i].ExternalRefs = append([]SBOMExternalRef(nil), pkg.ExternalRefs...)
	}
	return out
}

func cloneStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	return append([]string(nil), values...)
}

func invalidSBOM(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidSBOM, fmt.Sprintf(format, args...))
}
