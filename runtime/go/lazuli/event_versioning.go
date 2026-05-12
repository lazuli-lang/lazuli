package lazuli

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"sync"
)

var (
	// ErrInvalidEventVersion is returned when an event version is not a valid
	// semantic version.
	ErrInvalidEventVersion = errors.New("lazuli: event version is invalid")

	// ErrNilEventUpcasterRegistry is returned when an upcast is requested
	// without a registry.
	ErrNilEventUpcasterRegistry = errors.New("lazuli: event upcaster registry is nil")

	// ErrEventUpcasterNameRequired is returned when an upcaster is registered
	// without an event name.
	ErrEventUpcasterNameRequired = errors.New("lazuli: event upcaster name is required")

	// ErrNilEventUpcaster is returned when a nil upcaster is registered.
	ErrNilEventUpcaster = errors.New("lazuli: event upcaster is nil")

	// ErrEventUpcasterVersionOrder is returned when an upcaster does not move
	// an event to a later semantic version.
	ErrEventUpcasterVersionOrder = errors.New("lazuli: event upcaster version order is invalid")

	// ErrEventUpcasterDuplicate is returned when an upcaster edge is registered
	// more than once.
	ErrEventUpcasterDuplicate = errors.New("lazuli: event upcaster already registered")

	// ErrEventVersionDowngrade is returned when an upcast request targets an
	// older version than the event currently has.
	ErrEventVersionDowngrade = errors.New("lazuli: event version downgrade is not supported")

	// ErrEventUpcasterPathMissing is returned when the registry cannot build a
	// complete chain from the current version to the target version.
	ErrEventUpcasterPathMissing = errors.New("lazuli: event upcaster path is missing")
)

// EventVersion is a semantic version used to identify an event payload schema.
type EventVersion struct {
	Major int
	Minor int
	Patch int

	PreRelease string
	Build      string
}

// ParseEventVersion parses a semantic event version. A leading "v" prefix is
// accepted for convenience and omitted by String.
func ParseEventVersion(input string) (EventVersion, error) {
	version := strings.TrimSpace(input)
	if version == "" {
		return EventVersion{}, fmt.Errorf("lazuli: parse event version %q: %w", input, ErrInvalidEventVersion)
	}
	if version[0] == 'v' || version[0] == 'V' {
		version = version[1:]
	}

	coreAndPre, build, hasBuild := strings.Cut(version, "+")
	if hasBuild {
		if build == "" || strings.Contains(build, "+") || !validEventVersionIdentifiers(build, false) {
			return EventVersion{}, fmt.Errorf("lazuli: parse event version %q: %w", input, ErrInvalidEventVersion)
		}
	}

	core, preRelease, hasPreRelease := strings.Cut(coreAndPre, "-")
	if hasPreRelease {
		if preRelease == "" || !validEventVersionIdentifiers(preRelease, true) {
			return EventVersion{}, fmt.Errorf("lazuli: parse event version %q: %w", input, ErrInvalidEventVersion)
		}
	}

	parts := strings.Split(core, ".")
	if len(parts) != 3 {
		return EventVersion{}, fmt.Errorf("lazuli: parse event version %q: %w", input, ErrInvalidEventVersion)
	}

	major, err := parseEventVersionNumber(parts[0])
	if err != nil {
		return EventVersion{}, fmt.Errorf("lazuli: parse event version %q: %w", input, err)
	}
	minor, err := parseEventVersionNumber(parts[1])
	if err != nil {
		return EventVersion{}, fmt.Errorf("lazuli: parse event version %q: %w", input, err)
	}
	patch, err := parseEventVersionNumber(parts[2])
	if err != nil {
		return EventVersion{}, fmt.Errorf("lazuli: parse event version %q: %w", input, err)
	}

	return EventVersion{
		Major:      major,
		Minor:      minor,
		Patch:      patch,
		PreRelease: preRelease,
		Build:      build,
	}, nil
}

// MustParseEventVersion parses version or panics.
func MustParseEventVersion(version string) EventVersion {
	parsed, err := ParseEventVersion(version)
	if err != nil {
		panic(err)
	}
	return parsed
}

// String returns the normalized semantic version string.
func (v EventVersion) String() string {
	out := fmt.Sprintf("%d.%d.%d", v.Major, v.Minor, v.Patch)
	if v.PreRelease != "" {
		out += "-" + v.PreRelease
	}
	if v.Build != "" {
		out += "+" + v.Build
	}
	return out
}

