package lazuli_test

import (
	"context"
	"errors"
	"net/http"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

type repositorySoftRow struct {
	DeletedAt *lazuli.Time
}

func (r repositorySoftRow) SoftDeleted() bool {
	return r.DeletedAt != nil
}

func (r *repositorySoftRow) MarkSoftDeleted(at lazuli.Time) {
	r.DeletedAt = &at
}

func TestRepositoryNotFoundErrorCarriesTypedEnvelope(t *testing.T) {
	err := lazuli.NewRepositoryNotFound("customer", lazuli.ID(42))

	if !errors.Is(err, lazuli.ErrRepositoryNotFound) {
		t.Fatal("errors.Is did not match ErrRepositoryNotFound")
	}

	var typed *lazuli.RepositoryNotFoundError
	if !errors.As(err, &typed) {
		t.Fatal("errors.As did not recover RepositoryNotFoundError")
	}
	if typed.Resource != "customer" {
		t.Fatalf("Resource = %q, want customer", typed.Resource)
	}
	if typed.ID != lazuli.ID(42) {
		t.Fatalf("ID = %v, want 42", typed.ID)
	}

	var envelope *lazuli.Error
	if !errors.As(err, &envelope) {
		t.Fatal("errors.As did not expose Lazuli Error envelope")
	}
	if envelope.Status != http.StatusNotFound {
		t.Fatalf("Status = %d, want %d", envelope.Status, http.StatusNotFound)
	}
	if envelope.Code != lazuli.CodeNotFound {
		t.Fatalf("Code = %q, want %q", envelope.Code, lazuli.CodeNotFound)
	}
	if envelope.Base.Kind != "repository" || envelope.Base.Op != "lookup_by_id" {
		t.Fatalf("Base scope = %q/%q, want repository/lookup_by_id", envelope.Base.Kind, envelope.Base.Op)
	}
	if envelope.Data.(map[string]any)["resource"] != "customer" {
		t.Fatalf("Data resource = %v, want customer", envelope.Data.(map[string]any)["resource"])
	}

	problem := lazuli.ProblemFromError(err)
	if problem.Status != http.StatusNotFound {
		t.Fatalf("Problem status = %d, want %d", problem.Status, http.StatusNotFound)
	}
	if problem.Extensions["code"] != lazuli.CodeNotFound {
		t.Fatalf("Problem code = %v, want %q", problem.Extensions["code"], lazuli.CodeNotFound)
	}
}

func TestRepositoryConflictErrorCarriesReasonAndCause(t *testing.T) {
	cause := errors.New("duplicate email")
	err := &lazuli.RepositoryConflictError{
		Base:     lazuli.ErrorBase{Cause: cause},
		Resource: "customer",
		ID:       lazuli.ID(7),
		Reason:   "email already exists",
	}

	if !errors.Is(err, lazuli.ErrRepositoryConflict) {
		t.Fatal("errors.Is did not match ErrRepositoryConflict")
	}
	if !errors.Is(err, cause) {
		t.Fatal("errors.Is did not preserve conflict cause")
	}

	var envelope *lazuli.Error
	if !errors.As(err, &envelope) {
		t.Fatal("errors.As did not expose Lazuli Error envelope")
	}
	if envelope.Status != http.StatusConflict {
		t.Fatalf("Status = %d, want %d", envelope.Status, http.StatusConflict)
	}
	if envelope.Code != lazuli.CodeConflict {
		t.Fatalf("Code = %q, want %q", envelope.Code, lazuli.CodeConflict)
	}
	data := envelope.Data.(map[string]any)
	if data["reason"] != "email already exists" {
		t.Fatalf("Data reason = %v, want email already exists", data["reason"])
	}
}

func TestRepositoryIDLookupFuncImplementsContract(t *testing.T) {
	var repo lazuli.RepositoryIDLookup[string] = lazuli.RepositoryIDLookupFunc[string](
		func(_ context.Context, id lazuli.ID) (string, error) {
			if id != 99 {
				return "", lazuli.NewRepositoryNotFound("customer", id)
			}
			return "found", nil
		},
	)

	got, err := repo.LookupByID(context.Background(), 99)
	if err != nil {
		t.Fatalf("LookupByID returned error: %v", err)
	}
	if got != "found" {
		t.Fatalf("LookupByID = %q, want found", got)
	}

	_, err = repo.LookupByID(context.Background(), 100)
	if !errors.Is(err, lazuli.ErrRepositoryNotFound) {
		t.Fatalf("LookupByID missing error = %v, want ErrRepositoryNotFound", err)
	}
}

func TestSoftDeleteMarkersAreOptional(t *testing.T) {
	if lazuli.IsSoftDeleted(struct{}{}) {
		t.Fatal("non-marker row reported soft-deleted")
	}
	if lazuli.MarkSoftDeleted(struct{}{}, lazuli.Time{}) {
		t.Fatal("non-updater row reported soft-delete update")
	}

	row := repositorySoftRow{}
	if lazuli.IsSoftDeleted(row) {
		t.Fatal("new marker row reported soft-deleted")
	}

	deletedAt := lazuli.Time(time.Unix(1, 0).UTC())
	if !lazuli.MarkSoftDeleted(&row, deletedAt) {
		t.Fatal("MarkSoftDeleted did not apply updater")
	}
	if !lazuli.IsSoftDeleted(row) {
		t.Fatal("marked row did not report soft-deleted")
	}
	if row.DeletedAt == nil || !row.DeletedAt.Equal(time.Unix(1, 0).UTC()) {
		t.Fatalf("DeletedAt = %v, want %v", row.DeletedAt, time.Unix(1, 0).UTC())
	}
}
