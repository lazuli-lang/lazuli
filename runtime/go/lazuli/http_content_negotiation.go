package lazuli

import (
	"math"
	"mime"
	"strconv"
	"strings"
)

// AcceptMediaRange is one parsed media range from an HTTP Accept header.
type AcceptMediaRange struct {
	// MediaType is the normalized type/subtype without parameters.
	MediaType string

	// Type is the normalized media type family, for example "application".
	Type string

	// Subtype is the normalized media subtype, for example "json" or "*".
	Subtype string

	// Params contains media type parameters that appeared before q.
	Params map[string]string

	// Quality is the q value for this range. Missing q defaults to 1.
	Quality float64
}

// ParseAccept parses an HTTP Accept header into media ranges in header order.
// Invalid ranges are ignored. q=0 ranges are retained so negotiation can reject
// a more specific media type even when a later wildcard would otherwise match.
func ParseAccept(header string) []AcceptMediaRange {
	var ranges []AcceptMediaRange
	for _, part := range splitHeaderValue(header, ',') {
		if mediaRange, ok := parseAcceptMediaRange(part); ok {
			ranges = append(ranges, mediaRange)
		}
	}
	return ranges
}

// NegotiateContentType returns the best offered content type for acceptHeader.
// Empty or invalid Accept headers allow any offered type. The returned value is
// the matching offered content type, trimmed but otherwise preserved.
func NegotiateContentType(acceptHeader string, offers ...string) (string, bool) {
	return BestMediaType(ParseAccept(acceptHeader), offers...)
}

// BestMediaType returns the best offered media type for parsed Accept ranges.
// Empty ranges allow any offered type. Ties are resolved by client range order,
// then by offered order so results are stable for equal preferences.
func BestMediaType(ranges []AcceptMediaRange, offers ...string) (string, bool) {
	parsedOffers := make([]negotiationOffer, 0, len(offers))
	for _, offer := range offers {
		parsed, ok := parseNegotiationOffer(offer)
		if ok {
			parsedOffers = append(parsedOffers, parsed)
		}
	}
	if len(parsedOffers) == 0 {
		return "", false
	}
	if len(ranges) == 0 {
		return parsedOffers[0].value, true
	}

	best := negotiationCandidate{quality: -1}
	for offerOrder, offer := range parsedOffers {
		match, ok := bestRangeForOffer(ranges, offer)
		if !ok || match.quality <= 0 {
			continue
		}

		candidate := negotiationCandidate{
			value:       offer.value,
			quality:     match.quality,
			specificity: match.specificity,
			params:      match.params,
			rangeOrder:  match.rangeOrder,
			offerOrder:  offerOrder,
		}
		if candidate.betterThan(best) {
			best = candidate
		}
	}
	if best.quality < 0 {
		return "", false
	}
	return best.value, true
}

type negotiationOffer struct {
	value   string
	typ     string
	subtype string
	params  map[string]string
}

type negotiationMatch struct {
	quality     float64
	specificity int
	params      int
	rangeOrder  int
}

type negotiationCandidate struct {
	value       string
	quality     float64
	specificity int
	params      int
	rangeOrder  int
	offerOrder  int
}

func (c negotiationCandidate) betterThan(other negotiationCandidate) bool {
	if c.quality != other.quality {
		return c.quality > other.quality
	}
	if c.specificity != other.specificity {
		return c.specificity > other.specificity
	}
	if c.params != other.params {
		return c.params > other.params
	}
	if c.rangeOrder != other.rangeOrder {
		return c.rangeOrder < other.rangeOrder
	}
	return c.offerOrder < other.offerOrder
}

func parseAcceptMediaRange(part string) (AcceptMediaRange, bool) {
	part = strings.TrimSpace(part)
	if part == "" {
		return AcceptMediaRange{}, false
	}

	segments := splitHeaderValue(part, ';')
	mediaTypePart := strings.TrimSpace(segments[0])
	if mediaTypePart == "" {
		return AcceptMediaRange{}, false
	}

	quality := 1.0
	mediaParams := make([]string, 0, len(segments)-1)
	for _, segment := range segments[1:] {
		name, value, ok := headerParameter(segment)
		if !ok {
			return AcceptMediaRange{}, false
		}
		if strings.EqualFold(name, "q") {
			parsed, ok := parseQuality(value)
			if !ok {
				return AcceptMediaRange{}, false
			}
			quality = parsed
			break
		}
		mediaParams = append(mediaParams, segment)
	}

	parseValue := mediaTypePart
	if len(mediaParams) > 0 {
		parseValue += ";" + strings.Join(mediaParams, ";")
	}
	mediaType, params, err := mime.ParseMediaType(parseValue)
	if err != nil {
		return AcceptMediaRange{}, false
	}

	typ, subtype, ok := splitMediaType(mediaType)
	if !ok || (typ == "*" && subtype != "*") {
		return AcceptMediaRange{}, false
	}
	return AcceptMediaRange{
		MediaType: typ + "/" + subtype,
		Type:      typ,
		Subtype:   subtype,
		Params:    params,
		Quality:   quality,
	}, true
}