// Compare returns -1 when v is older than other, 0 when they have equal
// precedence, and 1 when v is newer than other. Build metadata is ignored.
func (v EventVersion) Compare(other EventVersion) int {
	switch {
	case v.Major != other.Major:
		return compareInts(v.Major, other.Major)
	case v.Minor != other.Minor:
		return compareInts(v.Minor, other.Minor)
	case v.Patch != other.Patch:
		return compareInts(v.Patch, other.Patch)
	case v.PreRelease == "" && other.PreRelease == "":
		return 0
	case v.PreRelease == "":
		return 1
	case other.PreRelease == "":
		return -1
	default:
		return comparePreRelease(v.PreRelease, other.PreRelease)
	}
}

// CompareEventVersions compares two semantic event versions.
func CompareEventVersions(left, right EventVersion) int {
	return left.Compare(right)
}

// EventUpcaster transforms an event payload from one registered version to the
// next. It should return a complete event envelope for the new version.
type EventUpcaster func(ctx context.Context, event Event) (Event, error)

// EventUpcasterRegistry stores adapter-neutral event upcasters.
type EventUpcasterRegistry struct {
	mu        sync.RWMutex
	upcasters map[eventUpcasterKey]eventUpcasterEntry
}

// NewEventUpcasterRegistry returns an empty event upcaster registry.
func NewEventUpcasterRegistry() *EventUpcasterRegistry {
	return &EventUpcasterRegistry{}
}

// Register records an upcaster edge for one event name and version range.
func (r *EventUpcasterRegistry) Register(name string, from, to EventVersion, upcaster EventUpcaster) error {
	if r == nil {
		return ErrNilEventUpcasterRegistry
	}
	name = strings.TrimSpace(name)
	if name == "" {
		return ErrEventUpcasterNameRequired
	}
	if upcaster == nil {
		return ErrNilEventUpcaster
	}
	if from.Compare(to) >= 0 {
		return fmt.Errorf("lazuli: register event upcaster %q from %s to %s: %w", name, from, to, ErrEventUpcasterVersionOrder)
	}

	key := eventUpcasterKey{
		name: name,
		from: from.registryKey(),
		to:   to.registryKey(),
	}

	r.mu.Lock()
	defer r.mu.Unlock()
	if r.upcasters == nil {
		r.upcasters = make(map[eventUpcasterKey]eventUpcasterEntry)
	}
	if _, ok := r.upcasters[key]; ok {
		return fmt.Errorf("lazuli: register event upcaster %q from %s to %s: %w", name, from, to, ErrEventUpcasterDuplicate)
	}
	r.upcasters[key] = eventUpcasterEntry{
		to:       to.withoutBuild(),
		upcaster: upcaster,
	}
	return nil
}

// Upcast applies registered upcasters in version order until target is reached.
func (r *EventUpcasterRegistry) Upcast(ctx context.Context, event Event, from, target EventVersion) (Event, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if r == nil {
		return Event{}, ErrNilEventUpcasterRegistry
	}
	if err := ctx.Err(); err != nil {
		return Event{}, err
	}
	if from.Compare(target) > 0 {
		return Event{}, fmt.Errorf("lazuli: upcast event %q from %s to %s: %w", event.Name, from, target, ErrEventVersionDowngrade)
	}

	current := from.withoutBuild()
	target = target.withoutBuild()
	steps, ok := r.chain(event.Name, current, target)
	if !ok {
		return Event{}, fmt.Errorf("lazuli: upcast event %q from %s to %s: %w", event.Name, current, target, ErrEventUpcasterPathMissing)
	}

	upcasted := cloneEvent(event)
	for _, step := range steps {
		if err := ctx.Err(); err != nil {
			return Event{}, err
		}

		nextEvent, err := step.upcaster(ctx, cloneEvent(upcasted))
		if err != nil {
			return Event{}, fmt.Errorf("lazuli: upcast event %q from %s to %s: %w", event.Name, step.from, step.to, err)
		}
		upcasted = cloneEvent(nextEvent)
	}
	return cloneEvent(upcasted), nil
}

type eventVersionKey struct {
	major      int
	minor      int
	patch      int
	preRelease string
}

type eventUpcasterKey struct {
	name string
	from eventVersionKey
	to   eventVersionKey
}

type eventUpcasterEntry struct {
	to       EventVersion
	upcaster EventUpcaster
}

type eventUpcasterStep struct {
	from     EventVersion
	to       EventVersion
	upcaster EventUpcaster
}

