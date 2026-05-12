package openapi

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"path"
	"strings"
	"unicode"
)

// ErrInvalidDocument is returned when an OpenAPI artifact fails Lazuli's
// dependency-free structural validation.
var ErrInvalidDocument = errors.New("lazuli/openapi: invalid document")

// ValidateDocumentJSON validates a JSON OpenAPI artifact.
//
// This is a small structural guard, not a full OpenAPI schema validator. It
// requires an OpenAPI 3.x root object with openapi, info, and paths fields,
// validates path and method keys after normalization, and checks that every
// present operationId is unique.
func ValidateDocumentJSON(data []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(data))

	var doc map[string]any
	if err := decoder.Decode(&doc); err != nil {
		if errors.Is(err, io.EOF) {
			return invalidDocument("empty JSON document")
		}
		return invalidDocument("decode JSON: %v", err)
	}

	var extra any
	err := decoder.Decode(&extra)
	if errors.Is(err, io.EOF) {
		return ValidateDocument(doc)
	}
	if err != nil {
		return invalidDocument("decode trailing JSON: %v", err)
	}
	return invalidDocument("unexpected trailing JSON value")
}

// ValidateDocument validates a decoded OpenAPI document.
//
// The input map is not modified. Callers that need canonical keys can use
// NormalizePath and NormalizeMethod before building the artifact map.
func ValidateDocument(doc map[string]any) error {
	if doc == nil {
		return invalidDocument("document must be a JSON object")
	}

	version, ok := doc["openapi"].(string)
	if !ok || strings.TrimSpace(version) == "" {
		return invalidDocument("openapi field is required")
	}
	if !strings.HasPrefix(strings.TrimSpace(version), "3.") {
		return invalidDocument("openapi version %q is not OpenAPI 3", version)
	}

	if _, ok := objectField(doc, "info"); !ok {
		return invalidDocument("info field must be an object")
	}

	paths, ok := objectField(doc, "paths")
	if !ok {
		return invalidDocument("paths field must be an object")
	}

	seenPaths := make(map[string]string, len(paths))
	seenOperationIDs := map[string]string{}
	for rawPath, rawPathItem := range paths {
		normalizedPath, err := NormalizePath(rawPath)
		if err != nil {
			return invalidDocument("path %q: %v", rawPath, err)
		}
		if previous, exists := seenPaths[normalizedPath]; exists {
			return invalidDocument("path %q normalizes to %q already used by %q", rawPath, normalizedPath, previous)
		}
		seenPaths[normalizedPath] = rawPath

		pathItem, ok := rawPathItem.(map[string]any)
		if !ok || pathItem == nil {
			return invalidDocument("path %q item must be an object", rawPath)
		}
		if err := validatePathItem(normalizedPath, pathItem, seenOperationIDs); err != nil {
			return err
		}
	}

	return nil
}

// NormalizePath returns the canonical OpenAPI path spelling used by Lazuli.
//
// It trims whitespace, adds a leading slash when omitted, converts ":name"
// path parameters to "{name}", collapses redundant separators, and rejects
// unsafe path shapes.
func NormalizePath(rawPath string) (string, error) {
	name := strings.TrimSpace(rawPath)
	if name == "" {
		return "", invalidDocument("path is required")
	}
	if strings.HasPrefix(name, "//") || strings.Contains(name, "://") {
		return "", invalidDocument("absolute paths and URLs are not allowed")
	}
	if strings.ContainsAny(name, "\\?#") {
		return "", invalidDocument("path must not contain backslashes, query strings, or fragments")
	}
	for _, ch := range name {
		if unicode.IsControl(ch) {
			return "", invalidDocument("path must not contain control characters")
		}
	}
	if !strings.HasPrefix(name, "/") {
		name = "/" + name
	}

	segments := strings.Split(name, "/")
	for i, segment := range segments {
		if segment == "" || segment == "." {
			continue
		}
		if segment == ".." {
			return "", invalidDocument("path must not contain parent traversal")
		}
		if strings.HasPrefix(segment, ":") {
			param := segment[1:]
			if isPathParamName(param) {
				segments[i] = "{" + param + "}"
			}
			continue
		}
		if strings.ContainsAny(segment, "{}") && !hasValidPathTemplates(segment) {
			return "", invalidDocument("invalid path template segment %q", segment)
		}
	}

	cleaned := path.Clean(strings.Join(segments, "/"))
	if cleaned == "." {
		return "/", nil
	}
	return cleaned, nil
}