func parseNegotiationOffer(offer string) (negotiationOffer, bool) {
	offer = strings.TrimSpace(offer)
	if offer == "" {
		return negotiationOffer{}, false
	}

	mediaType, params, err := mime.ParseMediaType(offer)
	if err != nil {
		return negotiationOffer{}, false
	}
	typ, subtype, ok := splitMediaType(mediaType)
	if !ok || typ == "*" || subtype == "*" {
		return negotiationOffer{}, false
	}
	return negotiationOffer{
		value:   offer,
		typ:     typ,
		subtype: subtype,
		params:  params,
	}, true
}

func bestRangeForOffer(ranges []AcceptMediaRange, offer negotiationOffer) (negotiationMatch, bool) {
	best := negotiationMatch{specificity: -1, quality: -1}
	for order, mediaRange := range ranges {
		specificity, ok := matchSpecificity(mediaRange, offer)
		if !ok {
			continue
		}

		match := negotiationMatch{
			quality:     mediaRange.Quality,
			specificity: specificity,
			params:      len(mediaRange.Params),
			rangeOrder:  order,
		}
		if !match.betterRangeThan(best) {
			continue
		}
		best = match
	}
	if best.specificity < 0 {
		return negotiationMatch{}, false
	}
	return best, true
}

func (m negotiationMatch) betterRangeThan(other negotiationMatch) bool {
	if m.specificity != other.specificity {
		return m.specificity > other.specificity
	}
	if m.params != other.params {
		return m.params > other.params
	}
	if m.quality != other.quality {
		return m.quality > other.quality
	}
	return m.rangeOrder < other.rangeOrder
}

func matchSpecificity(mediaRange AcceptMediaRange, offer negotiationOffer) (int, bool) {
	switch {
	case mediaRange.Type == "*" && mediaRange.Subtype == "*":
		if !mediaParamsMatch(mediaRange.Params, offer.params) {
			return 0, false
		}
		return 0, true
	case mediaRange.Type == offer.typ && mediaRange.Subtype == "*":
		if !mediaParamsMatch(mediaRange.Params, offer.params) {
			return 0, false
		}
		return 1, true
	case mediaRange.Type == offer.typ && mediaRange.Subtype == offer.subtype:
		if !mediaParamsMatch(mediaRange.Params, offer.params) {
			return 0, false
		}
		return 2, true
	default:
		return 0, false
	}
}

func mediaParamsMatch(want, got map[string]string) bool {
	for name, value := range want {
		if got[name] != value {
			return false
		}
	}
	return true
}

func splitMediaType(mediaType string) (string, string, bool) {
	typ, subtype, ok := strings.Cut(strings.ToLower(strings.TrimSpace(mediaType)), "/")
	if !ok || typ == "" || subtype == "" {
		return "", "", false
	}
	return typ, subtype, true
}

func parseQuality(value string) (float64, bool) {
	value = strings.Trim(strings.TrimSpace(value), `"`)
	if value == "" {
		return 0, false
	}
	quality, err := strconv.ParseFloat(value, 64)
	if err != nil || quality < 0 || quality > 1 || math.IsNaN(quality) {
		return 0, false
	}
	return quality, true
}

func headerParameter(segment string) (string, string, bool) {
	name, value, ok := strings.Cut(strings.TrimSpace(segment), "=")
	if !ok {
		return "", "", false
	}
	name = strings.TrimSpace(name)
	if name == "" {
		return "", "", false
	}
	return name, strings.TrimSpace(value), true
}

func splitHeaderValue(value string, separator byte) []string {
	var parts []string
	start := 0
	quoted := false
	escaped := false

	for i := 0; i < len(value); i++ {
		ch := value[i]
		if escaped {
			escaped = false
			continue
		}
		if quoted && ch == '\\' {
			escaped = true
			continue
		}
		if ch == '"' {
			quoted = !quoted
			continue
		}
		if !quoted && ch == separator {
			parts = append(parts, value[start:i])
			start = i + 1
		}
	}
	return append(parts, value[start:])
}