func (r *EventUpcasterRegistry) chain(name string, current, target EventVersion) ([]eventUpcasterStep, bool) {
	if current.Compare(target) == 0 {
		return nil, true
	}

	r.mu.RLock()
	defer r.mu.RUnlock()

	return r.chainLocked(name, current, target)
}

func (r *EventUpcasterRegistry) chainLocked(name string, current, target EventVersion) ([]eventUpcasterStep, bool) {
	candidates := make([]eventUpcasterStep, 0)
	for key, entry := range r.upcasters {
		if key.name != name || key.from != current.registryKey() {
			continue
		}
		if entry.to.Compare(current) <= 0 || entry.to.Compare(target) > 0 {
			continue
		}
		candidates = append(candidates, eventUpcasterStep{
			from:     current,
			to:       entry.to,
			upcaster: entry.upcaster,
		})
	}
	sort.Slice(candidates, func(i, j int) bool {
		return candidates[i].to.Compare(candidates[j].to) < 0
	})

	for _, candidate := range candidates {
		if candidate.to.Compare(target) == 0 {
			return []eventUpcasterStep{candidate}, true
		}
		rest, ok := r.chainLocked(name, candidate.to, target)
		if ok {
			return append([]eventUpcasterStep{candidate}, rest...), true
		}
	}
	return nil, false
}

func (v EventVersion) withoutBuild() EventVersion {
	v.Build = ""
	return v
}

func (v EventVersion) registryKey() eventVersionKey {
	v = v.withoutBuild()
	return eventVersionKey{
		major:      v.Major,
		minor:      v.Minor,
		patch:      v.Patch,
		preRelease: v.PreRelease,
	}
}

func parseEventVersionNumber(input string) (int, error) {
	if input == "" {
		return 0, ErrInvalidEventVersion
	}
	if len(input) > 1 && input[0] == '0' {
		return 0, ErrInvalidEventVersion
	}
	for i := 0; i < len(input); i++ {
		if !isEventVersionDigit(input[i]) {
			return 0, ErrInvalidEventVersion
		}
	}
	value, err := strconv.Atoi(input)
	if err != nil {
		return 0, ErrInvalidEventVersion
	}
	return value, nil
}

func validEventVersionIdentifiers(input string, checkNumericLeadingZero bool) bool {
	parts := strings.Split(input, ".")
	for _, part := range parts {
		if part == "" {
			return false
		}
		allDigits := true
		for i := 0; i < len(part); i++ {
			if !isEventVersionDigit(part[i]) {
				allDigits = false
			}
			if !isEventVersionDigit(part[i]) && !isEventVersionLetter(part[i]) && part[i] != '-' {
				return false
			}
		}
		if checkNumericLeadingZero && allDigits && len(part) > 1 && part[0] == '0' {
			return false
		}
	}
	return true
}

func compareInts(left, right int) int {
	switch {
	case left < right:
		return -1
	case left > right:
		return 1
	default:
		return 0
	}
}

func comparePreRelease(left, right string) int {
	leftParts := strings.Split(left, ".")
	rightParts := strings.Split(right, ".")
	for i := 0; i < len(leftParts) && i < len(rightParts); i++ {
		leftPart := leftParts[i]
		rightPart := rightParts[i]
		leftNumber, leftIsNumber := eventVersionIdentifierNumber(leftPart)
		rightNumber, rightIsNumber := eventVersionIdentifierNumber(rightPart)
		switch {
		case leftIsNumber && rightIsNumber && leftNumber != rightNumber:
			return compareInts(leftNumber, rightNumber)
		case leftIsNumber && !rightIsNumber:
			return -1
		case !leftIsNumber && rightIsNumber:
			return 1
		case !leftIsNumber && !rightIsNumber && leftPart != rightPart:
			if leftPart < rightPart {
				return -1
			}
			return 1
		}
	}
	return compareInts(len(leftParts), len(rightParts))
}

func eventVersionIdentifierNumber(input string) (int, bool) {
	for i := 0; i < len(input); i++ {
		if !isEventVersionDigit(input[i]) {
			return 0, false
		}
	}
	value, err := strconv.Atoi(input)
	if err != nil {
		return 0, false
	}
	return value, true
}

func isEventVersionDigit(b byte) bool {
	return b >= '0' && b <= '9'
}

func isEventVersionLetter(b byte) bool {
	return (b >= 'A' && b <= 'Z') || (b >= 'a' && b <= 'z')
}
