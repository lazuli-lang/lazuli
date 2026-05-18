// Registry adapters that resolve user-authored MCP handlers at
// invocation time. Mirrors the pattern used by
// `runtime/go/lazuli/handler_registry.go` for command/query handlers,
// but typed for the MCP handler shapes (`ToolHandler`,
// `ResourceHandler`, `PromptHandler`).
//
// Codegen emits `mcp.ToolHandlerFromRegistry("<feature>.<name>")` in
// the generated mcp_server registration, and user code registers the
// matching handler with `mcp.RegisterTool` / `RegisterResource` /
// `RegisterPrompt` in its package `init()`.
package mcp

import (
	"context"
	"fmt"
	"sync"
)

var (
	registryMu  sync.RWMutex
	toolReg     = map[string]ToolHandler{}
	resourceReg = map[string]ResourceHandler{}
	promptReg   = map[string]PromptHandler{}
)

// RegisterTool registers a user-authored tool handler under name. The
// codegen-emitted `ServerRegistration` resolves handlers by name via
// `ToolHandlerFromRegistry`; the user-side init() registers them here.
//
//	package knowledgehandlers
//
//	func SearchSlugs(ctx context.Context, args map[string]any) (any, error) { ... }
//
//	func init() {
//	    mcp.RegisterTool("knowledge.search_slugs", SearchSlugs)
//	}
//
// Idempotent — last registration wins. Doctor `MCP-HANDLER-001`
// (framework-side) flags duplicate registrations as structural errors.
func RegisterTool(name string, h ToolHandler) {
	registryMu.Lock()
	defer registryMu.Unlock()
	toolReg[name] = h
}

// RegisterResource registers a user-authored resource handler.
func RegisterResource(name string, h ResourceHandler) {
	registryMu.Lock()
	defer registryMu.Unlock()
	resourceReg[name] = h
}

// RegisterPrompt registers a user-authored prompt handler.
func RegisterPrompt(name string, h PromptHandler) {
	registryMu.Lock()
	defer registryMu.Unlock()
	promptReg[name] = h
}

// ToolHandlerFromRegistry returns a `ToolHandler` that looks up the
// underlying handler at invocation time. Used in generated code to
// keep the gen package free of import cycles into user-authored code.
func ToolHandlerFromRegistry(name string) ToolHandler {
	return func(ctx context.Context, args map[string]any) (any, error) {
		registryMu.RLock()
		h, ok := toolReg[name]
		registryMu.RUnlock()
		if !ok {
			return nil, fmt.Errorf("%w: tool %q not registered", ErrMCPUnknownTool, name)
		}
		return h(ctx, args)
	}
}

// ResourceHandlerFromRegistry returns a `ResourceHandler` that resolves
// at invocation time.
func ResourceHandlerFromRegistry(name string) ResourceHandler {
	return func(ctx context.Context, uri string) ([]byte, string, error) {
		registryMu.RLock()
		h, ok := resourceReg[name]
		registryMu.RUnlock()
		if !ok {
			return nil, "", fmt.Errorf("%w: resource %q not registered", ErrMCPUnknownResource, name)
		}
		return h(ctx, uri)
	}
}

// PromptHandlerFromRegistry returns a `PromptHandler` that resolves
// at invocation time.
func PromptHandlerFromRegistry(name string) PromptHandler {
	return func(ctx context.Context, args map[string]any) ([]PromptMessage, error) {
		registryMu.RLock()
		h, ok := promptReg[name]
		registryMu.RUnlock()
		if !ok {
			return nil, fmt.Errorf("%w: prompt %q not registered", ErrMCPUnknownPrompt, name)
		}
		return h(ctx, args)
	}
}

var (
	serverRegMu       sync.RWMutex
	serverRegistrations []ServerRegistration
)

// RegisterServer adds a generated ServerRegistration to the global
// registry. Codegen emits one call per `mcp_server <name>` block in
// the package init() of the feature's `mcp_server.gen.go`. The Lazuli
// runtime boot path enumerates the registry and starts each server's
// configured transport in parallel.
func RegisterServer(reg ServerRegistration) {
	serverRegMu.Lock()
	defer serverRegMu.Unlock()
	serverRegistrations = append(serverRegistrations, reg)
}

// RegisteredServers returns a snapshot of all servers registered via
// RegisterServer. The Lazuli runtime calls this from `lazuli.Boot`
// (or the user's main.go after init() runs).
func RegisteredServers() []ServerRegistration {
	serverRegMu.RLock()
	defer serverRegMu.RUnlock()
	out := make([]ServerRegistration, len(serverRegistrations))
	copy(out, serverRegistrations)
	return out
}
