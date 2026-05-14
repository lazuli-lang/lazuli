package lazuli

import (
	"errors"
	"strings"
	"sync"
)

// Registry holds every resource/command/query registered at boot.
// Generated init() blocks call RegisterResource/Command/Query.
type Registry struct {
	mu        sync.RWMutex
	resources map[string]*resourceErased
	commands  map[string]commandRegistration
	queries   map[string]queryRegistration
	apis      map[string]apiRegistration
}

// GlobalRegistry is the process-wide typed registry singleton.
var GlobalRegistry = &Registry{
	resources: map[string]*resourceErased{},
	commands:  map[string]commandRegistration{},
	queries:   map[string]queryRegistration{},
	apis:      map[string]apiRegistration{},
}

type commandRegistration struct{ Name, Feature string }
type queryRegistration struct{ Name, Feature string }
type apiRegistration struct{ Name, Feature, Path string }

// RegisterResource adds a type-erased resource to this registry.
func (r *Registry) RegisterResource(name string, erased *resourceErased) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	if _, exists := r.resources[name]; exists {
		panic("lazuli: resource " + name + " registered twice")
	}
	r.resources[name] = erased
}

// RegisterCommand adds a command registration to this registry.
func (r *Registry) RegisterCommand(reg commandRegistration) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	if _, exists := r.commands[reg.Name]; exists {
		panic("lazuli: command " + reg.Name + " registered twice")
	}
	r.commands[reg.Name] = reg
}

// RegisterQuery adds a query registration to this registry.
func (r *Registry) RegisterQuery(reg queryRegistration) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	if _, exists := r.queries[reg.Name]; exists {
		panic("lazuli: query " + reg.Name + " registered twice")
	}
	r.queries[reg.Name] = reg
}

// RegisterApi adds an API registration to this registry.
func (r *Registry) RegisterApi(reg apiRegistration) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	if _, exists := r.apis[reg.Name]; exists {
		panic("lazuli: api " + reg.Name + " registered twice")
	}
	r.apis[reg.Name] = reg
}

// Resources returns a snapshot of every resource in this registry.
func (r *Registry) Resources() map[string]*resourceErased {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	r.syncLegacyLocked()

	out := make(map[string]*resourceErased, len(r.resources))
	for name, resource := range r.resources {
		out[name] = resource
	}
	return out
}

// Commands returns a snapshot of every command in this registry.
func (r *Registry) Commands() map[string]commandRegistration {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	r.syncLegacyLocked()

	out := make(map[string]commandRegistration, len(r.commands))
	for name, command := range r.commands {
		out[name] = command
	}
	return out
}

// Queries returns a snapshot of every query in this registry.
func (r *Registry) Queries() map[string]queryRegistration {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	r.syncLegacyLocked()

	out := make(map[string]queryRegistration, len(r.queries))
	for name, query := range r.queries {
		out[name] = query
	}
	return out
}

// Apis returns a snapshot of every API in this registry.
func (r *Registry) Apis() map[string]apiRegistration {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.initLocked()
	r.syncLegacyLocked()

	out := make(map[string]apiRegistration, len(r.apis))
	for name, api := range r.apis {
		out[name] = api
	}
	return out
}

// QueryContract is the public descriptor of a registered query.
type QueryContract struct {
	Name    string
	Feature string
}

// ErrQueryNotFound is returned by LookupQuery when no query is registered under that name.
var ErrQueryNotFound = errors.New("lazuli: query not found")

// LookupQuery returns the QueryContract registered under name n, or ErrQueryNotFound.
func (r *Registry) LookupQuery(n string) (*QueryContract, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	if q, ok := r.queries[n]; ok {
		return &QueryContract{Name: q.Name, Feature: q.Feature}, nil
	}
	return nil, ErrQueryNotFound
}

