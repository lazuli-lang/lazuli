package lazuli

import (
	"context"
	"errors"
	"fmt"
	"net/http"
)

// Wrap returns err with a Lazuli error envelope populated from base and the
// active SourceTag in ctx. Nil errors stay nil. Errors that already contain a
// Lazuli envelope are returned unchanged to keep one envelope per boundary.
//
// EXPERIMENTAL: subject to change before 1.0.
func Wrap(ctx context.Context, err error, base ...ErrorBase) error {
	if err == nil {
		return nil
	}
	if errorWrapHasEnvelope(err) {
		return err
	}

	selected := ErrorBase{}
	if len(base) > 0 {
		selected = base[0]
	}
	return errorWrapBuild(ctx, err, selected, "", false)
}

// Wrapf is Wrap with a formatted envelope message. It accepts either
// Wrapf(ctx, err, "message %s", arg) or
// Wrapf(ctx, err, ErrorBase{Code: "code"}, "message %s", arg).
//
// EXPERIMENTAL: subject to change before 1.0.
func Wrapf(ctx context.Context, err error, args ...any) error {
	if err == nil {
		return nil
	}
	if errorWrapHasEnvelope(err) {
		return err
	}

	parsed := errorWrapParseArgs(args)
	cause := err
	if parsed.cause != nil {
		cause = errorWrapJoinDistinct(cause, parsed.cause)
	}
	return errorWrapBuild(ctx, cause, parsed.base, parsed.message, parsed.hasMessage)
}

// Errorf returns a source-aware Lazuli error envelope. It accepts either
// Errorf(ctx, "message %s", arg) or
// Errorf(ctx, ErrorBase{Code: "code"}, "message %s", arg). A %w verb in the
// format string is preserved in the unwrap chain.
//
// EXPERIMENTAL: subject to change before 1.0.
func Errorf(ctx context.Context, args ...any) error {
	parsed := errorWrapParseArgs(args)
	cause := errorWrapJoinDistinct(parsed.base.Cause, parsed.cause)
	if cause != nil && errorWrapHasEnvelope(cause) {
		if parsed.formatted != nil {
			return parsed.formatted
		}
		if parsed.hasMessage {
			return fmt.Errorf("%s: %w", parsed.message, cause)
		}
		return cause
	}
	return errorWrapBuild(ctx, cause, parsed.base, parsed.message, parsed.hasMessage)
}

type errorWrapArgs struct {
	base       ErrorBase
	message    string
	cause      error
	formatted  error
	hasMessage bool
}

func errorWrapBuild(ctx context.Context, cause error, base ErrorBase, message string, hasMessage bool) error {
	baseWasZero := errorWrapBaseIsZero(base)

	if cause != nil {
		base.Cause = cause
	}
	if hasMessage {
		base.Message = message
	} else if base.Message == "" && cause != nil {
		base.Message = cause.Error()
	}

	base = errorWrapApplyClassification(base, baseWasZero)
	base = errorWrapApplySource(ctx, base)

	return &Error{
		Status:  base.Status,
		Code:    base.Code,
		Message: base.Message,
		Base:    base,
	}
}

func errorWrapParseArgs(args []any) errorWrapArgs {
	var parsed errorWrapArgs
	if len(args) == 0 {
		return parsed
	}

	switch base := args[0].(type) {
	case ErrorBase:
		parsed.base = base
		args = args[1:]
	case *ErrorBase:
		if base != nil {
			parsed.base = *base
			args = args[1:]
		}
	}
	if len(args) == 0 {
		if parsed.base.Message != "" {
			parsed.message = parsed.base.Message
			parsed.hasMessage = true
		}
		return parsed
	}

	format, ok := args[0].(string)
	if !ok {
		parsed.message = fmt.Sprint(args...)
		parsed.hasMessage = true
		return parsed
	}

	parsed.message, parsed.cause, parsed.formatted = errorWrapFormat(format, args[1:])
	parsed.hasMessage = true
	return parsed
}

func errorWrapFormat(format string, args []any) (string, error, error) {
	err := fmt.Errorf(format, args...)
	switch wrapped := err.(type) {
	case interface{ Unwrap() []error }:
		return err.Error(), errors.Join(wrapped.Unwrap()...), err
	case interface{ Unwrap() error }:
		return err.Error(), wrapped.Unwrap(), err
	default:
		return err.Error(), nil, nil
	}
}

func errorWrapApplyClassification(base ErrorBase, baseWasZero bool) ErrorBase {
	cause := base.Cause
	if cause != nil {
		classification := ClassifyError(cause)
		if base.Code == "" {
			base.Code = classification.Code
		}
		if base.Status == 0 {
			base.Status = classification.Status
		}
		if baseWasZero {
			base.Origin = errorWrapOriginFromString(classification.Origin)
		}
		return base
	}

	if base.Code == "" {
		base.Code = CodeInternal
	}
	if base.Status == 0 {
		base.Status = http.StatusInternalServerError
	}
	if baseWasZero {
		base.Origin = OriginLibInternal
	}
	return base
}

func errorWrapApplySource(ctx context.Context, base ErrorBase) ErrorBase {
	tag, ok := SourceFromContext(ctx)
	if !ok {
		return base
	}
	if base.Feature == "" {
		base.Feature = tag.Feature
	}
	if base.Kind == "" {
		base.Kind = tag.Kind
	}
	if base.Op == "" {
		base.Op = tag.Name
	}
	if base.Source == "" && tag.File != "" && tag.Line > 0 && tag.Column > 0 {
		base.Source = FormatSourceLocation(tag.File, tag.Line, tag.Column)
	}
	return base
}

func errorWrapHasEnvelope(err error) bool {
	if err == nil {
		return false
	}

	var legacy *Error
	if errors.As(err, &legacy) {
		return true
	}
	var field *FieldError
	if errors.As(err, &field) {
		return true
	}
	var policy *PolicyError
	if errors.As(err, &policy) {
		return true
	}
	var tenant *TenantError
	if errors.As(err, &tenant) {
		return true
	}
	var adapter *AdapterError
	if errors.As(err, &adapter) {
		return true
	}
	var libBug *LibBugError
	if errors.As(err, &libBug) {
		return true
	}
	return false
}

func errorWrapOriginFromString(origin string) Origin {
	switch origin {
	case ErrorOriginUserDSL:
		return OriginUserDSL
	case ErrorOriginCodegenBug:
		return OriginCodegenBug
	case ErrorOriginAdapterRuntime:
		return OriginAdapterRuntime
	case ErrorOriginLibInternal:
		return OriginLibInternal
	default:
		return OriginLibInternal
	}
}

func errorWrapJoinDistinct(first error, second error) error {
	switch {
	case first == nil:
		return second
	case second == nil:
		return first
	case errors.Is(first, second), errors.Is(second, first):
		return first
	default:
		return errors.Join(first, second)
	}
}

func errorWrapBaseIsZero(base ErrorBase) bool {
	return base.Code == "" &&
		base.Origin == 0 &&
		base.Status == 0 &&
		base.Message == "" &&
		base.Feature == "" &&
		base.Kind == "" &&
		base.Op == "" &&
		base.Source == "" &&
		base.Cause == nil
}
