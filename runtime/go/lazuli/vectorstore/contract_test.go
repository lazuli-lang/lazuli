package vectorstore

import (
	"context"
	"errors"
	"testing"
)

func TestTypedErrorsAreDistinct(t *testing.T) {
	errs := []error{
		ErrVectorStoreUnbound,
		ErrEmbedderUnbound,
		ErrCollectionNotFound,
		ErrDimensionMismatch,
		ErrVectorStoreUnauthorized,
		ErrVectorStoreRateLimited,
		ErrVectorStoreUnavailable,
	}
	for i, a := range errs {
		for j, b := range errs {
			if i == j {
				continue
			}
			if errors.Is(a, b) {
				t.Errorf("errs[%d] %q reports Is errs[%d] %q — must be distinct", i, a, j, b)
			}
		}
	}
}

func TestItemZeroValue(t *testing.T) {
	var it Item
	if it.ID != "" || it.Vector != nil || it.Document != "" || it.Metadata != nil {
		t.Errorf("zero Item drifted: %#v", it)
	}
}

func TestVectorFilterZeroValue(t *testing.T) {
	var f VectorFilter
	if f.Tags != nil || f.Equals != nil || f.Raw != nil {
		t.Errorf("zero VectorFilter drifted: %#v", f)
	}
}

// fakeStore is the smallest VectorStore implementation that satisfies
// the contract. Exercises the interface shape: a mismatched signature
// makes this file fail to compile.
type fakeStore struct{}

func (fakeStore) Provider() string                  { return "@lazuli/plugin-test" }
func (fakeStore) Collection(name string) Collection { return fakeCollection{} }

type fakeCollection struct{}

func (fakeCollection) Add(_ context.Context, _ []Item) error  { return nil }
func (fakeCollection) Delete(_ context.Context, _ []string) error { return nil }
func (fakeCollection) Get(_ context.Context, _ []string) ([]Item, error) {
	return nil, nil
}
func (fakeCollection) Update(_ context.Context, _ []Item) error { return nil }
func (fakeCollection) QueryByVector(_ context.Context, _ []float32, _ VectorQuery) ([]Match, error) {
	return nil, nil
}
func (fakeCollection) QueryByText(_ context.Context, _ string, _ VectorQuery) ([]Match, error) {
	return nil, nil
}

func TestVectorStoreInterfaceShape(t *testing.T) {
	var s VectorStore = fakeStore{}
	if s.Provider() != "@lazuli/plugin-test" {
		t.Errorf("Provider mismatch: %q", s.Provider())
	}
	c := s.Collection("smoke")
	if err := c.Add(context.Background(), nil); err != nil {
		t.Errorf("Add: %v", err)
	}
	if err := c.Delete(context.Background(), nil); err != nil {
		t.Errorf("Delete: %v", err)
	}
}

// fakeEmbedder is the smallest Embedder implementation.
type fakeEmbedder struct{}

func (fakeEmbedder) Provider() string { return "@lazuli/plugin-test-embedder" }
func (fakeEmbedder) Embed(_ context.Context, _ string) ([]float32, error) {
	return []float32{0.1, 0.2, 0.3}, nil
}
func (fakeEmbedder) EmbedBatch(_ context.Context, texts []string) ([][]float32, error) {
	out := make([][]float32, len(texts))
	for i := range out {
		out[i] = []float32{0.1, 0.2, 0.3}
	}
	return out, nil
}
func (fakeEmbedder) DimensionHint() int { return 3 }

func TestEmbedderInterfaceShape(t *testing.T) {
	var e Embedder = fakeEmbedder{}
	if e.DimensionHint() != 3 {
		t.Errorf("DimensionHint = %d, want 3", e.DimensionHint())
	}
	v, err := e.Embed(context.Background(), "hi")
	if err != nil {
		t.Errorf("Embed: %v", err)
	}
	if len(v) != 3 {
		t.Errorf("Embed dim = %d, want 3", len(v))
	}
}
