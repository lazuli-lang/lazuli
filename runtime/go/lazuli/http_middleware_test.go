package lazuli

import (
	"net/http"
	"net/http/httptest"
	"reflect"
	"testing"
)

func TestChainAppliesMiddlewaresInDeclarationOrder(t *testing.T) {
	var order []string
	middleware := func(name string) Middleware {
		return func(next http.Handler) http.Handler {
			return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				order = append(order, name+":enter")
				next.ServeHTTP(w, r)
				order = append(order, name+":exit")
			})
		}
	}

	handler := Chain(
		middleware("first"),
		middleware("second"),
		middleware("third"),
	)(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		order = append(order, "handler")
	}))

	handler.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/", nil))

	want := []string{
		"first:enter",
		"second:enter",
		"third:enter",
		"handler",
		"third:exit",
		"second:exit",
		"first:exit",
	}
	if !reflect.DeepEqual(order, want) {
		t.Fatalf("order = %v, want %v", order, want)
	}
}
