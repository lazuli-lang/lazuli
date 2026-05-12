package observability

import (
	"bytes"
	"context"
	"log/slog"
	"strings"
	"testing"
	"time"
)

func TestDeterministicRateSamplerKeepsEvenRate(t *testing.T) {
	t.Parallel()

	sampler := NewDeterministicRateSampler(0.25)
	got := make([]bool, 8)
	for i := range got {
		got[i] = sampler.Sample(context.Background(), slog.Record{})
	}

	want := []bool{false, false, false, true, false, false, false, true}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("sample[%d] = %v, want %v in %v", i, got[i], want[i], got)
		}
	}
}

func TestDeterministicRateSamplerClampsBounds(t *testing.T) {
	t.Parallel()

	dropAll := NewDeterministicRateSampler(-1)
	if dropAll.Sample(context.Background(), slog.Record{}) {
		t.Fatal("negative rate sampled a record, want drop")
	}

	keepAll := NewDeterministicRateSampler(2)
	for i := 0; i < 4; i++ {
		if !keepAll.Sample(context.Background(), slog.Record{}) {
			t.Fatalf("rate above one sample[%d] = false, want true", i)
		}
	}
}

func TestBurstSamplerAllowsLimitPerWindow(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	sampler := NewBurstSampler(2, time.Second)
	sampler.Clock = func() time.Time { return now }

	if !sampler.Sample(context.Background(), slog.Record{}) {
		t.Fatal("first sample = false, want true")
	}
	if !sampler.Sample(context.Background(), slog.Record{}) {
		t.Fatal("second sample = false, want true")
	}
	if sampler.Sample(context.Background(), slog.Record{}) {
		t.Fatal("third sample = true, want false after burst limit")
	}

	now = now.Add(999 * time.Millisecond)
	if sampler.Sample(context.Background(), slog.Record{}) {
		t.Fatal("sample before window reset = true, want false")
	}

	now = now.Add(time.Millisecond)
	if !sampler.Sample(context.Background(), slog.Record{}) {
		t.Fatal("sample after window reset = false, want true")
	}
}

func TestPerKeySamplerMaintainsIndependentState(t *testing.T) {
	t.Parallel()

	sampler := NewPerKeySampler(LogRecordAttrKey("tenant"), func(string) LogSampler {
		return NewBurstSampler(1, time.Hour)
	})

	if !sampler.Sample(context.Background(), logSamplingRecord("first", slog.String("tenant", "a"))) {
		t.Fatal("tenant a first sample = false, want true")
	}
	if sampler.Sample(context.Background(), logSamplingRecord("second", slog.String("tenant", "a"))) {
		t.Fatal("tenant a second sample = true, want false")
	}
	if !sampler.Sample(context.Background(), logSamplingRecord("third", slog.String("tenant", "b"))) {
		t.Fatal("tenant b first sample = false, want true")
	}
}

func TestLogRecordAttrKeyReadsGroupedAttrs(t *testing.T) {
	t.Parallel()

	record := logSamplingRecord("grouped", slog.Group("request", slog.String("id", "req-1")))
	key := LogRecordAttrKey("id")(context.Background(), record)
	if key != "req-1" {
		t.Fatalf("LogRecordAttrKey(id) = %q, want req-1", key)
	}
}

func TestSamplingHandlerDropsSampledOutRecords(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer
	handler := NewSamplingHandler(
		slog.NewJSONHandler(&buf, nil),
		LogSamplerFunc(func(ctx context.Context, record slog.Record) bool {
			_ = ctx
			return record.Message == "keep"
		}),
	)
	logger := slog.New(handler)

	logger.Info("drop")
	logger.Info("keep")

	output := buf.String()
	if strings.Contains(output, "drop") {
		t.Fatalf("output contains sampled-out record: %s", output)
	}
	if !strings.Contains(output, "keep") {
		t.Fatalf("output = %q, want sampled-in record", output)
	}
}

func TestSamplingHandlerSamplesLoggerAttrs(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer
	sampler := NewPerKeySampler(LogRecordAttrKey("tenant"), func(string) LogSampler {
		return NewBurstSampler(1, time.Hour)
	})
	logger := slog.New(NewSamplingHandler(slog.NewJSONHandler(&buf, nil), sampler))

	logger.With("tenant", "a").Info("a-first")
	logger.With("tenant", "a").Info("a-second")
	logger.With("tenant", "b").Info("b-first")

	output := buf.String()
	if !strings.Contains(output, "a-first") {
		t.Fatalf("output = %q, want tenant a first record", output)
	}
	if strings.Contains(output, "a-second") {
		t.Fatalf("output contains tenant a sampled-out record: %s", output)
	}
	if !strings.Contains(output, "b-first") {
		t.Fatalf("output = %q, want tenant b first record", output)
	}
}

func logSamplingRecord(message string, attrs ...slog.Attr) slog.Record {
	record := slog.NewRecord(time.Time{}, slog.LevelInfo, message, 0)
	record.AddAttrs(attrs...)
	return record
}
