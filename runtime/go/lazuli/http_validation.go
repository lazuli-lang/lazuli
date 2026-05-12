package lazuli

import "net/http"

// RequestValidator validates an inbound HTTP request before the generated
// handler decodes and dispatches it.
type RequestValidator interface {
	Validate(*http.Request) []ValidationViolation
}

// RequestValidatorFunc adapts a function to RequestValidator.
type RequestValidatorFunc func(*http.Request) []ValidationViolation

// Validate implements RequestValidator.
func (f RequestValidatorFunc) Validate(r *http.Request) []ValidationViolation {
	if f == nil {
		return nil
	}
	return f(r)
}

// ValidationViolation describes one request validation failure. Status is not
// encoded into the Problem JSON; use it to classify malformed request-shape
// failures as 400. Zero or 422 classify as semantic validation failures.
type ValidationViolation struct {
	Location string `json:"location,omitempty"`
	Field    string `json:"field,omitempty"`
	Code     string `json:"code,omitempty"`
	Message  string `json:"message,omitempty"`
	Status   int    `json:"-"`
}

// RequestValidationMiddleware runs validators before the next handler. It
// aggregates every violation so generated handlers can return complete field
// feedback in one Problem response. If any violation is classified as 400 the
// response status is 400; otherwise validation failures use 422.
func RequestValidationMiddleware(validators ...RequestValidator) Middleware {
	return func(next http.Handler) http.Handler {
		if len(validators) == 0 {
			return next
		}

		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			violations := collectRequestValidationViolations(r, validators)
			if len(violations) > 0 {
				WriteProblem(w, requestValidationProblem(violations))
				return
			}

			next.ServeHTTP(w, r)
		})
	}
}

func collectRequestValidationViolations(r *http.Request, validators []RequestValidator) []ValidationViolation {
	var violations []ValidationViolation
	for _, validator := range validators {
		if validator == nil {
			continue
		}
		violations = append(violations, validator.Validate(r)...)
	}
	return violations
}

func requestValidationProblem(violations []ValidationViolation) Problem {
	status := requestValidationStatus(violations)
	code := CodeValidationFailed
	if status == http.StatusBadRequest {
		code = CodeBadRequest
	}

	return Problem{
		Status: status,
		Detail: "request validation failed",
		Extensions: map[string]any{
			"code":       code,
			"violations": violations,
		},
	}
}

func requestValidationStatus(violations []ValidationViolation) int {
	for _, violation := range violations {
		if violation.Status == http.StatusBadRequest {
			return http.StatusBadRequest
		}
	}
	return http.StatusUnprocessableEntity
}
