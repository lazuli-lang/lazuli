package waf

import (
	"net/http/httptest"
	"testing"
)

func TestNoopFilterAllowsAll(t *testing.T) {
	f := NoopFilter{}
	req := httptest.NewRequest("GET", "/", nil)
	d, err := f.Inspect(req.Context(), req)
	if err != nil || d != Allow {
		t.Fatalf("NoopFilter must Allow; got (%v, %v)", d, err)
	}
}
