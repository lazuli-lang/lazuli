package storage

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"
)

const versionIDTimeLayout = "20060102T150405.000000000Z"

var (
	// ErrVersioningPolicyInvalid is returned when a version retention or
	// listing policy contains invalid predicates.
	ErrVersioningPolicyInvalid = errors.New("lazuli/storage: versioning_policy_invalid")

	// ErrVersioningObjectInvalid is returned when a version snapshot cannot be
	// addressed or classified.
	ErrVersioningObjectInvalid = errors.New("lazuli/storage: versioning_object_invalid")
)

// VersionID is the adapter-neutral identifier for one stored object version.
// The helper-generated shape is timestamp-prefixed and lexicographically
// sortable, but adapters may persist it opaquely.
type VersionID string

// NewVersionID returns a deterministic, storage-key-safe version id for now and
// sequence. The timestamp is normalized to UTC; sequence breaks ties for
// multiple versions minted in the same nanosecond.
func NewVersionID(now time.Time, sequence uint64) VersionID {
	return VersionID(now.UTC().Format(versionIDTimeLayout) + fmt.Sprintf("-%016x", sequence))
}

// String renders id as its stable string token.
func (id VersionID) String() string {
	return string(id)
}

// ObjectVersion is the adapter-neutral metadata snapshot for one object
// version. Tombstone versions represent deletes; when the latest version for a
// key is a tombstone, the object has no current live value.
type ObjectVersion struct {
	Key          Key
	VersionID    VersionID
	CreatedAt    time.Time
	LastModified time.Time
	Visibility   FileVisibility
	ContentType  string
	Size         int64
	Tombstone    bool
}

// IsTombstone reports whether v is a delete marker rather than live content.
func (v ObjectVersion) IsTombstone() bool {
	return v.Tombstone
}

// LatestObjectVersion returns the newest version in versions, including
// tombstones. Ties are resolved by VersionID so the result is deterministic.
func LatestObjectVersion(versions []ObjectVersion) (ObjectVersion, bool) {
	if len(versions) == 0 {
		return ObjectVersion{}, false
	}

	latest := versions[0]
	for _, version := range versions[1:] {
		if compareObjectVersionRecency(version, latest) > 0 {
			latest = version
		}
	}
	return latest, true
}

// LatestLiveObjectVersion returns the newest non-tombstone version in versions.
func LatestLiveObjectVersion(versions []ObjectVersion) (ObjectVersion, bool) {
	var latest ObjectVersion
	found := false
	for _, version := range versions {
		if version.Tombstone {
			continue
		}
		if !found || compareObjectVersionRecency(version, latest) > 0 {
			latest = version
			found = true
		}
	}
	return latest, found
}

// CurrentObjectVersion returns the current live object version. If the latest
// version is a tombstone, no current version exists even when older live
// versions remain.
func CurrentObjectVersion(versions []ObjectVersion) (ObjectVersion, bool) {
	latest, ok := LatestObjectVersion(versions)
	if !ok || latest.Tombstone {
		return ObjectVersion{}, false
	}
	return latest, true
}

// VersionRetentionPolicy controls dry-run deletion decisions for historical
// versions. RetainLatest defaults to one version per key when zero; time-based
// retention fields are disabled when zero.
type VersionRetentionPolicy struct {
	RetainLatest        int
	RetainNoncurrentFor time.Duration
	RetainTombstonesFor time.Duration
}

// Validate checks that the policy can produce deterministic lifecycle
// decisions.
func (p VersionRetentionPolicy) Validate() error {
	return ValidateVersionRetentionPolicy(p)
}

// VersionRetentionPlan is the dry-run result for version lifecycle decisions.
// It does not call ObjectStore; callers pass selected delete transitions to the
// bound adapter or job runner.
type VersionRetentionPlan struct {
	DryRun      bool
	GeneratedAt time.Time
	Entries     []VersionRetentionPlanEntry
}

// VersionRetentionPlanEntry describes the retention decision for one version.
type VersionRetentionPlanEntry struct {
	Key        Key
	VersionID  VersionID
	Tombstone  bool
	IsLatest   bool
	Transition LifecycleTransition
	Age        time.Duration
	EligibleAt time.Time
	Reason     string
}

