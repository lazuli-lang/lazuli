package vectorstore

import (
	"errors"
	"testing"
)

func TestErrorSentinelsAreDistinct(t *testing.T) {
	sentinels := []error{
		ErrVectorStoreUnbound,
		ErrEmbedderUnbound,
		ErrCollectionNotFound,
		ErrDimensionMismatch,
		ErrVectorStoreUnauthorized,
		ErrVectorStoreRateLimited,
		ErrVectorStoreUnavailable,
	}
	for i := range sentinels {
		for j := range sentinels {
			if i == j {
				continue
			}
			if errors.Is(sentinels[i], sentinels[j]) {
				t.Errorf("sentinel[%d] wraps sentinel[%d]: %v == %v",
					i, j, sentinels[i], sentinels[j])
			}
		}
	}
}

// TestVectorQueryDefaultLimit documents that VectorQuery.Limit == 0
// is the zero value; adapters interpret 0 as "use their own default".
func TestVectorQueryDefaultLimit(t *testing.T) {
	q := VectorQuery{}
	if q.Limit != 0 {
		t.Errorf("expected zero Limit, got %d", q.Limit)
	}
}
