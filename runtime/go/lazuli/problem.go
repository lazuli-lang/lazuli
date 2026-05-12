package lazuli

import (
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
)

const defaultProblemType = "about:blank"

// Problem is an RFC 9457 problem details object.
//
// Extensions are encoded as top-level JSON members alongside type, title,
// status, detail, and instance. Extension names that collide with standard
// problem member names are ignored when encoding.
type Problem struct {
	Type       string         `json:"type,omitempty"`
	Title      string         `json:"title,omitempty"`
	Status     int            `json:"status,omitempty"`
	Detail     string         `json:"detail,omitempty"`
	Instance   string         `json:"instance,omitempty"`
	Extensions map[string]any `json:"-"`
}

// MarshalJSON encodes p with extension members flattened into the problem
// object, as defined by RFC 9457.
func (p Problem) MarshalJSON() ([]byte, error) {
	object := make(map[string]any, len(p.Extensions)+5)
	if p.Type != "" {
		object["type"] = p.Type
	}
	if p.Title != "" {
		object["title"] = p.Title
	}
	if p.Status != 0 {
		object["status"] = p.Status
	}
	if p.Detail != "" {
		object["detail"] = p.Detail
	}
	if p.Instance != "" {
		object["instance"] = p.Instance
	}
	for name, value := range p.Extensions {
		if name == "" || isStandardProblemMember(name) {
			continue
		}
		object[name] = value
	}
	return json.Marshal(object)
}

// UnmarshalJSON decodes p and stores non-standard problem members in
// Extensions.
func (p *Problem) UnmarshalJSON(data []byte) error {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}

	var problem Problem
	for name, value := range raw {
		switch name {
		case "type":
			if err := json.Unmarshal(value, &problem.Type); err != nil {
				return err
			}
		case "title":
			if err := json.Unmarshal(value, &problem.Title); err != nil {
				return err
			}
		case "status":
			if err := json.Unmarshal(value, &problem.Status); err != nil {
				return err
			}
		case "detail":
			if err := json.Unmarshal(value, &problem.Detail); err != nil {
				return err
			}
		case "instance":
			if err := json.Unmarshal(value, &problem.Instance); err != nil {
				return err
			}
		default:
			if problem.Extensions == nil {
				problem.Extensions = make(map[string]any)
			}
			var extension any
			if err := json.Unmarshal(value, &extension); err != nil {
				return err
			}
			problem.Extensions[name] = extension
		}
	}

	*p = problem
	return nil
}

// WriteProblem writes problem as an application/problem+json HTTP response.
// Missing type and title values default to about:blank and the HTTP status
// text. A missing or invalid status defaults to 500.
func WriteProblem(w http.ResponseWriter, problem Problem) {
	problem = normalizeProblem(problem)
	w.Header().Set("Content-Type", "application/problem+json")
	w.WriteHeader(problem.Status)
	if err := json.NewEncoder(w).Encode(problem); err != nil {
		slog.Error("lazuli: failed to encode problem response", "error", err)
	}
}

// ProblemFromError converts err into a problem details response. Lazuli Error
// values preserve Code and Data as extension members named "code" and "data".
func ProblemFromError(err error) Problem {
	if err == nil {
		return normalizeProblem(Problem{Status: http.StatusInternalServerError})
	}

	var le *Error
	if errors.As(err, &le) && le != nil {
		status := le.Status
		if status == 0 {
			status = http.StatusInternalServerError
		}
		extensions := make(map[string]any, 2)
		if le.Code != "" {
			extensions["code"] = le.Code
		}
		if le.Data != nil {
			extensions["data"] = le.Data
		}
		detail := le.Message
		if detail == "" {
			detail = err.Error()
		}
		return normalizeProblem(Problem{
			Type:       defaultProblemType,
			Status:     status,
			Detail:     detail,
			Extensions: extensions,
		})
	}

	return normalizeProblem(Problem{
		Status: http.StatusInternalServerError,
		Detail: err.Error(),
		Extensions: map[string]any{
			"code": CodeInternal,
		},
	})
}

func normalizeProblem(problem Problem) Problem {
	if problem.Type == "" {
		problem.Type = defaultProblemType
	}
	if problem.Status < 100 || problem.Status > 599 {
		problem.Status = http.StatusInternalServerError
	}
	if problem.Title == "" {
		problem.Title = http.StatusText(problem.Status)
		if problem.Title == "" {
			problem.Title = "HTTP Error"
		}
	}
	return problem
}

func isStandardProblemMember(name string) bool {
	switch name {
	case "type", "title", "status", "detail", "instance":
		return true
	default:
		return false
	}
}