// ValidateVersionRetentionPolicy checks version lifecycle predicates for
// structural validity.
func ValidateVersionRetentionPolicy(policy VersionRetentionPolicy) error {
	if policy.RetainLatest < 0 {
		return fmt.Errorf("%w: retain latest must be non-negative", ErrVersioningPolicyInvalid)
	}
	if policy.RetainNoncurrentFor < 0 {
		return fmt.Errorf("%w: noncurrent retention must be non-negative", ErrVersioningPolicyInvalid)
	}
	if policy.RetainTombstonesFor < 0 {
		return fmt.Errorf("%w: tombstone retention must be non-negative", ErrVersioningPolicyInvalid)
	}
	return nil
}

// ValidateObjectVersions checks the version snapshots required for versioning
// plans and latest/current helpers.
func ValidateObjectVersions(versions []ObjectVersion) error {
	for i, version := range versions {
		if version.Key == "" {
			return fmt.Errorf("%w: version %d has empty key", ErrVersioningObjectInvalid, i)
		}
		if version.VersionID == "" {
			return fmt.Errorf("%w: version %d has empty version id", ErrVersioningObjectInvalid, i)
		}
		if version.Size < 0 {
			return fmt.Errorf("%w: version %d has negative size", ErrVersioningObjectInvalid, i)
		}
		if !isKnownFileVisibility(version.Visibility) {
			return fmt.Errorf("%w: version %d has unknown visibility %q", ErrVersioningObjectInvalid, i, version.Visibility)
		}
	}
	return nil
}

// BuildVersionRetentionPlan evaluates policy against versions and returns a
// deterministic dry-run plan ordered by key and newest version first.
func BuildVersionRetentionPlan(policy VersionRetentionPolicy, versions []ObjectVersion, now time.Time) (VersionRetentionPlan, error) {
	if err := ValidateVersionRetentionPolicy(policy); err != nil {
		return VersionRetentionPlan{}, err
	}
	if err := ValidateObjectVersions(versions); err != nil {
		return VersionRetentionPlan{}, err
	}
	if now.IsZero() {
		now = time.Now()
	}

	sorted := sortedObjectVersions(versions)
	latestByKey := latestVersionIDsByKey(sorted)
	plan := VersionRetentionPlan{
		DryRun:      true,
		GeneratedAt: now,
		Entries:     make([]VersionRetentionPlanEntry, 0, len(sorted)),
	}

	retainLatest := policy.RetainLatest
	if retainLatest == 0 {
		retainLatest = 1
	}

	var currentKey Key
	rank := 0
	for _, version := range sorted {
		if version.Key != currentKey {
			currentKey = version.Key
			rank = 0
		}
		rank++
		plan.Entries = append(plan.Entries, versionRetentionPlanEntry(policy, version, now, rank, retainLatest, latestByKey[version.Key] == version.VersionID))
	}

	return plan, nil
}

// VersionListingOptions controls how BuildVersionListingPlan exposes version
// snapshots. The zero value lists current live objects only.
type VersionListingOptions struct {
	Prefix            string
	IncludeNoncurrent bool
	IncludeTombstones bool
	Limit             int
}

// VersionListingPlan is a deterministic listing dry-run. It contains metadata
// only; adapters remain responsible for provider-specific list calls.
type VersionListingPlan struct {
	Entries []VersionListingEntry
}

// VersionListingEntry describes one listed object version.
type VersionListingEntry struct {
	Key          Key
	VersionID    VersionID
	Tombstone    bool
	IsLatest     bool
	Current      bool
	Visibility   FileVisibility
	ContentType  string
	Size         int64
	CreatedAt    time.Time
	LastModified time.Time
}

