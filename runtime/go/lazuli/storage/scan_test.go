package storage_test

import (
	"context"
	"errors"
	"io"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/storage"
)

type scanHookFunc func(context.Context, storage.Metadata, io.Reader) (storage.ScanResult, error)

func (f scanHookFunc) Scan(ctx context.Context, metadata storage.Metadata, body io.Reader) (storage.ScanResult, error) {
	return f(ctx, metadata, body)
}

func TestRunScanHooksNoHooksAllowsWithoutOpeningBody(t *testing.T) {
	t.Parallel()

	called := false
	result, err := storage.RunScanHooks(context.Background(), nil, storage.Metadata{}, func(context.Context) (io.ReadCloser, error) {
		called = true
		return io.NopCloser(strings.NewReader("payload")), nil
	})
	if err != nil {
		t.Fatalf("RunScanHooks failed: %v", err)
	}
	if result.Verdict != storage.VerdictClean {
		t.Fatalf("verdict = %s, want clean", result.Verdict)
	}
	if called {
		t.Fatal("body opener was called with no hooks")
	}
}

func TestRunScanHooksRunsEachHookWithFreshBody(t *testing.T) {
	t.Parallel()

	metadata := storage.Metadata{
		Filename:    "invoice.pdf",
		ContentType: "application/pdf",
		Size:        7,
	}
	var opens int
	var saw []string
	open := func(context.Context) (io.ReadCloser, error) {
		opens++
		return io.NopCloser(strings.NewReader("payload")), nil
	}
	hooks := []storage.ScanHook{
		scanHookFunc(func(_ context.Context, got storage.Metadata, body io.Reader) (storage.ScanResult, error) {
			if got != metadata {
				t.Fatalf("metadata = %+v, want %+v", got, metadata)
			}
			bytes, err := io.ReadAll(body)
			if err != nil {
				t.Fatalf("ReadAll failed: %v", err)
			}
			saw = append(saw, "first:"+string(bytes))
			return storage.ScanResult{Verdict: storage.VerdictClean, Scanner: "first"}, nil
		}),
		scanHookFunc(func(_ context.Context, _ storage.Metadata, body io.Reader) (storage.ScanResult, error) {
			bytes, err := io.ReadAll(body)
			if err != nil {
				t.Fatalf("ReadAll failed: %v", err)
			}
			saw = append(saw, "second:"+string(bytes))
			return storage.ScanResult{Verdict: storage.VerdictClean, Scanner: "second"}, nil
		}),
	}

	result, err := storage.RunScanHooks(context.Background(), hooks, metadata, open)
	if err != nil {
		t.Fatalf("RunScanHooks failed: %v", err)
	}
	if result.Verdict != storage.VerdictClean {
		t.Fatalf("verdict = %s, want clean", result.Verdict)
	}
	if opens != 2 {
		t.Fatalf("open calls = %d, want 2", opens)
	}
	if got, want := strings.Join(saw, ","), "first:payload,second:payload"; got != want {
		t.Fatalf("hook reads = %q, want %q", got, want)
	}
}

func TestRunScanHooksAggregatesBySeverityAndKeepsFirstWinner(t *testing.T) {
	t.Parallel()

	var calls []string
	hooks := []storage.ScanHook{
		staticScanHook(&calls, "unavailable", storage.ScanResult{Verdict: storage.VerdictUnavailable, Scanner: "scanner-down"}, nil),
		staticScanHook(&calls, "blocked", storage.ScanResult{Verdict: storage.VerdictBlocked, Scanner: "policy", Reason: "encrypted archive"}, nil),
		staticScanHook(&calls, "infected-a", storage.ScanResult{Verdict: storage.VerdictInfected, Scanner: "av-a", Reason: "eicar"}, nil),
		staticScanHook(&calls, "infected-b", storage.ScanResult{Verdict: storage.VerdictInfected, Scanner: "av-b", Reason: "other"}, nil),
	}

	result, err := storage.RunScanHooks(context.Background(), hooks, storage.Metadata{}, scanTestBody("payload"))
	if !errors.Is(err, storage.ErrFileInfected) {
		t.Fatalf("expected ErrFileInfected, got %v", err)
	}
	if result.Scanner != "av-a" {
		t.Fatalf("scanner = %q, want first infected scanner", result.Scanner)
	}
	if got, want := strings.Join(calls, ","), "unavailable,blocked,infected-a,infected-b"; got != want {
		t.Fatalf("calls = %q, want %q", got, want)
	}
}

