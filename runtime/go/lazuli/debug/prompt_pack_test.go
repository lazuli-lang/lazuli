package debug

import (
	"encoding/json"
	"errors"
	"fmt"
	"reflect"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/observability"
)

type promptPackTestError struct {
	Code    string
	Status  int
	message string
}

func (e *promptPackTestError) Error() string {
	return e.message
}

func TestPromptPackBuilderBuildsDeterministicPayload(t *testing.T) {
	root := &promptPackTestError{
		Code:    "validation_failed",
		Status:  422,
		message: "email invalid",
	}

	builder := NewPromptPackBuilder(&PromptPackConfig{MaxBytes: 8 * 1024})
	builder.SetError(fmt.Errorf("create customer: %w", root))
	builder.AddRecommendation(PromptPackRecommendation{
		Route:    "contact_codegen_owner",
		Message:  "inspect codegen pattern",
		Priority: 20,
	})
	builder.AddRecommendation(PromptPackRecommendation{
		Route:    "read_lzi",
		Message:  "check field validation near source",
		Target:   "features/customer.lzi:42:1",
		Priority: 10,
	})
	builder.AddSourceSnippet(PromptPackSourceSnippet{
		Name:     "ir",
		Path:     "features/customer.command.json",
		Language: "json",
		Content:  `{"kind":"command","op":"create_customer"}`,
	})
	builder.AddSourceSnippet(PromptPackSourceSnippet{
		Name:      "command",
		Path:      `.\\features\\customer.lzi`,
		StartLine: 42,
		EndLine:   47,
		Language:  "lzi",
		Content:   "command create_customer\n  field email Email required\n",
	})
	builder.AddProfileRow(PromptPackProfileRow{
		Feature:     "invoice",
		Kind:        "query",
		Op:          "list",
		SampleCount: 3,
		CPUNanos:    int64(6 * time.Second),
	})
	builder.AddProfileOpReport(observability.ProfileOpReport{
		Feature:        "customer",
		Kind:           "command",
		Op:             "create_customer",
		PatternID:      "command_pgx_insert",
		PatternVersion: "v1",
		SampleCount:    2,
		CPUDuration:    4 * time.Second,
		AllocDuration:  1500 * time.Millisecond,
	})

	data, summary, err := builder.Build()
	if err != nil {
		t.Fatalf("Build() error = %v", err)
	}
	if summary.TotalBytes != len(data) {
		t.Fatalf("summary.TotalBytes = %d, want %d", summary.TotalBytes, len(data))
	}
	if summary.Truncated {
		t.Fatal("summary.Truncated = true, want false")
	}

	payload := promptPackDecode(t, data)
	if payload.Version != PromptPackVersion {
		t.Fatalf("Version = %q, want %q", payload.Version, PromptPackVersion)
	}
	if payload.Metadata.TotalBytes != len(data) {
		t.Fatalf("payload total bytes = %d, want %d", payload.Metadata.TotalBytes, len(data))
	}
	if payload.ErrorReport == nil {
		t.Fatal("ErrorReport = nil")
	}
	if payload.ErrorReport.Code != "validation_failed" || payload.ErrorReport.Status != 422 {
		t.Fatalf("ErrorReport code/status = %q/%d, want validation_failed/422", payload.ErrorReport.Code, payload.ErrorReport.Status)
	}

	gotSnippetOrder := []string{
		payload.SourceSnippets[0].Path,
		payload.SourceSnippets[1].Path,
	}
	wantSnippetOrder := []string{
		"features/customer.command.json",
		"features/customer.lzi",
	}
	if !reflect.DeepEqual(gotSnippetOrder, wantSnippetOrder) {
		t.Fatalf("source snippet order = %#v, want %#v", gotSnippetOrder, wantSnippetOrder)
	}

	gotRoutes := []string{
		payload.Recommendations[0].Route,
		payload.Recommendations[1].Route,
	}
	wantRoutes := []string{"read_lzi", "contact_codegen_owner"}
	if !reflect.DeepEqual(gotRoutes, wantRoutes) {
		t.Fatalf("recommendation routes = %#v, want %#v", gotRoutes, wantRoutes)
	}

	if got := payload.ProfileRows[0].Feature + "." + payload.ProfileRows[0].Kind + "." + payload.ProfileRows[0].Op; got != "invoice.query.list" {
		t.Fatalf("first profile row = %q, want invoice.query.list", got)
	}
	customer := payload.ProfileRows[1]
	if customer.PatternID != "command_pgx_insert" || customer.CPUNanos != int64(4*time.Second) || customer.AllocNanos != int64(1500*time.Millisecond) {
		t.Fatalf("converted profile row = %#v", customer)
	}

	reordered, _, err := BuildPromptPack(PromptPackInput{
		Config:      PromptPackConfig{MaxBytes: 8 * 1024},
		ErrorReport: payload.ErrorReport,
		SourceSnippets: []PromptPackSourceSnippet{
			builder.sourceSnippets[1],
			builder.sourceSnippets[0],
		},
		ProfileRows: []PromptPackProfileRow{
			builder.profileRows[1],
			builder.profileRows[0],
		},
		Recommendations: []PromptPackRecommendation{
			builder.recommendations[1],
			builder.recommendations[0],
		},
	})
	if err != nil {
		t.Fatalf("BuildPromptPack(reordered) error = %v", err)
	}
	if string(reordered) != string(data) {
		t.Fatalf("prompt pack changed after input reorder\nfirst:  %s\nsecond: %s", data, reordered)
	}
}

