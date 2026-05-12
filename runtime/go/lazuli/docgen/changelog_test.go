package docgen

import (
	"errors"
	"reflect"
	"testing"
)

func TestCompareRouteSnapshotsDetectsAndClassifiesChanges(t *testing.T) {
	before := []Route{
		{
			Name:    "accounts.show",
			Method:  "get",
			Path:    "/accounts/{id}",
			Feature: "accounts",
			Tags:    []string{"read"},
			Summary: "Fetch account",
		},
		{
			Name:    "accounts.delete",
			Method:  "DELETE",
			Path:    "/accounts/{id}",
			Feature: "accounts",
			Summary: "Delete account",
		},
		{
			Name:   "health",
			Method: "GET",
			Path:   "/healthz",
		},
		{
			Name:    "billing.list",
			Method:  "GET",
			Path:    "/billing/invoices",
			Feature: "billing",
			Summary: "List invoices",
		},
	}
	after := []Route{
		{
			Name:    "accounts.show",
			Method:  "GET",
			Path:    "/accounts/{id}",
			Feature: "crm",
			Tags:    []string{"accounts", "read"},
			Summary: "Fetch account profile",
		},
		{
			Name:    "accounts.create",
			Method:  "POST",
			Path:    "/accounts",
			Feature: "accounts",
			Summary: "Create account",
		},
		{
			Name:   "system.health",
			Method: "GET",
			Path:   "/healthz",
		},
		{
			Name:    "billing.list",
			Method:  "GET",
			Path:    "/billing/invoices",
			Feature: "billing",
			Summary: "List invoices",
		},
	}

	got, err := CompareRouteSnapshots(before, after)
	if err != nil {
		t.Fatalf("CompareRouteSnapshots() error = %v", err)
	}

	want := RouteChangelog{
		Added: []RouteChange{
			{
				Kind:   RouteChangeAdded,
				Impact: ChangeImpactNonBreaking,
				After: Route{
					Name:    "accounts.create",
					Method:  "POST",
					Path:    "/accounts",
					Feature: "accounts",
					Summary: "Create account",
				},
			},
		},
		Removed: []RouteChange{
			{
				Kind:   RouteChangeRemoved,
				Impact: ChangeImpactBreaking,
				Before: Route{
					Name:    "accounts.delete",
					Method:  "DELETE",
					Path:    "/accounts/{id}",
					Feature: "accounts",
					Summary: "Delete account",
				},
			},
		},
		Changed: []RouteChange{
			{
				Kind:   RouteChangeChanged,
				Impact: ChangeImpactNonBreaking,
				Before: Route{
					Name:    "accounts.show",
					Method:  "GET",
					Path:    "/accounts/{id}",
					Feature: "accounts",
					Tags:    []string{"read"},
					Summary: "Fetch account",
				},
				After: Route{
					Name:    "accounts.show",
					Method:  "GET",
					Path:    "/accounts/{id}",
					Feature: "crm",
					Tags:    []string{"accounts", "read"},
					Summary: "Fetch account profile",
				},
				Fields: []RouteFieldChange{
					{Field: "Feature", Before: "accounts", After: "crm"},
					{Field: "Tags", Before: "read", After: "accounts, read"},
					{Field: "Summary", Before: "Fetch account", After: "Fetch account profile"},
				},
			},
			{
				Kind:   RouteChangeChanged,
				Impact: ChangeImpactBreaking,
				Before: Route{
					Name:   "health",
					Method: "GET",
					Path:   "/healthz",
				},
				After: Route{
					Name:   "system.health",
					Method: "GET",
					Path:   "/healthz",
				},
				Fields: []RouteFieldChange{
					{Field: "Name", Before: "health", After: "system.health"},
				},
			},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("CompareRouteSnapshots() = %#v, want %#v", got, want)
	}

	if !got.HasChanges() {
		t.Fatal("CompareRouteSnapshots() HasChanges() = false, want true")
	}
	if !got.HasBreakingChanges() {
		t.Fatal("CompareRouteSnapshots() HasBreakingChanges() = false, want true")
	}
	if got.Added[0].After.Method != "POST" || before[0].Method != "get" {
		t.Fatalf("CompareRouteSnapshots() did not normalize output or mutated input: got=%#v before=%#v", got.Added[0].After, before[0])
	}
}

func TestMarkdownChangelogRendersDeterministicSummary(t *testing.T) {
	changelog, err := CompareRouteSnapshots(
		[]Route{
			{Name: "users.get", Method: "GET", Path: "/users/{id}", Tags: []string{"read"}, Summary: "Fetch user"},
			{Name: "users.delete", Method: "DELETE", Path: "/users/{id}", Summary: "Delete user"},
		},
		[]Route{
			{Name: "users.show", Method: "GET", Path: "/users/{id}", Tags: []string{"users", "read"}, Summary: "Fetch | profile"},
			{Name: "users.create", Method: "POST", Path: "/users", Summary: "Create user"},
		},
	)
	if err != nil {
		t.Fatalf("CompareRouteSnapshots() error = %v", err)
	}

	got := MarkdownChangelog(changelog, ChangelogMarkdownOptions{Title: "Release 2026.05"})
	const want = `# Release 2026.05

| Impact | Count |
| --- | --- |
| Breaking | 2 |
| Non-breaking | 1 |

## Breaking Changes

| Type | Method | Path | Name | Details |
| --- | --- | --- | --- | --- |
| Removed | DELETE | /users/{id} | users.delete | Route removed |
| Changed | GET | /users/{id} | users.show | Name: users.get -> users.show<br>Tags: read -> read, users<br>Summary: Fetch user -> Fetch \| profile |

## Non-Breaking Changes

| Type | Method | Path | Name | Details |
| --- | --- | --- | --- | --- |
| Added | POST | /users | users.create | Route added |
`
	if got != want {
		t.Fatalf("MarkdownChangelog() mismatch\nwant:\n%s\ngot:\n%s", want, got)
	}
}

func TestCompareRouteSnapshotsRejectsInvalidSnapshots(t *testing.T) {
	_, err := CompareRouteSnapshots(
		[]Route{
			{Name: "ok", Method: "GET", Path: "/same"},
			{Name: "dupe", Method: "get", Path: "/same"},
		},
		nil,
	)
	if !errors.Is(err, ErrDuplicateRoute) {
		t.Fatalf("CompareRouteSnapshots() error = %v, want ErrDuplicateRoute", err)
	}
}
