package cache

import "testing"

func TestHashArgsMapOrderIsStable(t *testing.T) {
	left := map[string]any{"a": 1, "b": 2}
	right := map[string]any{"b": 2, "a": 1}

	leftHash, err := HashArgs(left)
	if err != nil {
		t.Fatalf("HashArgs(left) error = %v", err)
	}
	rightHash, err := HashArgs(right)
	if err != nil {
		t.Fatalf("HashArgs(right) error = %v", err)
	}

	const want = "43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777"
	if leftHash != want {
		t.Fatalf("HashArgs(left) = %q, want %q", leftHash, want)
	}
	if rightHash != leftHash {
		t.Fatalf("HashArgs(right) = %q, want %q", rightHash, leftHash)
	}
}

func TestHashArgsStructsAreStable(t *testing.T) {
	type args struct {
		Name  string `json:"name"`
		Limit int    `json:"limit"`
	}

	first, err := HashArgs(args{Name: "Ada", Limit: 20})
	if err != nil {
		t.Fatalf("HashArgs(first) error = %v", err)
	}
	second, err := HashArgs(args{Name: "Ada", Limit: 20})
	if err != nil {
		t.Fatalf("HashArgs(second) error = %v", err)
	}
	changed, err := HashArgs(args{Name: "Ada", Limit: 21})
	if err != nil {
		t.Fatalf("HashArgs(changed) error = %v", err)
	}

	if second != first {
		t.Fatalf("HashArgs(second) = %q, want %q", second, first)
	}
	if changed == first {
		t.Fatalf("HashArgs(changed) = %q, want a different hash", changed)
	}
}

func TestHashArgsReturnsJSONErrors(t *testing.T) {
	if _, err := HashArgs(make(chan int)); err == nil {
		t.Fatal("HashArgs(channel) error = nil, want error")
	}
}

func TestNormalizeNamespace(t *testing.T) {
	tests := []struct {
		name      string
		namespace string
		want      string
	}{
		{name: "empty", namespace: "  ", want: ""},
		{name: "lowercase", namespace: "Customer Reports | V2", want: "customer-reports-v2"},
		{name: "preserve safe separators", namespace: "billing_v2.cache", want: "billing_v2.cache"},
		{name: "collapse dashes", namespace: "customer---reports", want: "customer-reports"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := NormalizeNamespace(tt.namespace); got != tt.want {
				t.Fatalf("NormalizeNamespace(%q) = %q, want %q", tt.namespace, got, tt.want)
			}
		})
	}
}

func TestBuildKey(t *testing.T) {
	args := map[string]any{"b": 2, "a": 1}
	const argsHash = "43258cff783fe7036d8a43033f830adfc60ec037382473548ac742b888292777"

	key, err := BuildKey(KeyParts{
		Query: "customer.query.list",
		Args:  args,
	})
	if err != nil {
		t.Fatalf("BuildKey(default) error = %v", err)
	}
	want := "customer.query.list|-|" + argsHash
	if key != want {
		t.Fatalf("BuildKey(default) = %q, want %q", key, want)
	}

	key, err = BuildKey(KeyParts{
		Spec:   QuerySpec{Namespace: " Customer Reports "},
		Query:  " customer.query.list ",
		Tenant: " org-7 ",
		Args:   args,
	})
	if err != nil {
		t.Fatalf("BuildKey(namespaced) error = %v", err)
	}
	want = "customer.query.list|customer-reports|org-7|" + argsHash
	if key != want {
		t.Fatalf("BuildKey(namespaced) = %q, want %q", key, want)
	}

	key, err = BuildKey(KeyParts{
		Spec:      QuerySpec{Namespace: "from-spec"},
		Namespace: "override",
		Query:     "customer.query.list",
		Tenant:    "org-7",
		Args:      args,
	})
	if err != nil {
		t.Fatalf("BuildKey(override) error = %v", err)
	}
	want = "customer.query.list|override|org-7|" + argsHash
	if key != want {
		t.Fatalf("BuildKey(override) = %q, want %q", key, want)
	}
}

func TestBuildKeyRequiresQuery(t *testing.T) {
	if _, err := BuildKey(KeyParts{Args: map[string]any{}}); err == nil {
		t.Fatal("BuildKey(empty query) error = nil, want error")
	}
}

func TestBuildKeyReturnsArgHashError(t *testing.T) {
	_, err := BuildKey(KeyParts{
		Query: "customer.query.list",
		Args:  make(chan int),
	})
	if err == nil {
		t.Fatal("BuildKey(channel args) error = nil, want error")
	}
}
