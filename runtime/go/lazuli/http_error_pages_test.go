package lazuli

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestWriteHTTPStatusErrorUsesHTMLRenderer(t *testing.T) {
	var gotProblem Problem
	renderer := HTTPErrorPageRendererFunc(func(r *http.Request, problem Problem) (HTTPErrorPage, error) {
		if r.URL.Path != "/missing" {
			t.Fatalf("renderer path = %q, want /missing", r.URL.Path)
		}
		gotProblem = problem
		return HTTPErrorPage{
			ContentType: contentTypeHTML,
			Body:        []byte("<!doctype html><h1>" + problem.Title + "</h1>"),
		}, nil
	})

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/missing", nil)
	req.Header.Set("Accept", "text/html")

	WriteHTTPStatusError(rec, req, http.StatusNotFound, renderer)

	if rec.Code != http.StatusNotFound {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNotFound)
	}
	if got := rec.Header().Get("Content-Type"); got != contentTypeHTML {
		t.Fatalf("Content-Type = %q, want %q", got, contentTypeHTML)
	}
	if got := rec.Header().Get("Vary"); got != "Accept" {
		t.Fatalf("Vary = %q, want Accept", got)
	}
	if gotProblem.Status != http.StatusNotFound || gotProblem.Title != http.StatusText(http.StatusNotFound) {
		t.Fatalf("renderer problem = %+v, want normalized 404 problem", gotProblem)
	}
	if got := rec.Body.String(); !strings.Contains(got, "<h1>Not Found</h1>") {
		t.Fatalf("body = %q, want rendered HTML", got)
	}
}

func TestWriteHTTPStatusErrorFallsBackToProblemJSON(t *testing.T) {
	called := false
	renderer := HTTPErrorPageRendererFunc(func(*http.Request, Problem) (HTTPErrorPage, error) {
		called = true
		return HTTPErrorPage{}, nil
	})

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/api/missing", nil)
	req.Header.Set("Accept", "application/json")

	WriteHTTPStatusError(rec, req, http.StatusNotFound, renderer)

	if called {
		t.Fatal("renderer was called for JSON request")
	}
	if rec.Code != http.StatusNotFound {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNotFound)
	}
	if got := rec.Header().Get("Content-Type"); got != contentTypeProblemJSON {
		t.Fatalf("Content-Type = %q, want %q", got, contentTypeProblemJSON)
	}

	var problem Problem
	if err := json.NewDecoder(rec.Body).Decode(&problem); err != nil {
		t.Fatalf("decode problem JSON: %v; body = %q", err, rec.Body.String())
	}
	if problem.Type != defaultProblemType {
		t.Fatalf("Type = %q, want %q", problem.Type, defaultProblemType)
	}
	if problem.Status != http.StatusNotFound {
		t.Fatalf("Status = %d, want %d", problem.Status, http.StatusNotFound)
	}
	if problem.Title != http.StatusText(http.StatusNotFound) {
		t.Fatalf("Title = %q, want %q", problem.Title, http.StatusText(http.StatusNotFound))
	}
}

func TestWriteHTTPProblemFallsBackToText(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/gone", nil)
	req.Header.Set("Accept", "text/plain")

	WriteHTTPProblem(rec, req, Problem{
		Status: http.StatusGone,
		Detail: "that page moved",
	}, nil)

	if rec.Code != http.StatusGone {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusGone)
	}
	if got := rec.Header().Get("Content-Type"); got != contentTypePlainText {
		t.Fatalf("Content-Type = %q, want %q", got, contentTypePlainText)
	}
	if got := rec.Body.String(); got != "that page moved\n" {
		t.Fatalf("body = %q, want text detail", got)
	}
}

func TestWriteHTTPStatusErrorFallsBackWhenRendererFails(t *testing.T) {
	renderer := HTTPErrorPageRendererFunc(func(*http.Request, Problem) (HTTPErrorPage, error) {
		return HTTPErrorPage{}, errors.New("template missing")
	})

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/missing", nil)
	req.Header.Set("Accept", "text/html")

	WriteHTTPStatusError(rec, req, http.StatusInternalServerError, renderer)

	if rec.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusInternalServerError)
	}
	if got := rec.Header().Get("Content-Type"); got != contentTypePlainText {
		t.Fatalf("Content-Type = %q, want %q", got, contentTypePlainText)
	}
	if got := rec.Body.String(); got != "Internal Server Error\n" {
		t.Fatalf("body = %q, want text fallback", got)
	}
}
