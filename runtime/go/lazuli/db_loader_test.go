package lazuli

import (
	"context"
	"errors"
	"reflect"
	"testing"
)

func TestDBLoaderLoadManyBatchesMissingKeysAndCaches(t *testing.T) {
	batch := &dbLoaderBatchFake[int, string]{
		values: map[int]string{
			1: "one",
			2: "two",
		},
	}
	loader := NewDBLoader[int, string](batch.Load)

	values, err := loader.LoadMany(t.Context(), 1, 2, 1)
	if err != nil {
		t.Fatalf("LoadMany returned error: %v", err)
	}
	assertDBLoaderValues(t, values, map[int]string{1: "one", 2: "two"})
	assertDBLoaderCalls(t, batch.calls, [][]int{{1, 2}})

	values, err = loader.LoadMany(t.Context(), 2, 3)
	if err != nil {
		t.Fatalf("LoadMany returned error: %v", err)
	}
	assertDBLoaderValues(t, values, map[int]string{2: "two"})
	assertDBLoaderCalls(t, batch.calls, [][]int{{1, 2}, {3}})

	value, ok, err := loader.Load(t.Context(), 3)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if ok {
		t.Fatalf("Load missing key ok = true, value = %q", value)
	}
	assertDBLoaderCalls(t, batch.calls, [][]int{{1, 2}, {3}})
}

func TestDBLoaderPrimeAndClear(t *testing.T) {
	batch := &dbLoaderBatchFake[int, string]{
		values: map[int]string{
			1: "loaded",
		},
	}
	loader := NewDBLoader[int, string](batch.Load)
	loader.Prime(1, "primed")

	value, ok, err := loader.Load(t.Context(), 1)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}
	if !ok || value != "primed" {
		t.Fatalf("Load primed value = %q, %v; want primed, true", value, ok)
	}
	assertDBLoaderCalls(t, batch.calls, nil)

	loader.Clear(1)
	value, ok, err = loader.Load(t.Context(), 1)
	if err != nil {
		t.Fatalf("Load after Clear returned error: %v", err)
	}
	if !ok || value != "loaded" {
		t.Fatalf("Load after Clear = %q, %v; want loaded, true", value, ok)
	}
	assertDBLoaderCalls(t, batch.calls, [][]int{{1}})

	_, ok, err = loader.Load(t.Context(), 99)
	if err != nil {
		t.Fatalf("Load missing key returned error: %v", err)
	}
	if ok {
		t.Fatalf("Load missing key ok = true, want false")
	}
	loader.Prime(99, "now-present")
	value, ok, err = loader.Load(t.Context(), 99)
	if err != nil {
		t.Fatalf("Load primed miss returned error: %v", err)
	}
	if !ok || value != "now-present" {
		t.Fatalf("Load primed miss = %q, %v; want now-present, true", value, ok)
	}
}

func TestDBLoaderClearAll(t *testing.T) {
	batch := &dbLoaderBatchFake[int, string]{
		values: map[int]string{
			1: "one",
			2: "two",
		},
	}
	loader := NewDBLoader[int, string](batch.Load)
	if err := loader.Preload(t.Context(), 1, 2); err != nil {
		t.Fatalf("Preload returned error: %v", err)
	}
	loader.ClearAll()

	if _, _, err := loader.Load(t.Context(), 1); err != nil {
		t.Fatalf("Load after ClearAll returned error: %v", err)
	}
	assertDBLoaderCalls(t, batch.calls, [][]int{{1, 2}, {1}})
}

func TestDBLoaderCacheIsPerLoaderInstance(t *testing.T) {
	batch := &dbLoaderBatchFake[int, string]{
		values: map[int]string{1: "one"},
	}
	first := NewDBLoader[int, string](batch.Load)
	second := NewDBLoader[int, string](batch.Load)

	if _, _, err := first.Load(t.Context(), 1); err != nil {
		t.Fatalf("first Load returned error: %v", err)
	}
	if _, _, err := first.Load(t.Context(), 1); err != nil {
		t.Fatalf("first cached Load returned error: %v", err)
	}
	if _, _, err := second.Load(t.Context(), 1); err != nil {
		t.Fatalf("second Load returned error: %v", err)
	}

	assertDBLoaderCalls(t, batch.calls, [][]int{{1}, {1}})
}

func TestDBLoaderHonorsContextCancellation(t *testing.T) {
	canceled, cancel := context.WithCancel(t.Context())
	cancel()
	batch := &dbLoaderBatchFake[int, string]{values: map[int]string{1: "one"}}
	loader := NewDBLoader[int, string](batch.Load)

	if _, _, err := loader.Load(canceled, 1); !errors.Is(err, context.Canceled) {
		t.Fatalf("Load canceled error = %v, want context.Canceled", err)
	}
	assertDBLoaderCalls(t, batch.calls, nil)

	ctx, cancelDuringBatch := context.WithCancel(t.Context())
	loader = NewDBLoader[int, string](func(context.Context, []int) (map[int]string, error) {
		cancelDuringBatch()
		return map[int]string{1: "one"}, nil
	})

	if _, _, err := loader.Load(ctx, 1); !errors.Is(err, context.Canceled) {
		t.Fatalf("Load canceled during batch error = %v, want context.Canceled", err)
	}

	value, ok, err := loader.Load(t.Context(), 1)
	if err != nil {
		t.Fatalf("Load after canceled batch returned error: %v", err)
	}
	if !ok || value != "one" {
		t.Fatalf("Load after canceled batch = %q, %v; want one, true", value, ok)
	}
}

func TestDBLoaderValidatesLoaderAndBatchFunction(t *testing.T) {
	var nilLoader *DBLoader[int, string]
	if _, _, err := nilLoader.Load(t.Context(), 1); !errors.Is(err, errNilDBLoader) {
		t.Fatalf("nil loader Load error = %v, want %v", err, errNilDBLoader)
	}

	var zeroLoader DBLoader[int, string]
	if _, _, err := zeroLoader.Load(t.Context(), 1); !errors.Is(err, errNilDBLoaderBatchFunc) {
		t.Fatalf("zero loader Load error = %v, want %v", err, errNilDBLoaderBatchFunc)
	}

	zeroLoader.Prime(1, "primed")
	value, ok, err := zeroLoader.Load(t.Context(), 1)
	if err != nil {
		t.Fatalf("primed zero loader Load returned error: %v", err)
	}
	if !ok || value != "primed" {
		t.Fatalf("primed zero loader Load = %q, %v; want primed, true", value, ok)
	}
}

type dbLoaderBatchFake[K comparable, V any] struct {
	values map[K]V
	calls  [][]K
}

func (f *dbLoaderBatchFake[K, V]) Load(_ context.Context, keys []K) (map[K]V, error) {
	f.calls = append(f.calls, append([]K(nil), keys...))

	values := make(map[K]V, len(keys))
	for _, key := range keys {
		value, ok := f.values[key]
		if ok {
			values[key] = value
		}
	}
	return values, nil
}

func assertDBLoaderCalls[K comparable](t *testing.T, got, want [][]K) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("calls = %#v, want %#v", got, want)
	}
}

func assertDBLoaderValues[K comparable, V any](t *testing.T, got, want map[K]V) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("values = %#v, want %#v", got, want)
	}
}