func TestRunScanHooksReturnsBlockedWhenNoHookFindsInfection(t *testing.T) {
	t.Parallel()

	hooks := []storage.ScanHook{
		scanHookFunc(func(context.Context, storage.Metadata, io.Reader) (storage.ScanResult, error) {
			return storage.ScanResult{Verdict: storage.VerdictUnavailable, Scanner: "down"}, nil
		}),
		scanHookFunc(func(context.Context, storage.Metadata, io.Reader) (storage.ScanResult, error) {
			return storage.ScanResult{Verdict: storage.VerdictBlocked, Scanner: "policy"}, nil
		}),
	}

	result, err := storage.RunScanHooks(context.Background(), hooks, storage.Metadata{}, scanTestBody("payload"))
	if !errors.Is(err, storage.ErrFileBlocked) {
		t.Fatalf("expected ErrFileBlocked, got %v", err)
	}
	if result.Scanner != "policy" {
		t.Fatalf("scanner = %q, want policy", result.Scanner)
	}
}

func TestRunScanHooksClassifiesHookErrorsAsUnavailable(t *testing.T) {
	t.Parallel()

	scannerDown := errors.New("scanner down")
	hooks := []storage.ScanHook{
		scanHookFunc(func(context.Context, storage.Metadata, io.Reader) (storage.ScanResult, error) {
			return storage.ScanResult{Scanner: "av"}, scannerDown
		}),
	}

	result, err := storage.RunScanHooks(context.Background(), hooks, storage.Metadata{}, scanTestBody("payload"))
	if !errors.Is(err, storage.ErrScanUnavailable) {
		t.Fatalf("expected ErrScanUnavailable, got %v", err)
	}
	if result.Verdict != storage.VerdictUnavailable {
		t.Fatalf("verdict = %s, want unavailable", result.Verdict)
	}
	if !strings.Contains(err.Error(), scannerDown.Error()) {
		t.Fatalf("error %q does not include hook cause", err)
	}
}

func TestRunScanHooksPropagatesContextCancellation(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	called := false
	_, err := storage.RunScanHooks(ctx, []storage.ScanHook{
		scanHookFunc(func(context.Context, storage.Metadata, io.Reader) (storage.ScanResult, error) {
			called = true
			return storage.ScanResult{Verdict: storage.VerdictClean}, nil
		}),
	}, storage.Metadata{}, scanTestBody("payload"))
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("expected context.Canceled, got %v", err)
	}
	if called {
		t.Fatal("hook was called after context cancellation")
	}
}

func TestVerdictString(t *testing.T) {
	t.Parallel()

	cases := map[storage.Verdict]string{
		storage.VerdictClean:       "clean",
		storage.VerdictInfected:    "infected",
		storage.VerdictBlocked:     "blocked",
		storage.VerdictUnavailable: "unavailable",
		storage.Verdict(99):        "unknown",
	}
	for verdict, want := range cases {
		if got := verdict.String(); got != want {
			t.Fatalf("Verdict(%d).String() = %q, want %q", verdict, got, want)
		}
	}
}

func staticScanHook(calls *[]string, name string, result storage.ScanResult, err error) storage.ScanHook {
	return scanHookFunc(func(context.Context, storage.Metadata, io.Reader) (storage.ScanResult, error) {
		*calls = append(*calls, name)
		return result, err
	})
}

func scanTestBody(payload string) storage.BodyOpener {
	return func(context.Context) (io.ReadCloser, error) {
		return io.NopCloser(strings.NewReader(payload)), nil
	}
}
