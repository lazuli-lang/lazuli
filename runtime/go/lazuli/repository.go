package lazuli

import (
	"context"
	"errors"
	"fmt"
	"net/http"
)

// CodeConflict is the stable error code for optimistic-lock, uniqueness, and
// other generated repository write conflicts.
const CodeConflict = "conflict"

// Repository sentinel errors let generated CRUD code expose stable errors.Is
// checks while also carrying typed resource metadata.
var (
	ErrRepositoryNotFound = errors.New("lazuli/repository: not_found")
	ErrRepositoryConflict = errors.New("lazuli/repository: conflict")
)

// RepositoryIDLookup is the minimal lookup contract generated repositories
// expose for target loading and CRUD-by-ID helpers.
type RepositoryIDLookup[T any] interface {
	LookupByID(context.Context, ID) (T, error)
}

// RepositoryIDLookupFunc adapts a function to RepositoryIDLookup.
type RepositoryIDLookupFunc[T any] func(context.Context, ID) (T, error)

// LookupByID implements RepositoryIDLookup.
func (f RepositoryIDLookupFunc[T]) LookupByID(ctx context.Context, id ID) (T, error) {
	return f(ctx, id)
}

// RepositoryNotFoundError reports a missing repository row.
type RepositoryNotFoundError struct {
	Base     ErrorBase
	Resource string
	ID       any
}

// NewRepositoryNotFound returns a typed 404 repository error.
func NewRepositoryNotFound(resource string, id any) *RepositoryNotFoundError {
	return &RepositoryNotFoundError{
		Resource: resource,
		ID:       id,
	}
}

// Error implements the error interface.
func (e *RepositoryNotFoundError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("repository_not_found", e.errorBase())
}

// Unwrap exposes the underlying cause for errors.Is and errors.As.
func (e *RepositoryNotFoundError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

// Is matches the repository not-found sentinel.
func (e *RepositoryNotFoundError) Is(target error) bool {
	return target == ErrRepositoryNotFound
}

// As exposes the legacy Lazuli error envelope to existing HTTP/error helpers.
func (e *RepositoryNotFoundError) As(target any) bool {
	if e == nil {
		return false
	}
	if out, ok := target.(**Error); ok {
		*out = e.errorEnvelope()
		return true
	}
	return false
}

func (e *RepositoryNotFoundError) errorEnvelope() *Error {
	base := e.errorBase()
	return &Error{
		Status:  base.Status,
		Code:    base.Code,
		Message: base.Message,
		Data:    repositoryErrorData(e.Resource, e.ID, ""),
		Base:    base,
	}
}

func (e *RepositoryNotFoundError) errorBase() ErrorBase {
	base := e.Base
	if base.Code == "" {
		base.Code = CodeNotFound
	}
	if base.Status == 0 {
		base.Status = http.StatusNotFound
	}
	if base.Origin == 0 {
		base.Origin = OriginUserDSL
	}
	if base.Kind == "" {
		base.Kind = "repository"
	}
	if base.Op == "" {
		base.Op = "lookup_by_id"
	}
	if base.Message == "" {
		base.Message = repositoryNotFoundMessage(e.Resource, e.ID)
	}
	base.Cause = errors.Join(ErrRepositoryNotFound, base.Cause)
	return base
}

// RepositoryConflictError reports a generated repository write conflict.
type RepositoryConflictError struct {
	Base     ErrorBase
	Resource string
	ID       any
	Reason   string
}

// NewRepositoryConflict returns a typed 409 repository error.
func NewRepositoryConflict(resource string, id any, reason string) *RepositoryConflictError {
	return &RepositoryConflictError{
		Resource: resource,
		ID:       id,
		Reason:   reason,
	}
}

// Error implements the error interface.
func (e *RepositoryConflictError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return typedErrorString("repository_conflict", e.errorBase())
}

// Unwrap exposes the underlying cause for errors.Is and errors.As.
func (e *RepositoryConflictError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Base.Cause
}

// Is matches the repository conflict sentinel.
func (e *RepositoryConflictError) Is(target error) bool {
	return target == ErrRepositoryConflict
}

// As exposes the legacy Lazuli error envelope to existing HTTP/error helpers.
func (e *RepositoryConflictError) As(target any) bool {
	if e == nil {
		return false
	}
	if out, ok := target.(**Error); ok {
		*out = e.errorEnvelope()
		return true
	}
	return false
}

func (e *RepositoryConflictError) errorEnvelope() *Error {
	base := e.errorBase()
	return &Error{
		Status:  base.Status,
		Code:    base.Code,
		Message: base.Message,
		Data:    repositoryErrorData(e.Resource, e.ID, e.Reason),
		Base:    base,
	}
}

func (e *RepositoryConflictError) errorBase() ErrorBase {
	base := e.Base
	if base.Code == "" {
		base.Code = CodeConflict
	}
	if base.Status == 0 {
		base.Status = http.StatusConflict
	}
	if base.Origin == 0 {
		base.Origin = OriginUserDSL
	}
	if base.Kind == "" {
		base.Kind = "repository"
	}
	if base.Op == "" {
		base.Op = "write"
	}
	if base.Message == "" {
		base.Message = repositoryConflictMessage(e.Resource, e.ID, e.Reason)
	}
	base.Cause = errors.Join(ErrRepositoryConflict, base.Cause)
	return base
}

func repositoryNotFoundMessage(resource string, id any) string {
	if resource == "" {
		return fmt.Sprintf("repository row %v not found", id)
	}
	return fmt.Sprintf("%s %v not found", resource, id)
}

func repositoryConflictMessage(resource string, id any, reason string) string {
	subject := "repository row"
	if resource != "" {
		subject = resource
	}
	if id != nil {
		subject = fmt.Sprintf("%s %v", subject, id)
	}
	if reason == "" {
		return subject + " conflict"
	}
	return subject + " conflict: " + reason
}

func repositoryErrorData(resource string, id any, reason string) map[string]any {
	data := make(map[string]any, 3)
	if resource != "" {
		data["resource"] = resource
	}
	if id != nil {
		data["id"] = id
	}
	if reason != "" {
		data["reason"] = reason
	}
	return data
}

// SoftDeleteMarker is an optional contract for generated row types that carry
// soft-delete state.
type SoftDeleteMarker interface {
	SoftDeleted() bool
}

// SoftDeleteMarkerFunc adapts a function to SoftDeleteMarker.
type SoftDeleteMarkerFunc func() bool

// SoftDeleted implements SoftDeleteMarker.
func (f SoftDeleteMarkerFunc) SoftDeleted() bool {
	return f()
}

// IsSoftDeleted reports whether row opts into SoftDeleteMarker and is marked
// deleted. Types that do not implement the marker are treated as active rows.
func IsSoftDeleted(row any) bool {
	marker, ok := row.(SoftDeleteMarker)
	return ok && marker.SoftDeleted()
}

// SoftDeleteUpdater is an optional contract for generated row types that can
// mark themselves deleted in in-memory repositories or fakes.
type SoftDeleteUpdater interface {
	MarkSoftDeleted(Time)
}

// MarkSoftDeleted marks row deleted when it implements SoftDeleteUpdater and
// reports whether the marker was applied.
func MarkSoftDeleted(row any, at Time) bool {
	updater, ok := row.(SoftDeleteUpdater)
	if !ok {
		return false
	}
	updater.MarkSoftDeleted(at)
	return true
}
