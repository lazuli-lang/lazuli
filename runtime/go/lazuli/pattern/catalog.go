// Package pattern provides the runtime catalog for Lazuli codegen pattern
// annotations.
package pattern

import (
	"errors"
	"fmt"
	"strings"
)

// AnnotationPrefix is the comment marker used before generated Go functions.
const AnnotationPrefix = "//lazuli:pattern"

// PatternID names a generated code pattern emitted by Lazuli codegen.
type PatternID string

const (
	PatternCommandPgxInsert    PatternID = "command_pgx_insert"
	PatternCommandPgxUpdate    PatternID = "command_pgx_update"
	PatternQueryPgxList        PatternID = "query_pgx_list"
	PatternQueryPgxLookup      PatternID = "query_pgx_lookup"
	PatternResourceListPgxScan PatternID = "resource_list_pgx_scan"
	PatternJobRiverWorker      PatternID = "job_river_worker"
	PatternWebhookHmacReceiver PatternID = "webhook_hmac_receiver"
)

// PatternVersion names the implementation version for a generated pattern.
type PatternVersion string

const (
	VersionV1 PatternVersion = "v1"
	VersionV2 PatternVersion = "v2"
	VersionV3 PatternVersion = "v3"
)

var (
	// ErrInvalidAnnotation is returned when a comment is not a Lazuli pattern
	// annotation in canonical form.
	ErrInvalidAnnotation = errors.New("lazuli/pattern: invalid annotation")
	// ErrUnknownPatternID is returned when a PatternID is outside the catalog.
	ErrUnknownPatternID = errors.New("lazuli/pattern: unknown pattern id")
	// ErrUnknownPatternVersion is returned when a PatternVersion is outside the
	// catalog.
	ErrUnknownPatternVersion = errors.New("lazuli/pattern: unknown pattern version")
)

var catalogPatternIDs = []PatternID{
	PatternCommandPgxInsert,
	PatternCommandPgxUpdate,
	PatternQueryPgxList,
	PatternQueryPgxLookup,
	PatternResourceListPgxScan,
	PatternJobRiverWorker,
	PatternWebhookHmacReceiver,
}

var catalogPatternVersions = []PatternVersion{
	VersionV1,
	VersionV2,
	VersionV3,
}

var catalogPatternIDSet = catalogBuildPatternIDSet(catalogPatternIDs)
var catalogPatternVersionSet = catalogBuildPatternVersionSet(catalogPatternVersions)

// Annotation is a parsed Lazuli pattern annotation.
type Annotation struct {
	ID      PatternID
	Version PatternVersion
}

// ParseAnnotation parses a canonical Lazuli pattern annotation comment.
//
// The accepted shape is:
//
//	//lazuli:pattern <id> <version>
func ParseAnnotation(line string) (Annotation, error) {
	fields := strings.Fields(strings.TrimSpace(line))
	if len(fields) != 3 || fields[0] != AnnotationPrefix {
		return Annotation{}, fmt.Errorf("%w: expected %q", ErrInvalidAnnotation, AnnotationPrefix+" <id> <version>")
	}

	annotation := Annotation{
		ID:      PatternID(fields[1]),
		Version: PatternVersion(fields[2]),
	}
	if err := annotation.Validate(); err != nil {
		return Annotation{}, err
	}
	return annotation, nil
}

// FormatAnnotation returns the canonical Lazuli pattern annotation comment.
func FormatAnnotation(id PatternID, version PatternVersion) (string, error) {
	annotation := Annotation{ID: id, Version: version}
	if err := annotation.Validate(); err != nil {
		return "", err
	}
	return annotation.String(), nil
}

// Validate returns nil when annotation uses a known pattern ID and version.
func (annotation Annotation) Validate() error {
	return ValidateAnnotation(annotation.ID, annotation.Version)
}

// String returns the canonical comment spelling for annotation.
func (annotation Annotation) String() string {
	return AnnotationPrefix + " " + string(annotation.ID) + " " + string(annotation.Version)
}

// ValidateAnnotation returns nil when id and version are both in the catalog.
func ValidateAnnotation(id PatternID, version PatternVersion) error {
	if !IsKnownPatternID(id) {
		return fmt.Errorf("%w: %q", ErrUnknownPatternID, id)
	}
	if !IsKnownPatternVersion(version) {
		return fmt.Errorf("%w: %q", ErrUnknownPatternVersion, version)
	}
	return nil
}

// IsKnownAnnotation reports whether id and version are both in the catalog.
func IsKnownAnnotation(id PatternID, version PatternVersion) bool {
	return IsKnownPatternID(id) && IsKnownPatternVersion(version)
}

// IsKnownPatternID reports whether id is in the closed catalog.
func IsKnownPatternID(id PatternID) bool {
	_, ok := catalogPatternIDSet[id]
	return ok
}

// IsKnownPatternVersion reports whether version is in the closed catalog.
func IsKnownPatternVersion(version PatternVersion) bool {
	_, ok := catalogPatternVersionSet[version]
	return ok
}

// PatternIDs returns the known pattern IDs in catalog order.
func PatternIDs() []PatternID {
	return append([]PatternID(nil), catalogPatternIDs...)
}

// PatternVersions returns the known pattern versions in catalog order.
func PatternVersions() []PatternVersion {
	return append([]PatternVersion(nil), catalogPatternVersions...)
}

func catalogBuildPatternIDSet(ids []PatternID) map[PatternID]struct{} {
	set := make(map[PatternID]struct{}, len(ids))
	for _, id := range ids {
		set[id] = struct{}{}
	}
	return set
}

func catalogBuildPatternVersionSet(versions []PatternVersion) map[PatternVersion]struct{} {
	set := make(map[PatternVersion]struct{}, len(versions))
	for _, version := range versions {
		set[version] = struct{}{}
	}
	return set
}
