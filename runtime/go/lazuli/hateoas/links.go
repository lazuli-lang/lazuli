// Package hateoas provides small helpers for adding hypermedia links to JSON
// API responses without tying handlers to a specific API provider.
package hateoas

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strconv"
	"strings"
)

const (
	// RelSelf is the relation name for a resource's canonical URL.
	RelSelf = "self"

	// RelNext is the relation name for the next page in a collection.
	RelNext = "next"

	// RelPrev is the relation name for the previous page in a collection.
	RelPrev = "prev"

	// RelCreate is the relation name for a collection create endpoint.
	RelCreate = "create"

	// RelUpdate is the relation name for a resource update endpoint.
	RelUpdate = "update"

	// RelDelete is the relation name for a resource delete endpoint.
	RelDelete = "delete"
)

const (
	defaultLimitParam  = "limit"
	defaultOffsetParam = "offset"
)

// Link describes one hypermedia affordance.
//
// Rel identifies the relation used when a Link is encoded inside Links. It is
// not emitted inside the link object because the relation is the JSON object
// key. Method is optional, but the helper constructors set the conventional
// HTTP method for common CRUD affordances.
type Link struct {
	Rel    string `json:"-"`
	Href   string `json:"href"`
	Method string `json:"method,omitempty"`
	Type   string `json:"type,omitempty"`
	Title  string `json:"title,omitempty"`
}

// Links is an ordered set of links.
//
// When marshaled to JSON, links are encoded as an object keyed by relation:
// {"self":{"href":"/items"}}. Empty relations and hrefs are skipped. If the
// same relation appears more than once, the last link wins.
type Links []Link

// NewLinks returns a Links value containing links with non-empty rels and hrefs.
func NewLinks(links ...Link) Links {
	var out Links
	for _, link := range links {
		out = out.With(link)
	}
	return out
}

// With appends link when it has both a relation and href.
func (links Links) With(link Link) Links {
	if strings.TrimSpace(link.Rel) == "" || strings.TrimSpace(link.Href) == "" {
		return links
	}
	return append(links, link)
}

// MarshalJSON encodes links as a relation-keyed object.
func (links Links) MarshalJSON() ([]byte, error) {
	object := make(map[string]Link, len(links))
	for _, link := range links {
		rel := strings.TrimSpace(link.Rel)
		href := strings.TrimSpace(link.Href)
		if rel == "" || href == "" {
			continue
		}
		link.Rel = ""
		link.Href = href
		link.Method = strings.TrimSpace(link.Method)
		link.Type = strings.TrimSpace(link.Type)
		link.Title = strings.TrimSpace(link.Title)
		object[rel] = link
	}
	return json.Marshal(object)
}

// Resource wraps one response entity with hypermedia links.
type Resource[T any] struct {
	Data  T
	Links Links
}

// NewResource returns a linked single-entity response wrapper.
func NewResource[T any](data T, links Links) Resource[T] {
	return Resource[T]{Data: data, Links: links}
}

// MarshalJSON encodes r as {"data":...,"_links":{...}}.
func (r Resource[T]) MarshalJSON() ([]byte, error) {
	type resource struct {
		Data  T     `json:"data"`
		Links Links `json:"_links,omitempty"`
	}
	return json.Marshal(resource{
		Data:  r.Data,
		Links: r.Links,
	})
}

// Collection wraps a response collection with hypermedia links.
type Collection[T any] struct {
	Items []T
	Links Links
}

// NewCollection returns a linked collection response wrapper.
func NewCollection[T any](items []T, links Links) Collection[T] {
	return Collection[T]{Items: items, Links: links}
}

// MarshalJSON encodes c as {"data":[...],"_links":{...}}.
func (c Collection[T]) MarshalJSON() ([]byte, error) {
	type collection struct {
		Data  []T   `json:"data"`
		Links Links `json:"_links,omitempty"`
	}
	return json.Marshal(collection{
		Data:  c.Items,
		Links: c.Links,
	})
}

// Pagination describes an offset-paginated collection endpoint.
//
// BaseURL may be an absolute URL or a relative API path. Existing query
// parameters are preserved. LimitParam and OffsetParam default to "limit" and
// "offset". HasNext controls whether a next link is emitted so callers can use
// either a total count or a "limit + 1" fetch strategy.
type Pagination struct {
	BaseURL     string
	Limit       int
	Offset      int
	HasNext     bool
	LimitParam  string
	OffsetParam string
}

// CollectionLinks returns self, prev, and next links for an offset-paginated
// collection.
func CollectionLinks(page Pagination) (Links, error) {
	if page.Limit <= 0 {
		return nil, fmt.Errorf("lazuli/hateoas: pagination limit must be greater than zero")
	}
	if page.Offset < 0 {
		return nil, fmt.Errorf("lazuli/hateoas: pagination offset must not be negative")
	}

	self, err := paginationHref(page, page.Offset)
	if err != nil {
		return nil, err
	}

	links := NewLinks(Self(self))
	if page.Offset > 0 {
		prevOffset := page.Offset - page.Limit
		if prevOffset < 0 {
			prevOffset = 0
		}
		prev, err := paginationHref(page, prevOffset)
		if err != nil {
			return nil, err
		}
		links = links.With(Prev(prev))
	}
	if page.HasNext {
		next, err := paginationHref(page, page.Offset+page.Limit)
		if err != nil {
			return nil, err
		}
		links = links.With(Next(next))
	}
	return links, nil
}

