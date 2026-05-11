package cache

// Tag fan-out helper: given a list of authored tag labels and the
// universe of cached entries (each carrying its own tag set), return
// the keys whose tags intersect the labels. Backends use this when
// implementing `InvalidateTags`.
//
// This file declares the algorithm at the typed layer; concrete
// backends own their own storage details.

// IntersectTags returns true when `entryTags` shares at least one
// label with `wanted`. Order doesn't matter; comparison is exact (no
// normalisation — labels are already lowercase identifiers per the
// language contract).
func IntersectTags(entryTags []string, wanted []string) bool {
	if len(entryTags) == 0 || len(wanted) == 0 {
		return false
	}
	// Convert `wanted` to a tiny set; entries are usually small.
	set := make(map[string]struct{}, len(wanted))
	for _, label := range wanted {
		set[label] = struct{}{}
	}
	for _, tag := range entryTags {
		if _, ok := set[tag]; ok {
			return true
		}
	}
	return false
}
