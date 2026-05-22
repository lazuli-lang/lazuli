package lazuli

import (
	"encoding/json"
	"errors"
)

// TypedDecode safely re-decodes widened handler input into a concrete target.
func TypedDecode(input any, target any) error {
	raw, err := json.Marshal(input)
	if err != nil {
		return typedDecodeError(err)
	}
	if err := json.Unmarshal(raw, target); err != nil {
		return typedDecodeError(err)
	}
	return nil
}

func typedDecodeError(err error) *Error {
	fields := []string{}
	var typeErr *json.UnmarshalTypeError
	var invalidErr *json.InvalidUnmarshalError
	if errors.As(err, &typeErr) && typeErr.Field != "" {
		fields = append(fields, typeErr.Field)
	} else if errors.As(err, &invalidErr) {
		fields = append(fields, "target")
	}
	return &Error{
		Status:     400,
		Code:       CodeValidationFailed,
		Message:    "validation failed",
		MessageKey: CodeValidationFailed,
		Data:       map[string]any{"fields": fields},
		Base:       ErrorBase{Status: 400, Code: CodeValidationFailed, Message: "validation failed", Cause: err},
	}
}
