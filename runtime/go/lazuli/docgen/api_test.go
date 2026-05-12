package docgen

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestMarkdownGroupsByFeatureAndSortsRoutes(t *testing.T) {
	routes := []Route{
		{
			Name:    "billing.refund",
			Method:  "post",
			Path:    "/billing/refunds",
			Feature: "billing",
			Tags:    []string{"write"},
			Summary: "Issue refund",
		},
		{
			Name:    "accounts.show",
			Method:  "GET",
			Path:    "/accounts/{id}",
			Feature: "accounts",
			Tags:    []string{"read", "accounts"},
			Summary: "Fetch account",
		},
		{
			Name:    "billing.list",
			Method:  "GET",
			Path:    "/billing/invoices",
			Feature: "billing",
			Tags:    []string{"read", "billing"},
			Summary: "List | invoices",
		},
	}

	got, err := Markdown(routes, MarkdownOptions{Title: "Reference", GroupBy: GroupByFeature})
	if err != nil {
		t.Fatalf("Markdown() error = %v", err)
	}

	const want = `# Reference

## Feature: accounts

| Method | Path | Name | Feature | Tags | Summary |
| --- | --- | --- | --- | --- | --- |
| GET | /accounts/{id} | accounts.show | accounts | accounts, read | Fetch account |

## Feature: billing

| Method | Path | Name | Feature | Tags | Summary |
| --- | --- | --- | --- | --- | --- |
| GET | /billing/invoices | billing.list | billing | billing, read | List \| invoices |
| POST | /billing/refunds | billing.refund | billing | write | Issue refund |
`
	if got != want {
		t.Fatalf("Markdown() mismatch\nwant:\n%s\ngot:\n%s", want, got)
	}
}

func TestMarkdownGroupsByTag(t *testing.T) {
	routes := []Route{
		{
			Name:    "customers.create",
			Method:  "POST",
			Path:    "/customers",
			Feature: "crm",
			Tags:    []string{"write", "crm"},
			Summary: "Create customer",
		},
		{
			Name:    "health",
			Method:  "GET",
			Path:    "/healthz",
			Summary: "Health check",
		},
	}

	got, err := Markdown(routes, MarkdownOptions{GroupBy: GroupByTag})
	if err != nil {
		t.Fatalf("Markdown() error = %v", err)
	}

	const want = `# API Reference

## Tag: crm

| Method | Path | Name | Feature | Tags | Summary |
| --- | --- | --- | --- | --- | --- |
| POST | /customers | customers.create | crm | crm, write | Create customer |

## Tag: Untagged

| Method | Path | Name | Feature | Tags | Summary |
| --- | --- | --- | --- | --- | --- |
| GET | /healthz | health |  |  | Health check |

## Tag: write

| Method | Path | Name | Feature | Tags | Summary |
| --- | --- | --- | --- | --- | --- |
| POST | /customers | customers.create | crm | crm, write | Create customer |
`
	if got != want {
		t.Fatalf("Markdown() mismatch\nwant:\n%s\ngot:\n%s", want, got)
	}
}

func TestSortedRoutesNormalizesWithoutMutatingInput(t *testing.T) {
	routes := []Route{
		{
			Name:    "beta",
			Method:  "post",
			Path:    "/beta",
			Feature: "beta",
			Tags:    []string{"Write", "read", "write"},
		},
		{
			Name:   "alpha",
			Method: "get",
			Path:   "/alpha",
		},
	}

	got, err := SortedRoutes(routes)
	if err != nil {
		t.Fatalf("SortedRoutes() error = %v", err)
	}

	want := []Route{
		{Name: "alpha", Method: "GET", Path: "/alpha"},
		{Name: "beta", Method: "POST", Path: "/beta", Feature: "beta", Tags: []string{"read", "Write"}},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("SortedRoutes() = %#v, want %#v", got, want)
	}

	if routes[0].Method != "post" || !reflect.DeepEqual(routes[0].Tags, []string{"Write", "read", "write"}) {
		t.Fatalf("SortedRoutes() mutated input: %#v", routes[0])
	}
}

func TestValidateRoutesRejectsInvalidAndDuplicateMetadata(t *testing.T) {
	routes := []Route{
		{Name: "a", Method: "GET", Path: "/same"},
		{Name: "b", Method: "get", Path: "/same"},
		{Name: "bad", Method: "TRACE", Path: "relative", Tags: []string{"ok", " "}},
	}

	err := ValidateRoutes(routes)
	if !errors.Is(err, ErrDuplicateRoute) {
		t.Fatalf("ValidateRoutes() error = %v, want ErrDuplicateRoute", err)
	}
	if !errors.Is(err, ErrInvalidRoute) {
		t.Fatalf("ValidateRoutes() error = %v, want ErrInvalidRoute", err)
	}
	for _, want := range []string{"route[1] GET /same", "route[2].method", "route[2].path", "route[2].tags[1]"} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("ValidateRoutes() error = %q, want substring %q", err.Error(), want)
		}
	}
}

func TestMarkdownRejectsInvalidOptions(t *testing.T) {
	_, err := Markdown(nil, MarkdownOptions{GroupBy: "owner"})
	if !errors.Is(err, ErrInvalidOptions) {
		t.Fatalf("Markdown() error = %v, want ErrInvalidOptions", err)
	}
}