// RegisterResource registers a Resource in the typed registry and the legacy
// erased dispatcher registry.
func RegisterResource[T any](resource *Resource[T]) {
	erased := resource.erased()
	GlobalRegistry.mu.Lock()
	registry.Lock()
	defer registry.Unlock()
	defer GlobalRegistry.mu.Unlock()

	if _, exists := GlobalRegistry.resources[resource.Name]; exists {
		panic("lazuli: resource " + resource.Name + " registered twice")
	}
	if _, exists := registry.resources[resource.Name]; exists {
		panic("lazuli: resource " + resource.Name + " registered twice")
	}
	GlobalRegistry.resources[resource.Name] = erased
	registry.resources[resource.Name] = erased
}

// RegisterCommand registers a Command in the typed registry and the legacy
// erased dispatcher registry.
func RegisterCommand[I, O any](command *Command[I, O]) {
	erased := command.erased()
	reg := commandRegistration{Name: command.Name, Feature: featureFromResource(command.Resource, command.Name)}

	GlobalRegistry.mu.Lock()
	registry.Lock()
	defer registry.Unlock()
	defer GlobalRegistry.mu.Unlock()

	if _, exists := GlobalRegistry.commands[command.Name]; exists {
		panic("lazuli: command " + command.Name + " registered twice")
	}
	if _, exists := registry.commands[command.Name]; exists {
		panic("lazuli: command " + command.Name + " registered twice")
	}
	GlobalRegistry.commands[command.Name] = reg
	registry.commands[command.Name] = erased
	registry.commandHandlers[command.Name] = command
}

// RegisterQuery registers a Query in the typed registry and the legacy erased
// dispatcher registry.
func RegisterQuery[A, R any](query *Query[A, R]) {
	erased := query.erased()
	reg := queryRegistration{Name: query.Name, Feature: featureFromResource(query.Resource, query.Name)}

	GlobalRegistry.mu.Lock()
	registry.Lock()
	defer registry.Unlock()
	defer GlobalRegistry.mu.Unlock()

	if _, exists := GlobalRegistry.queries[query.Name]; exists {
		panic("lazuli: query " + query.Name + " registered twice")
	}
	if _, exists := registry.queries[query.Name]; exists {
		panic("lazuli: query " + query.Name + " registered twice")
	}
	GlobalRegistry.queries[query.Name] = reg
	registry.queries[query.Name] = erased
	registry.queryHandlers[query.Name] = query
}

// RegisterApi registers an Api in the typed registry.
func RegisterApi[I, O any](api *Api[I, O]) {
	GlobalRegistry.RegisterApi(apiRegistration{
		Name:    api.Name,
		Feature: api.Feature,
		Path:    api.Path,
	})
}

func (api *Api[I, O]) register() {
	RegisterApi(api)
}

func (r *Registry) initLocked() {
	if r.resources == nil {
		r.resources = map[string]*resourceErased{}
	}
	if r.commands == nil {
		r.commands = map[string]commandRegistration{}
	}
	if r.queries == nil {
		r.queries = map[string]queryRegistration{}
	}
	if r.apis == nil {
		r.apis = map[string]apiRegistration{}
	}
}

func (r *Registry) syncLegacyLocked() {
	if r != GlobalRegistry {
		return
	}

	registry.RLock()
	defer registry.RUnlock()

	for name, resource := range registry.resources {
		if _, exists := r.resources[name]; !exists {
			r.resources[name] = resource
		}
	}
	for name, command := range registry.commands {
		if _, exists := r.commands[name]; !exists {
			r.commands[name] = commandRegistration{
				Name:    command.Name,
				Feature: featureFromResource(command.Resource, command.Name),
			}
		}
	}
	for name, query := range registry.queries {
		if _, exists := r.queries[name]; !exists {
			r.queries[name] = queryRegistration{
				Name:    query.Name,
				Feature: featureFromResource(query.Resource, query.Name),
			}
		}
	}
}

func featureFromResource(resource any, fallbackName string) string {
	if eraser, ok := resource.(interface{ erased() *resourceErased }); ok {
		return eraser.erased().Feature
	}
	return featureFromQualifiedName(fallbackName)
}

func featureFromQualifiedName(name string) string {
	if idx := strings.IndexByte(name, '.'); idx >= 0 {
		return name[:idx]
	}
	return ""
}