func TestBuildPromptPackBoundsPayload(t *testing.T) {
	data, summary, err := BuildPromptPack(PromptPackInput{
		Config: PromptPackConfig{MaxBytes: 700},
		SourceSnippets: []PromptPackSourceSnippet{{
			Name:      "large command",
			Path:      "features/customer.lzi",
			StartLine: 1,
			Language:  "lzi",
			Content:   strings.Repeat("command create_customer {}\n", 200),
		}},
		ProfileRows: []PromptPackProfileRow{
			{Feature: "customer", Kind: "command", Op: "create", CPUNanos: 5000},
			{Feature: "invoice", Kind: "query", Op: "list", CPUNanos: 4000},
		},
		Recommendations: []PromptPackRecommendation{{
			Route:   "read_lzi",
			Message: strings.Repeat("inspect source ", 80),
		}},
	})
	if err != nil {
		t.Fatalf("BuildPromptPack() error = %v", err)
	}
	if len(data) > 700 {
		t.Fatalf("payload length = %d, want <= 700", len(data))
	}
	if !summary.Truncated {
		t.Fatal("summary.Truncated = false, want true")
	}

	payload := promptPackDecode(t, data)
	if !payload.Metadata.Truncated {
		t.Fatal("payload.Metadata.Truncated = false, want true")
	}
	if len(payload.SourceSnippets) == 0 || !payload.SourceSnippets[0].Truncated {
		t.Fatalf("source snippet truncation missing: %#v", payload.SourceSnippets)
	}
}

func TestBuildPromptPackRejectsGeneratedGoSource(t *testing.T) {
	_, _, err := BuildPromptPack(PromptPackInput{
		SourceSnippets: []PromptPackSourceSnippet{{
			Path:    "dist/go/customer/customer.gen.go",
			Content: "package customer",
		}},
	})
	if !errors.Is(err, ErrPromptPackGeneratedSource) {
		t.Fatalf("BuildPromptPack() error = %v, want ErrPromptPackGeneratedSource", err)
	}
}

func TestBuildPromptPackReportsTooSmallBudget(t *testing.T) {
	_, _, err := BuildPromptPack(PromptPackInput{
		Config: PromptPackConfig{MaxBytes: 10},
	})
	if !errors.Is(err, ErrPromptPackBudget) {
		t.Fatalf("BuildPromptPack() error = %v, want ErrPromptPackBudget", err)
	}
}

func promptPackDecode(t *testing.T, data []byte) promptPackPayload {
	t.Helper()

	var payload promptPackPayload
	if err := json.Unmarshal(data, &payload); err != nil {
		t.Fatalf("unmarshal prompt pack: %v\n%s", err, data)
	}
	return payload
}