// BuildVersionListingPlan returns a deterministic listing plan ordered by key
// and newest version first. When IncludeNoncurrent is false, only the latest
// version for each key is eligible; if that latest version is a tombstone and
// IncludeTombstones is false, the key is omitted.
func BuildVersionListingPlan(versions []ObjectVersion, options VersionListingOptions) (VersionListingPlan, error) {
	if options.Limit < 0 {
		return VersionListingPlan{}, fmt.Errorf("%w: limit must be non-negative", ErrVersioningPolicyInvalid)
	}
	if err := ValidateObjectVersions(versions); err != nil {
		return VersionListingPlan{}, err
	}

	sorted := sortedObjectVersions(versions)
	latestByKey := latestVersionIDsByKey(sorted)
	entries := make([]VersionListingEntry, 0, len(sorted))

	for _, version := range sorted {
		if options.Prefix != "" && !strings.HasPrefix(string(version.Key), options.Prefix) {
			continue
		}

		isLatest := latestByKey[version.Key] == version.VersionID
		if !options.IncludeNoncurrent && !isLatest {
			continue
		}
		if version.Tombstone && !options.IncludeTombstones {
			continue
		}

		entries = append(entries, VersionListingEntry{
			Key:          version.Key,
			VersionID:    version.VersionID,
			Tombstone:    version.Tombstone,
			IsLatest:     isLatest,
			Current:      isLatest && !version.Tombstone,
			Visibility:   version.Visibility,
			ContentType:  version.ContentType,
			Size:         version.Size,
			CreatedAt:    version.CreatedAt,
			LastModified: version.LastModified,
		})
		if options.Limit > 0 && len(entries) == options.Limit {
			break
		}
	}

	return VersionListingPlan{Entries: entries}, nil
}

func versionRetentionPlanEntry(
	policy VersionRetentionPolicy,
	version ObjectVersion,
	now time.Time,
	rank int,
	retainLatest int,
	isLatest bool,
) VersionRetentionPlanEntry {
	entry := VersionRetentionPlanEntry{
		Key:        version.Key,
		VersionID:  version.VersionID,
		Tombstone:  version.Tombstone,
		IsLatest:   isLatest,
		Transition: LifecycleRetain,
		Age:        versionAgeAt(version, now),
		Reason:     "within latest retention",
	}
	if rank <= retainLatest {
		return entry
	}

	retention := policy.RetainNoncurrentFor
	retentionKind := "noncurrent"
	if version.Tombstone {
		retention = policy.RetainTombstonesFor
		retentionKind = "tombstone"
	}
	if retention <= 0 {
		entry.Reason = retentionKind + " retention disabled"
		return entry
	}

	entry.EligibleAt = versionEligibleAt(version, retention)
	if versionAgeAtLeast(version, now, retention) {
		entry.Transition = LifecycleDelete
		entry.Reason = retentionKind + " retention elapsed"
		return entry
	}

	entry.Reason = retentionKind + " within retention"
	return entry
}

func sortedObjectVersions(versions []ObjectVersion) []ObjectVersion {
	sorted := append([]ObjectVersion(nil), versions...)
	sort.SliceStable(sorted, func(i, j int) bool {
		if sorted[i].Key != sorted[j].Key {
			return sorted[i].Key < sorted[j].Key
		}
		return compareObjectVersionRecency(sorted[i], sorted[j]) > 0
	})
	return sorted
}

func latestVersionIDsByKey(sorted []ObjectVersion) map[Key]VersionID {
	latest := make(map[Key]VersionID)
	for _, version := range sorted {
		if _, ok := latest[version.Key]; !ok {
			latest[version.Key] = version.VersionID
		}
	}
	return latest
}

func compareObjectVersionRecency(left, right ObjectVersion) int {
	leftStamp := versionTimestamp(left)
	rightStamp := versionTimestamp(right)
	switch {
	case leftStamp.After(rightStamp):
		return 1
	case rightStamp.After(leftStamp):
		return -1
	case left.VersionID > right.VersionID:
		return 1
	case left.VersionID < right.VersionID:
		return -1
	default:
		return 0
	}
}

func versionAgeAt(version ObjectVersion, now time.Time) time.Duration {
	stamp := versionTimestamp(version)
	if stamp.IsZero() || now.Before(stamp) {
		return 0
	}
	return now.Sub(stamp)
}

func versionAgeAtLeast(version ObjectVersion, now time.Time, age time.Duration) bool {
	if age <= 0 {
		return true
	}
	return versionAgeAt(version, now) >= age
}

func versionEligibleAt(version ObjectVersion, age time.Duration) time.Time {
	stamp := versionTimestamp(version)
	if stamp.IsZero() {
		return time.Time{}
	}
	return stamp.Add(age)
}

func versionTimestamp(version ObjectVersion) time.Time {
	if !version.CreatedAt.IsZero() {
		return version.CreatedAt
	}
	return version.LastModified
}
