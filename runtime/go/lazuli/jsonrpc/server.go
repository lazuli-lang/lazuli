// Package jsonrpc provides a small JSON-RPC 2.0 method registry and HTTP handler.
package jsonrpc

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"sync"
)

const (
	// Version is the JSON-RPC protocol version this package speaks.
	Version = "2.0"

	// ContentType is the response Content-Type used by Server.
	ContentType = "application/json"
)

// ErrorCode is a JSON-RPC 2.0 error code.
type ErrorCode int

const (
	// CodeParseError means the server received invalid JSON.
	CodeParseError ErrorCode = -32700

	// CodeInvalidRequest means the JSON value is not a valid JSON-RPC request.
	CodeInvalidRequest ErrorCode = -32600

	// CodeMethodNotFound means the requested method is not registered.
	CodeMethodNotFound ErrorCode = -32601

	// CodeInvalidParams means the method parameters are invalid.
	CodeInvalidParams ErrorCode = -32602

	// CodeInternalError means the server hit an internal error while handling the request.
	CodeInternalError ErrorCode = -32603

	// CodeServerError is the first implementation-defined server error code.
	CodeServerError ErrorCode = -32000
)

// Request is a JSON-RPC 2.0 request object.
type Request struct {
	JSONRPC string          `json:"jsonrpc"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
	ID      json.RawMessage `json:"id,omitempty"`
}

// Notification reports whether r omits the id member and therefore must not
// receive a JSON-RPC response.
func (r Request) Notification() bool {
	return len(bytes.TrimSpace(r.ID)) == 0
}

// Response is a JSON-RPC 2.0 response object. Exactly one of Result or Error is
// written by MarshalJSON.
type Response struct {
	JSONRPC string          `json:"jsonrpc"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *Error          `json:"error,omitempty"`
	ID      json.RawMessage `json:"id,omitempty"`
}

// MarshalJSON writes a JSON-RPC response with either result or error, including
// a null result for successful nil method results.
func (r Response) MarshalJSON() ([]byte, error) {
	version := r.JSONRPC
	if version == "" {
		version = Version
	}
	id := r.ID
	if len(bytes.TrimSpace(id)) == 0 {
		id = nullID()
	}

	if r.Error != nil {
		return json.Marshal(struct {
			JSONRPC string          `json:"jsonrpc"`
			Error   *Error          `json:"error"`
			ID      json.RawMessage `json:"id"`
		}{
			JSONRPC: version,
			Error:   r.Error,
			ID:      id,
		})
	}

	result := r.Result
	if len(bytes.TrimSpace(result)) == 0 {
		result = nullID()
	}
	return json.Marshal(struct {
		JSONRPC string          `json:"jsonrpc"`
		Result  json.RawMessage `json:"result"`
		ID      json.RawMessage `json:"id"`
	}{
		JSONRPC: version,
		Result:  result,
		ID:      id,
	})
}

// Error is a JSON-RPC 2.0 error object.
type Error struct {
	Code    ErrorCode `json:"code"`
	Message string    `json:"message"`
	Data    any       `json:"data,omitempty"`
}

// Error implements the error interface.
func (e *Error) Error() string {
	if e == nil {
		return "<nil>"
	}
	return fmt.Sprintf("jsonrpc: %d %s", e.Code, e.Message)
}

// NewError returns a JSON-RPC error with the default message for code when
// message is empty.
func NewError(code ErrorCode, message string) *Error {
	if message == "" {
		message = code.DefaultMessage()
	}
	return &Error{Code: code, Message: message}
}

// NewErrorWithData returns a JSON-RPC error with an optional data value.
func NewErrorWithData(code ErrorCode, message string, data any) *Error {
	err := NewError(code, message)
	err.Data = data
	return err
}

