package secret

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"
)

const (
	// VersionActive is the provider-neutral label for the active secret version.
	VersionActive VersionLabel = "active"
	// VersionNext is the provider-neutral label for the next planned secret version.
	VersionNext VersionLabel = "next"
	// VersionPrevious is the provider-neutral label for the previous secret version.
	VersionPrevious VersionLabel = "previous"
)

var (
	// ErrRotationPurposeRequired is returned when a rotation schedule has no purpose.
	ErrRotationPurposeRequired = errors.New("lazuli/secret: rotation purpose required")
	// ErrRotationVersionRequired is returned when a rotation schedule has no concrete version.
	ErrRotationVersionRequired = errors.New("lazuli/secret: rotation version required")
	// ErrRotationActivationRequired is returned when a rotation version has no activation time.
	ErrRotationActivationRequired = errors.New("lazuli/secret: rotation activation required")
	// ErrRotationOverlapInvalid is returned when a rotation overlap is negative or collides with another window.
	ErrRotationOverlapInvalid = errors.New("lazuli/secret: rotation overlap invalid")
	// ErrDuplicateRotationPurpose is returned when a plan contains the same purpose twice.
	ErrDuplicateRotationPurpose = errors.New("lazuli/secret: duplicate rotation purpose")
	// ErrDuplicateRotationVersion is returned when a schedule contains the same version label twice.
	ErrDuplicateRotationVersion = errors.New("lazuli/secret: duplicate rotation version")
)

// RotationVersion is one concrete version in a rotation schedule.
//
// Ref should identify the versioned secret material. ActiveAt is the instant
// when callers should start using this version for new work.
type RotationVersion struct {
	Ref      SecretRef
	ActiveAt time.Time
}

// RotationOverlapWindow describes the grace period after a new version becomes
// active while the previous version may still need to be accepted.
//
// StartsAt is inclusive and EndsAt is exclusive.
type RotationOverlapWindow struct {
	Previous RotationVersion
	Active   RotationVersion
	StartsAt time.Time
	EndsAt   time.Time
}

// Contains reports whether at falls inside the overlap window.
func (w RotationOverlapWindow) Contains(at time.Time) bool {
	return !at.Before(w.StartsAt) && at.Before(w.EndsAt)
}

// RotationSchedule is the ordered plan for rotating one secret purpose.
//
// Versions may be supplied in any order. Overlap is applied to each transition
// after the new version becomes active.
type RotationSchedule struct {
	Purpose  string
	Overlap  time.Duration
	Versions []RotationVersion
}

// ValidateRotationSchedule checks that schedule can be used for deterministic
// rotation planning.
func ValidateRotationSchedule(schedule RotationSchedule) error {
	return schedule.Validate()
}

// Validate checks that schedule can be used for deterministic rotation planning.
func (s RotationSchedule) Validate() error {
	_, err := normalizeRotationSchedule(s)
	return err
}

// ActiveVersion returns the latest version active at the supplied time.
func (s RotationSchedule) ActiveVersion(at time.Time) (RotationVersion, bool) {
	versions := sortedRotationVersions(s.Versions)
	active := -1
	for i, version := range versions {
		if version.ActiveAt.After(at) {
			break
		}
		active = i
	}
	if active < 0 {
		return RotationVersion{}, false
	}
	return versions[active], true
}

// NextVersion returns the next version scheduled after the supplied time.
func (s RotationSchedule) NextVersion(at time.Time) (RotationVersion, bool) {
	versions := sortedRotationVersions(s.Versions)
	for _, version := range versions {
		if version.ActiveAt.After(at) {
			return version, true
		}
	}
	return RotationVersion{}, false
}

// PreviousVersion returns the version immediately before the active version.
func (s RotationSchedule) PreviousVersion(at time.Time) (RotationVersion, bool) {
	versions := sortedRotationVersions(s.Versions)
	active := -1
	for i, version := range versions {
		if version.ActiveAt.After(at) {
			break
		}
		active = i
	}
	if active <= 0 {
		return RotationVersion{}, false
	}
	return versions[active-1], true
}

// OverlapWindows returns every transition overlap window in activation order.
func (s RotationSchedule) OverlapWindows() []RotationOverlapWindow {
	if s.Overlap <= 0 {
		return nil
	}
	versions := sortedRotationVersions(s.Versions)
	if len(versions) < 2 {
		return nil
	}

	windows := make([]RotationOverlapWindow, 0, len(versions)-1)
	for i := 1; i < len(versions); i++ {
		startsAt := versions[i].ActiveAt
		windows = append(windows, RotationOverlapWindow{
			Previous: versions[i-1],
			Active:   versions[i],
			StartsAt: startsAt,
			EndsAt:   startsAt.Add(s.Overlap),
		})
	}
	return windows
}

