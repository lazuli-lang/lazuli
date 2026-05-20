// Scalar fixture providers are plugin-owned generators for semantic scalar
// types. Tooling consumes this narrow contract without learning each plugin's
// validation rules or package layout.

export interface ScalarFixtureProvider<T = string> {
  // Produce a fresh valid value of the type. Every plugin must implement this.
  generate(): T;
  // Optional batch helper. Callers can derive a default from generate().
  generateMany?(n: number): T[];
  // Optional stable canonical value for snapshots and examples.
  readonly example?: T;
  // Optional generator for a value that fails validation.
  invalid?(): T;
}

export type ScalarFixtures = Record<string, ScalarFixtureProvider>;
