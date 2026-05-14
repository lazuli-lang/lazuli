// Package lifecycle wraps github.com/looplab/fsm with a thin generic adapter.
package lifecycle

import (
	"context"
	"fmt"

	"github.com/looplab/fsm"
)

// Transition is one named state-to-state move authored in a lifecycle block.
type Transition[E ~string] struct {
	Name string
	From []string
	To   E
}

// Machine is a stateless transition table for a string-backed state enum.
type Machine[E ~string] struct {
	transitions []Transition[E]
}

// New constructs a Machine seeded with the lifecycle transition table.
func New[E ~string](_ E, transitions []Transition[E]) *Machine[E] {
	return &Machine[E]{transitions: transitions}
}

// Can reports whether transition is valid from the given state.
func (m *Machine[E]) Can(from E, transition string) bool {
	return fsm.NewFSM(string(from), m.events(), fsm.Callbacks{}).Can(transition)
}

// Apply executes transition from current and returns the resulting state.
func (m *Machine[E]) Apply(ctx context.Context, current E, transition string) (E, error) {
	machine := fsm.NewFSM(string(current), m.events(), fsm.Callbacks{})
	if err := machine.Event(ctx, transition); err != nil {
		var zero E
		return zero, fmt.Errorf("lifecycle: transition %q invalid from state %q: %w", transition, string(current), err)
	}
	return E(machine.Current()), nil
}

func (m *Machine[E]) events() []fsm.EventDesc {
	events := make([]fsm.EventDesc, 0, len(m.transitions))
	for _, t := range m.transitions {
		events = append(events, fsm.EventDesc{Name: t.Name, Src: stringSlice(t.From), Dst: string(t.To)})
	}
	return events
}

func stringSlice(xs []string) []string {
	out := make([]string, len(xs))
	copy(out, xs)
	return out
}