// LinkTo returns a link for rel, method, and href.
func LinkTo(rel, method, href string) Link {
	return Link{
		Rel:    rel,
		Href:   href,
		Method: method,
	}
}

// Self returns a GET self link.
func Self(href string) Link {
	return LinkTo(RelSelf, http.MethodGet, href)
}

// Next returns a GET next-page link.
func Next(href string) Link {
	return LinkTo(RelNext, http.MethodGet, href)
}

// Prev returns a GET previous-page link.
func Prev(href string) Link {
	return LinkTo(RelPrev, http.MethodGet, href)
}

// Create returns a POST create link.
func Create(href string) Link {
	return LinkTo(RelCreate, http.MethodPost, href)
}

// Update returns a PATCH update link.
func Update(href string) Link {
	return LinkTo(RelUpdate, http.MethodPatch, href)
}

// Delete returns a DELETE link.
func Delete(href string) Link {
	return LinkTo(RelDelete, http.MethodDelete, href)
}

// JoinURL joins base with path segments while rejecting path traversal and
// other unsafe path input.
//
// Base may be an absolute URL or a relative API path. Query strings and
// fragments on base are preserved. Segments may contain slash-separated path
// fragments, but "." and ".." path components, backslashes, and control
// characters are rejected.
func JoinURL(base string, segments ...string) (string, error) {
	base = strings.TrimSpace(base)
	if base == "" {
		return "", fmt.Errorf("lazuli/hateoas: URL base is required")
	}
	if hasUnsafeURLText(base) {
		return "", fmt.Errorf("lazuli/hateoas: URL contains unsafe characters")
	}

	u, err := url.Parse(base)
	if err != nil {
		return "", fmt.Errorf("lazuli/hateoas: parse URL: %w", err)
	}
	if u.Opaque != "" {
		return "", fmt.Errorf("lazuli/hateoas: opaque URLs cannot be joined safely")
	}

	parts, absolute, err := safePathParts(u.EscapedPath())
	if err != nil {
		return "", err
	}
	if u.Host != "" {
		absolute = true
	}

	for _, segment := range segments {
		if hasUnsafeURLText(segment) {
			return "", fmt.Errorf("lazuli/hateoas: URL segment contains unsafe characters")
		}
		segmentParts, _, err := safePathParts(segment)
		if err != nil {
			return "", err
		}
		parts = append(parts, segmentParts...)
	}

	u.Path = joinedPath(parts, absolute)
	u.RawPath = ""
	return u.String(), nil
}

func paginationHref(page Pagination, offset int) (string, error) {
	href, err := JoinURL(page.BaseURL)
	if err != nil {
		return "", err
	}
	u, err := url.Parse(href)
	if err != nil {
		return "", fmt.Errorf("lazuli/hateoas: parse pagination URL: %w", err)
	}

	limitParam := strings.TrimSpace(page.LimitParam)
	if limitParam == "" {
		limitParam = defaultLimitParam
	}
	offsetParam := strings.TrimSpace(page.OffsetParam)
	if offsetParam == "" {
		offsetParam = defaultOffsetParam
	}

	q := u.Query()
	q.Set(limitParam, strconv.Itoa(page.Limit))
	q.Set(offsetParam, strconv.Itoa(offset))
	u.RawQuery = q.Encode()
	return u.String(), nil
}

func safePathParts(rawPath string) ([]string, bool, error) {
	decoded, err := url.PathUnescape(rawPath)
	if err != nil {
		return nil, false, fmt.Errorf("lazuli/hateoas: invalid URL path escape: %w", err)
	}

	absolute := strings.HasPrefix(decoded, "/")
	var parts []string
	for _, part := range strings.Split(decoded, "/") {
		if part == "" {
			continue
		}
		switch part {
		case ".", "..":
			return nil, false, fmt.Errorf("lazuli/hateoas: URL path must not contain %q", part)
		}
		if strings.ContainsAny(part, "?#") {
			return nil, false, fmt.Errorf("lazuli/hateoas: URL path segment contains query or fragment delimiters")
		}
		parts = append(parts, part)
	}
	return parts, absolute, nil
}

func joinedPath(parts []string, absolute bool) string {
	joined := strings.Join(parts, "/")
	if absolute {
		return "/" + joined
	}
	return joined
}

func hasUnsafeURLText(value string) bool {
	if strings.ContainsAny(value, "\x00\\") {
		return true
	}
	for _, r := range value {
		if r < 0x20 || r == 0x7f {
			return true
		}
	}
	return false
}