// NormalizeMethod returns the canonical lower-case OpenAPI operation method.
func NormalizeMethod(method string) (string, error) {
	normalized := strings.ToLower(strings.TrimSpace(method))
	switch normalized {
	case "get", "put", "post", "delete", "options", "head", "patch", "trace":
		return normalized, nil
	default:
		return "", invalidDocument("unsupported HTTP method %q", method)
	}
}

func validatePathItem(normalizedPath string, pathItem map[string]any, seenOperationIDs map[string]string) error {
	seenMethods := map[string]string{}
	for rawMethod, rawOperation := range pathItem {
		if isPathItemMetadataField(rawMethod) {
			continue
		}
		if strings.HasPrefix(strings.ToLower(strings.TrimSpace(rawMethod)), "x-") {
			continue
		}

		method, err := NormalizeMethod(rawMethod)
		if err != nil {
			return invalidDocument("path %q method %q: %v", normalizedPath, rawMethod, err)
		}
		if previous, exists := seenMethods[method]; exists {
			return invalidDocument("path %q method %q normalizes to %q already used by %q", normalizedPath, rawMethod, method, previous)
		}
		seenMethods[method] = rawMethod

		operation, ok := rawOperation.(map[string]any)
		if !ok || operation == nil {
			return invalidDocument("path %q method %q operation must be an object", normalizedPath, rawMethod)
		}
		if err := validateOperationID(normalizedPath, method, operation, seenOperationIDs); err != nil {
			return err
		}
	}
	return nil
}

func validateOperationID(normalizedPath, method string, operation map[string]any, seen map[string]string) error {
	raw, exists := operation["operationId"]
	if !exists {
		return nil
	}
	operationID, ok := raw.(string)
	if !ok {
		return invalidDocument("path %q method %q operationId must be a string", normalizedPath, method)
	}
	operationID = strings.TrimSpace(operationID)
	if operationID == "" {
		return invalidDocument("path %q method %q operationId must not be empty", normalizedPath, method)
	}
	location := strings.ToUpper(method) + " " + normalizedPath
	if previous, exists := seen[operationID]; exists {
		return invalidDocument("operationId %q used by both %s and %s", operationID, previous, location)
	}
	seen[operationID] = location
	return nil
}

func objectField(doc map[string]any, name string) (map[string]any, bool) {
	value, exists := doc[name]
	if !exists {
		return nil, false
	}
	field, ok := value.(map[string]any)
	return field, ok && field != nil
}

func isPathItemMetadataField(field string) bool {
	switch strings.TrimSpace(field) {
	case "$ref", "summary", "description", "servers", "parameters":
		return true
	default:
		return false
	}
}

func hasValidPathTemplates(segment string) bool {
	for i := 0; i < len(segment); i++ {
		switch segment[i] {
		case '{':
			end := strings.IndexByte(segment[i+1:], '}')
			if end < 0 {
				return false
			}
			name := segment[i+1 : i+1+end]
			if !isPathParamName(name) {
				return false
			}
			i += end + 1
		case '}':
			return false
		}
	}
	return true
}

func isPathParamName(name string) bool {
	if name == "" {
		return false
	}
	for _, ch := range name {
		if (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') ||
			(ch >= '0' && ch <= '9') || ch == '_' {
			continue
		}
		return false
	}
	return true
}

func invalidDocument(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidDocument, fmt.Sprintf(format, args...))
}