// DefaultMessage returns the standard message for a JSON-RPC error code.
func (c ErrorCode) DefaultMessage() string {
	switch c {
	case CodeParseError:
		return "parse error"
	case CodeInvalidRequest:
		return "invalid request"
	case CodeMethodNotFound:
		return "method not found"
	case CodeInvalidParams:
		return "invalid params"
	case CodeInternalError:
		return "internal error"
	default:
		return "server error"
	}
}

// HandlerFunc handles one JSON-RPC method call.
type HandlerFunc func(context.Context, Request) (any, error)

// Registry stores JSON-RPC methods by name. The zero value is ready to use.
type Registry struct {
	mu      sync.RWMutex
	methods map[string]HandlerFunc
}

// NewRegistry returns an empty method registry.
func NewRegistry() *Registry {
	return &Registry{methods: map[string]HandlerFunc{}}
}

// Register records a method handler. It panics for empty names, nil handlers,
// or duplicate registrations.
func (r *Registry) Register(method string, handler HandlerFunc) {
	if method == "" {
		panic("lazuli/jsonrpc: method name is empty")
	}
	if handler == nil {
		panic("lazuli/jsonrpc: method handler is nil")
	}

	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	if _, exists := r.methods[method]; exists {
		panic("lazuli/jsonrpc: method " + method + " registered twice")
	}
	r.methods[method] = handler
}

// Lookup returns the registered handler for method.
func (r *Registry) Lookup(method string) (HandlerFunc, bool) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	handler, ok := r.methods[method]
	return handler, ok
}

func (r *Registry) initLocked() {
	if r.methods == nil {
		r.methods = map[string]HandlerFunc{}
	}
}

// Server is an HTTP handler and JSON-RPC method dispatcher.
type Server struct {
	registry *Registry
}

// NewServer returns a JSON-RPC server backed by registry. When registry is nil,
// a new empty registry is used.
func NewServer(registry *Registry) *Server {
	if registry == nil {
		registry = NewRegistry()
	}
	return &Server{registry: registry}
}

// Register records a method handler on the server's registry.
func (s *Server) Register(method string, handler HandlerFunc) {
	s.ensureRegistry().Register(method, handler)
}

// ServeHTTP handles JSON-RPC 2.0 requests over POST. It returns 204 when a
// request or batch contains only valid notifications.
func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", http.MethodPost)
		http.Error(w, http.StatusText(http.StatusMethodNotAllowed), http.StatusMethodNotAllowed)
		return
	}

	var body []byte
	var err error
	if r.Body != nil {
		defer r.Body.Close()
		body, err = io.ReadAll(r.Body)
	}
	if err != nil {
		s.writeSingle(w, errorResponse(nullID(), NewError(CodeParseError, "failed to read request body")))
		return
	}

	payload, ok := s.handleBody(r.Context(), body)
	if !ok {
		w.WriteHeader(http.StatusNoContent)
		return
	}

	w.Header().Set("Content-Type", ContentType)
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(payload)
}

func (s *Server) ensureRegistry() *Registry {
	if s.registry != nil {
		return s.registry
	}
	s.registry = NewRegistry()
	return s.registry
}

func (s *Server) handleBody(ctx context.Context, body []byte) ([]byte, bool) {
	trimmed := bytes.TrimSpace(body)
	if len(trimmed) == 0 {
		return mustMarshal(errorResponse(nullID(), NewError(CodeParseError, ""))), true
	}
	if !json.Valid(trimmed) {
		return mustMarshal(errorResponse(nullID(), NewError(CodeParseError, ""))), true
	}

	if trimmed[0] == '[' {
		return s.handleBatch(ctx, trimmed)
	}

	response, ok := s.handleRaw(ctx, trimmed)
	if !ok {
		return nil, false
	}
	return mustMarshal(response), true
}