// OverlapWindow returns the overlap window containing at, if any.
func (s RotationSchedule) OverlapWindow(at time.Time) (RotationOverlapWindow, bool) {
	for _, window := range s.OverlapWindows() {
		if window.Contains(at) {
			return window, true
		}
	}
	return RotationOverlapWindow{}, false
}

// RotationPlan groups rotation schedules for lookup by purpose.
type RotationPlan struct {
	Schedules []RotationSchedule
}

// ValidateRotationPlan checks every schedule and rejects duplicate purposes.
func ValidateRotationPlan(plan RotationPlan) error {
	return plan.Validate()
}

// Validate checks every schedule and rejects duplicate purposes.
func (p RotationPlan) Validate() error {
	seen := make(map[string]struct{}, len(p.Schedules))
	for i, schedule := range p.Schedules {
		normalized, err := normalizeRotationSchedule(schedule)
		if err != nil {
			return fmt.Errorf("rotation schedule %d: %w", i, err)
		}
		if _, exists := seen[normalized.Purpose]; exists {
			return fmt.Errorf("%w %q", ErrDuplicateRotationPurpose, normalized.Purpose)
		}
		seen[normalized.Purpose] = struct{}{}
	}
	return nil
}

// LookupPurpose returns the schedule for purpose.
func (p RotationPlan) LookupPurpose(purpose string) (RotationSchedule, bool) {
	purpose = normalizeRotationPurpose(purpose)
	if purpose == "" {
		return RotationSchedule{}, false
	}
	for _, schedule := range p.Schedules {
		if normalizeRotationPurpose(schedule.Purpose) == purpose {
			return cloneRotationSchedule(schedule), true
		}
	}
	return RotationSchedule{}, false
}

func normalizeRotationSchedule(schedule RotationSchedule) (RotationSchedule, error) {
	schedule.Purpose = normalizeRotationPurpose(schedule.Purpose)
	if schedule.Purpose == "" {
		return schedule, ErrRotationPurposeRequired
	}
	if schedule.Overlap < 0 {
		return schedule, ErrRotationOverlapInvalid
	}
	if len(schedule.Versions) == 0 {
		return schedule, ErrRotationVersionRequired
	}

	schedule.Versions = sortedRotationVersions(schedule.Versions)
	seenVersions := make(map[VersionLabel]struct{}, len(schedule.Versions))
	for i, version := range schedule.Versions {
		ref, err := normalizeRef(version.Ref)
		if err != nil {
			return schedule, err
		}
		if ref.Version == "" {
			return schedule, ErrRotationVersionRequired
		}
		if version.ActiveAt.IsZero() {
			return schedule, ErrRotationActivationRequired
		}
		if _, exists := seenVersions[ref.Version]; exists {
			return schedule, fmt.Errorf("%w %q", ErrDuplicateRotationVersion, ref.Version)
		}
		if i > 0 && !schedule.Versions[i-1].ActiveAt.Before(version.ActiveAt) {
			return schedule, fmt.Errorf("%w: activation times must be strictly increasing", ErrRotationActivationRequired)
		}
		seenVersions[ref.Version] = struct{}{}
		schedule.Versions[i].Ref = ref
	}

	for i := 1; i+1 < len(schedule.Versions); i++ {
		endsAt := schedule.Versions[i].ActiveAt.Add(schedule.Overlap)
		if endsAt.After(schedule.Versions[i+1].ActiveAt) {
			return schedule, fmt.Errorf("%w for purpose %q", ErrRotationOverlapInvalid, schedule.Purpose)
		}
	}

	return schedule, nil
}

func sortedRotationVersions(versions []RotationVersion) []RotationVersion {
	out := make([]RotationVersion, len(versions))
	copy(out, versions)
	for i := range out {
		out[i].Ref.Name = strings.TrimSpace(trimEnvPrefix(out[i].Ref.Name))
		out[i].Ref.Version = VersionLabel(strings.TrimSpace(string(out[i].Ref.Version)))
	}
	sort.SliceStable(out, func(i, j int) bool {
		return rotationVersionLess(out[i], out[j])
	})
	return out
}

func rotationVersionLess(a, b RotationVersion) bool {
	if !a.ActiveAt.Equal(b.ActiveAt) {
		return a.ActiveAt.Before(b.ActiveAt)
	}
	if a.Ref.Name != b.Ref.Name {
		return a.Ref.Name < b.Ref.Name
	}
	return string(a.Ref.Version) < string(b.Ref.Version)
}

func normalizeRotationPurpose(purpose string) string {
	return strings.TrimSpace(purpose)
}

func cloneRotationSchedule(schedule RotationSchedule) RotationSchedule {
	schedule.Purpose = normalizeRotationPurpose(schedule.Purpose)
	schedule.Versions = sortedRotationVersions(schedule.Versions)
	return schedule
}
