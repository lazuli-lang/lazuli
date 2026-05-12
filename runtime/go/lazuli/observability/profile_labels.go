package observability

import "strings"

const (
	// ProfileLabelSource is the optional pprof label key for a Lazuli source location.
	ProfileLabelSource = "source"
)

// ProfileOpIdentity is the normalized Lazuli operation identity carried by
// profile labels and used by profile reports.
type ProfileOpIdentity struct {
	Feature string `json:"feature,omitempty"`
	Kind    string `json:"kind,omitempty"`
	Op      string `json:"op,omitempty"`
	Source  string `json:"source,omitempty"`

	PatternID      string `json:"pattern_id,omitempty"`
	PatternVersion string `json:"pattern_version,omitempty"`
}

// NormalizeProfileLabels returns a copy of labels with Lazuli pprof label keys
// normalized for profile reports. Empty keys and values are omitted.
func NormalizeProfileLabels(labels map[string]string) map[string]string {
	if len(labels) == 0 {
		return nil
	}

	normalized := make(map[string]string, len(labels))
	nameOp := ""
	for key, value := range labels {
		key = strings.TrimSpace(key)
		if key == "" {
			continue
		}

		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if key == opLabelNameKey {
			nameOp = value
			continue
		}
		normalized[key] = value
	}

	if _, ok := normalized[ProfileLabelOp]; !ok && nameOp != "" {
		normalized[ProfileLabelOp] = nameOp
	}

	if len(normalized) == 0 {
		return nil
	}
	return normalized
}

// ProfileOpIdentityFromLabels extracts a Lazuli operation identity from pprof
// labels. The returned boolean is false when feature, kind, or op is absent.
func ProfileOpIdentityFromLabels(labels map[string]string) (ProfileOpIdentity, bool) {
	labels = NormalizeProfileLabels(labels)
	identity := ProfileOpIdentity{
		Feature:        profileLabelValue(labels, ProfileLabelFeature),
		Kind:           profileLabelValue(labels, ProfileLabelKind),
		Op:             profileLabelValue(labels, ProfileLabelOp),
		Source:         profileLabelValue(labels, ProfileLabelSource),
		PatternID:      profileLabelValue(labels, ProfileLabelPatternID),
		PatternVersion: profileLabelValue(labels, ProfileLabelPatternVersion),
	}
	return identity, profileOpIdentityComplete(identity)
}

func profileLabelValue(labels map[string]string, keys ...string) string {
	if labels == nil {
		return ""
	}
	for _, key := range keys {
		if value := strings.TrimSpace(labels[key]); value != "" {
			return value
		}
	}
	return ""
}

func profileOpIdentityComplete(identity ProfileOpIdentity) bool {
	return identity.Feature != "" && identity.Kind != "" && identity.Op != ""
}
