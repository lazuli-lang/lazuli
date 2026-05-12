package hateoas

import (
	"encoding/json"
	"net/http"
	"strings"
	"testing"
)

func TestLinksMarshalRelationKeyedObject(t *testing.T) {
	links := NewLinks(
		Self("/customers/7"),
		Link{Rel: "ignored"},
		Link{Href: "/missing-rel"},
		Link{Rel: RelNext, Href: "/customers?offset=20", Method: http.MethodGet, Type: "application/json", Title: "next page"},
		Link{Rel: RelSelf, Href: "/customers/7?view=full", Method: http.MethodGet},
	)

	body := decodeLinks(t, links)
	if len(body) != 2 {
		t.Fatalf("link count = %d, want 2", len(body))
	}
	if got := body[RelSelf]["href"]; got != "/customers/7?view=full" {
		t.Fatalf("self href = %v, want /customers/7?view=full", got)
	}
	if got := body[RelSelf]["method"]; got != http.MethodGet {
		t.Fatalf("self method = %v, want %s", got, http.MethodGet)
	}
	if _, ok := body[RelSelf]["rel"]; ok {
		t.Fatal("self link encoded rel field")
	}
	if got := body[RelNext]["type"]; got != "application/json" {
		t.Fatalf("next type = %v, want application/json", got)
	}
	if got := body[RelNext]["title"]; got != "next page" {
		t.Fatalf("next title = %v, want next page", got)
	}
}

func TestCommonLinkHelpers(t *testing.T) {
	tests := []struct {
		name   string
		link   Link
		rel    string
		method string
	}{
		{name: "self", link: Self("/orders/9"), rel: RelSelf, method: http.MethodGet},
		{name: "next", link: Next("/orders?offset=20"), rel: RelNext, method: http.MethodGet},
		{name: "prev", link: Prev("/orders?offset=0"), rel: RelPrev, method: http.MethodGet},
		{name: "create", link: Create("/orders"), rel: RelCreate, method: http.MethodPost},
		{name: "update", link: Update("/orders/9"), rel: RelUpdate, method: http.MethodPatch},
		{name: "delete", link: Delete("/orders/9"), rel: RelDelete, method: http.MethodDelete},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.link.Rel != tt.rel {
				t.Fatalf("Rel = %q, want %q", tt.link.Rel, tt.rel)
			}
			if tt.link.Method != tt.method {
				t.Fatalf("Method = %q, want %q", tt.link.Method, tt.method)
			}
		})
	}
}

func TestResourceMarshalShape(t *testing.T) {
	resource := NewResource(customer{ID: 7, Name: "Ada"}, NewLinks(
		Self("/customers/7"),
		Update("/customers/7"),
		Delete("/customers/7"),
	))

	var body map[string]any
	marshalInto(t, resource, &body)

	data, ok := body["data"].(map[string]any)
	if !ok {
		t.Fatalf("data = %#v, want object", body["data"])
	}
	if data["id"] != float64(7) || data["name"] != "Ada" {
		t.Fatalf("data = %#v, want customer", data)
	}

	links := body["_links"].(map[string]any)
	self := links[RelSelf].(map[string]any)
	if self["href"] != "/customers/7" {
		t.Fatalf("self href = %#v, want /customers/7", self["href"])
	}
	update := links[RelUpdate].(map[string]any)
	if update["method"] != http.MethodPatch {
		t.Fatalf("update method = %#v, want %s", update["method"], http.MethodPatch)
	}
}

func TestCollectionMarshalShape(t *testing.T) {
	collection := NewCollection([]customer{
		{ID: 7, Name: "Ada"},
		{ID: 8, Name: "Grace"},
	}, NewLinks(Self("/customers"), Create("/customers")))

	var body map[string]any
	marshalInto(t, collection, &body)

	data, ok := body["data"].([]any)
	if !ok {
		t.Fatalf("data = %#v, want array", body["data"])
	}
	if len(data) != 2 {
		t.Fatalf("data length = %d, want 2", len(data))
	}

	links := body["_links"].(map[string]any)
	create := links[RelCreate].(map[string]any)
	if create["method"] != http.MethodPost {
		t.Fatalf("create method = %#v, want %s", create["method"], http.MethodPost)
	}
}

