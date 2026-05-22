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
	queries         map[string]*queryErased
	queryHandlers   map[string]queryHandler
}{
	resources:       make(map[string]*resourceErased),
	commands:        make(map[string]*commandErased),
	commandHandlers: make(map[string]commandHandler),
	queries:         make(map[string]*queryErased),
	queryHandlers:   make(map[string]queryHandler),
}

// commandHandler is implemented by every typed Command[I, O] so the HTTP
// dispatcher can recover the JSON-decoded input without knowing I or O.
type commandHandler interface {
	dispatch(ctx *Ctx, raw json.RawMessage) (any, error)
}

// queryHandler is implemented by every typed Query[A, R] so the HTTP
// dispatcher can recover args and forward to the right Run* method without
// knowing A or R.
type queryHandler interface {
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
	if ctx != nil && c.WithSource != nil {
		ctx.Context = c.WithSource(ctx.Context)
	}
	var input I
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &input); err != nil {
			return nil, &Error{Status: 400, Code: CodeBadRequest,
				Message: "invalid JSON body: " + err.Error()}
		}
	}
	return c.Handle(ctx, input)
}

// register on Query[A, R] inserts the type-erased view AND stores the typed
// dispatcher so the HTTP layer can decode args and forward to the right
// Run* method.
func (q *Query[A, R]) register() {
	registry.Lock()
	defer registry.Unlock()
	if _, exists := registry.queries[q.Name]; exists {
		panic(fmt.Sprintf("lazuli: query %q registered twice", q.Name))
	}
	registry.queries[q.Name] = q.erased()
	registry.queryHandlers[q.Name] = q
}

// dispatch on Query[A, R] decodes args and delegates to RunList / RunLookup
// based on Kind. Implements `queryHandler`.
func (q *Query[A, R]) dispatch(ctx *Ctx, raw json.RawMessage) (any, error) {
	if ctx != nil && q.WithSource != nil {
		ctx.Context = q.WithSource(ctx.Context)
	}
	var args A
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &args); err != nil {
			return nil, &Error{Status: 400, Code: CodeBadRequest,
				Message: "invalid JSON body: " + err.Error()}
		}
	}
	switch q.Kind {
	case QueryList:
		return q.RunList(ctx, args)
	case QueryLookup:
		return q.RunLookup(ctx, args)
	case QueryView:
		return q.RunSQL(ctx, args)
	case QuerySQL:
		return nil, &Error{Status: 501, Code: CodeInternal,
			Message: "query.sql execution not yet implemented in runtime spike"}
	default:
		return nil, &Error{Status: 500, Code: CodeInternal,
			Message: fmt.Sprintf("unknown query kind: %d", q.Kind)}
	}
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

// LookupQuery returns the type-erased view of a registered query by name.
func LookupQuery(name string) *queryErased {
	registry.RLock()
	defer registry.RUnlock()
	return registry.queries[name]
}

// lookupQueryHandler returns the typed dispatcher for the given query.
func lookupQueryHandler(name string) queryHandler {
	registry.RLock()
	defer registry.RUnlock()
	return registry.queryHandlers[name]
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

// Queries returns a snapshot of every registered query.
func Queries() []*queryErased {
	registry.RLock()
	defer registry.RUnlock()
	out := make([]*queryErased, 0, len(registry.queries))
	for _, q := range registry.queries {
		out = append(out, q)
	}
	return out
}