func (s *Server) handleBatch(ctx context.Context, body []byte) ([]byte, bool) {
	var batch []json.RawMessage
	if err := json.Unmarshal(body, &batch); err != nil {
		return mustMarshal(errorResponse(nullID(), NewError(CodeParseError, ""))), true
	}
	if len(batch) == 0 {
		return mustMarshal(errorResponse(nullID(), NewError(CodeInvalidRequest, ""))), true
	}

	responses := make([]Response, 0, len(batch))
	for _, raw := range batch {
		response, ok := s.handleRaw(ctx, raw)
		if ok {
			responses = append(responses, response)
		}
	}
	if len(responses) == 0 {
		return nil, false
	}
	return mustMarshal(responses), true
}

func (s *Server) handleRaw(ctx context.Context, raw json.RawMessage) (Response, bool) {
	req, rpcErr := parseRequest(raw)
	if rpcErr != nil {
		return errorResponse(req.ID, rpcErr), true
	}

	if req.Notification() {
		_ = s.dispatch(ctx, req)
		return Response{}, false
	}

	return s.dispatch(ctx, req), true
}

func (s *Server) dispatch(ctx context.Context, req Request) Response {
	handler, ok := s.ensureRegistry().Lookup(req.Method)
	if !ok {
		return errorResponse(req.ID, NewError(CodeMethodNotFound, "method not found: "+req.Method))
	}

	result, err := handler(ctx, req)
	if err != nil {
		return errorResponse(req.ID, rpcError(err))
	}

	payload, err := json.Marshal(result)
	if err != nil {
		return errorResponse(req.ID, NewError(CodeInternalError, "failed to encode method result"))
	}
	return successResponse(req.ID, payload)
}

func parseRequest(raw json.RawMessage) (Request, *Error) {
	var req Request
	if err := json.Unmarshal(raw, &req); err != nil {
		if len(req.ID) > 0 && validID(req.ID) {
			return Request{ID: cloneRaw(req.ID)}, NewError(CodeInvalidRequest, "")
		}
		return Request{ID: nullID()}, NewError(CodeInvalidRequest, "")
	}
	if len(req.ID) > 0 && !validID(req.ID) {
		return Request{ID: nullID()}, NewError(CodeInvalidRequest, "")
	}
	if req.JSONRPC != Version {
		return requestError(req), NewError(CodeInvalidRequest, "")
	}
	if req.Method == "" {
		return requestError(req), NewError(CodeInvalidRequest, "")
	}
	return req, nil
}

func requestError(req Request) Request {
	if len(req.ID) == 0 {
		req.ID = nullID()
	}
	return req
}

func validID(raw json.RawMessage) bool {
	var value any
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	if err := decoder.Decode(&value); err != nil {
		return false
	}
	switch value.(type) {
	case nil, string, json.Number:
		return true
	default:
		return false
	}
}

func rpcError(err error) *Error {
	var rpcErr *Error
	if errors.As(err, &rpcErr) {
		if rpcErr.Message == "" {
			clone := *rpcErr
			clone.Message = clone.Code.DefaultMessage()
			return &clone
		}
		return rpcErr
	}
	return NewError(CodeInternalError, "")
}

func successResponse(id json.RawMessage, result json.RawMessage) Response {
	return Response{
		JSONRPC: Version,
		Result:  cloneRaw(result),
		ID:      cloneRaw(id),
	}
}

func errorResponse(id json.RawMessage, err *Error) Response {
	if err == nil {
		err = NewError(CodeInternalError, "")
	}
	return Response{
		JSONRPC: Version,
		Error:   err,
		ID:      cloneRaw(id),
	}
}

func nullID() json.RawMessage {
	return json.RawMessage("null")
}

func cloneRaw(raw json.RawMessage) json.RawMessage {
	if len(raw) == 0 {
		return nil
	}
	return append(json.RawMessage(nil), raw...)
}

func mustMarshal(v any) []byte {
	payload, err := json.Marshal(v)
	if err == nil {
		return payload
	}
	return []byte(`{"jsonrpc":"2.0","error":{"code":-32603,"message":"internal error"},"id":null}`)
}

func (s *Server) writeSingle(w http.ResponseWriter, response Response) {
	w.Header().Set("Content-Type", ContentType)
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(mustMarshal(response))
}