func TestCollectionLinksBuildPaginationLinks(t *testing.T) {
	links, err := CollectionLinks(Pagination{
		BaseURL: "/customers?active=true",
		Limit:   20,
		Offset:  40,
		HasNext: true,
	})
	if err != nil {
		t.Fatalf("CollectionLinks error = %v", err)
	}

	body := decodeLinks(t, links)
	if got := body[RelSelf]["href"]; got != "/customers?active=true&limit=20&offset=40" {
		t.Fatalf("self href = %v, want current page", got)
	}
	if got := body[RelPrev]["href"]; got != "/customers?active=true&limit=20&offset=20" {
		t.Fatalf("prev href = %v, want previous page", got)
	}
	if got := body[RelNext]["href"]; got != "/customers?active=true&limit=20&offset=60" {
		t.Fatalf("next href = %v, want next page", got)
	}
}

func TestCollectionLinksUsesCustomPaginationParamNames(t *testing.T) {
	links, err := CollectionLinks(Pagination{
		BaseURL:     "/customers",
		Limit:       50,
		Offset:      0,
		LimitParam:  "page_size",
		OffsetParam: "start",
	})
	if err != nil {
		t.Fatalf("CollectionLinks error = %v", err)
	}

	body := decodeLinks(t, links)
	if got := body[RelSelf]["href"]; got != "/customers?page_size=50&start=0" {
		t.Fatalf("self href = %v, want custom param names", got)
	}
	if _, ok := body[RelPrev]; ok {
		t.Fatal("prev link present for first page")
	}
	if _, ok := body[RelNext]; ok {
		t.Fatal("next link present when HasNext is false")
	}
}

func TestCollectionLinksValidatesPagination(t *testing.T) {
	tests := []Pagination{
		{BaseURL: "/customers", Limit: 0},
		{BaseURL: "/customers", Limit: 10, Offset: -1},
		{BaseURL: "/customers/../admin", Limit: 10},
	}

	for _, page := range tests {
		if _, err := CollectionLinks(page); err == nil {
			t.Fatalf("CollectionLinks(%+v) error = nil, want error", page)
		}
	}
}

func TestJoinURL(t *testing.T) {
	tests := []struct {
		name     string
		base     string
		segments []string
		want     string
	}{
		{
			name:     "absolute path",
			base:     "/api",
			segments: []string{"customers", "42"},
			want:     "/api/customers/42",
		},
		{
			name:     "absolute URL with query and fragment",
			base:     "https://api.example.test/v1/?active=true#section",
			segments: []string{"/customers/", "Ada Lovelace"},
			want:     "https://api.example.test/v1/customers/Ada%20Lovelace?active=true#section",
		},
		{
			name:     "relative path",
			base:     "api",
			segments: []string{"customers"},
			want:     "api/customers",
		},
		{
			name: "root",
			base: "/",
			want: "/",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := JoinURL(tt.base, tt.segments...)
			if err != nil {
				t.Fatalf("JoinURL error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("JoinURL = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestJoinURLRejectsUnsafeInput(t *testing.T) {
	tests := []struct {
		name     string
		base     string
		segments []string
	}{
		{name: "empty base"},
		{name: "base traversal", base: "/api/../admin"},
		{name: "segment traversal", base: "/api", segments: []string{"../admin"}},
		{name: "backslash", base: "/api", segments: []string{`customers\42`}},
		{name: "query delimiter in segment", base: "/api", segments: []string{"customers?active=true"}},
		{name: "opaque URL", base: "mailto:admin@example.test"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got, err := JoinURL(tt.base, tt.segments...); err == nil {
				t.Fatalf("JoinURL = %q, error = nil, want error", got)
			}
		})
	}
}

type customer struct {
	ID   int    `json:"id"`
	Name string `json:"name"`
}

func decodeLinks(t *testing.T, links Links) map[string]map[string]any {
	t.Helper()

	var body map[string]map[string]any
	marshalInto(t, links, &body)
	return body
}

func marshalInto(t *testing.T, value any, out any) {
	t.Helper()

	data, err := json.Marshal(value)
	if err != nil {
		t.Fatalf("Marshal error = %v", err)
	}
	if strings.Contains(string(data), `"rel"`) {
		t.Fatalf("encoded JSON contains rel field: %s", data)
	}
	if err := json.Unmarshal(data, out); err != nil {
		t.Fatalf("Unmarshal error = %v; JSON = %s", err, data)
	}
}
