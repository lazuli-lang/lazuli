package lazuli

import (
	"encoding/json"
	"fmt"
	"sync"
)

// registry is the process-global directory of every Resource, Command,
// Query, etc. that generated code declares. `dist/<feature>/*.gen.go`
// `init()` blocks call `Register(...)` to populate it.
//
// The runtime looks up handlers by name when serving HTTP / dispatching jobs.
var registry = struct {
	sync.RWMutex
	resources       map[string]*resourceErased
	commands        map[string]*commandErased
	commandHandlers map[string]commandHandler
}{
	resources:       make(map[string]*resourceErased),
	commands:        make(map[string]*commandErased),
	commandHandlers: make(map[string]commandHandler),
}

// commandHandler is implemented by every typed Command[I, O] so the HTTP
// dispatcher can recover the JSON-decoded input without knowing I or O.
type commandHandler interface {
	dispatch(ctx *Ctx, raw json.RawMessage) (any, error)
}

// Registerable is implemented by Resource, Command, Query, etc. so generated
// code calls `lazuli.Register(&customer, &createCustomer, ...)` in one
// variadic line.
type Registerable interface {
	register()
}

// Register adds one or more Lazuli declarations to the global registry.
// Generated `init()` functions call this.
func Register(items ...Registerable) {
	for _, item := range items {
		item.register()
	}
}

// register on Resource[T] inserts the type-erased view into the registry.
func (r *Resource[T]) register() {
	registry.Lock()
	defer registry.Unlock()
	if _, exists := registry.resources[r.Name]; exists {
		panic(fmt.Sprintf("lazuli: resource %q registered twice", r.Name))
	}
	registry.resources[r.Name] = r.erased()
}

// register on Command[I, O] inserts the type-erased view AND stores the
// typed dispatcher so the HTTP layer can decode input and forward to Handle.
func (c *Command[I, O]) register() {
	registry.Lock()
	defer registry.Unlock()
	if _, exists := registry.commands[c.Name]; exists {
		panic(fmt.Sprintf("lazuli: command %q registered twice", c.Name))
	}
	registry.commands[c.Name] = c.erased()
	registry.commandHandlers[c.Name] = c
}

// dispatch on Command[I, O] decodes raw JSON into I and forwards to Handle.
// Implements `commandHandler` for the dispatcher.
func (c *Command[I, O]) dispatch(ctx *Ctx, raw json.RawMessage) (any, error) {
	var input I
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &input); err != nil {
			return nil, &Error{Status: 400, Code: CodeBadRequest,
				Message: "invalid JSON body: " + err.Error()}
		}
	}
	return c.Handle(ctx, input)
}

// LookupResource returns the type-erased view of a registered resource by
// name, or nil. Used by the dispatcher and tests.
func LookupResource(name string) *resourceErased {
	registry.RLock()
	defer registry.RUnlock()
	return registry.resources[name]
}

// LookupCommand returns the type-erased view of a registered command by name.
func LookupCommand(name string) *commandErased {
	registry.RLock()
	defer registry.RUnlock()
	return registry.commands[name]
}

// lookupCommandHandler returns the typed dispatcher for the given command.
func lookupCommandHandler(name string) commandHandler {
	registry.RLock()
	defer registry.RUnlock()
	return registry.commandHandlers[name]
}

// Resources returns a snapshot of every registered resource. Used at startup
// for migrations, route registration, and observability hooks.
func Resources() []*resourceErased {
	registry.RLock()
	defer registry.RUnlock()
	out := make([]*resourceErased, 0, len(registry.resources))
	for _, r := range registry.resources {
		out = append(out, r)
	}
	return out
}

// Commands returns a snapshot of every registered command.
func Commands() []*commandErased {
	registry.RLock()
	defer registry.RUnlock()
	out := make([]*commandErased, 0, len(registry.commands))
	for _, c := range registry.commands {
		out = append(out, c)
	}
	return out
}
