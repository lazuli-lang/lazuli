# Wave B.2 Status

Implementation commit: `5c3424dc482c8a86eadcf74a645f366b158542a2`

Helper paths:
- `runtime/go/lazuli/auth_guard.go`
- `runtime/go/lazuli/owned_by_actor.go`
- `runtime/go/lazuli/transition.go`
- `runtime/go/lazuli/partial_update.go`
- `runtime/go/lazuli/typed_decode.go`

Test counts:
- `AuthGuard`: 3 tests
- `OwnedByActor`: 5 tests
- `Transition`: 4 tests
- `PartialUpdate`: 4 tests
- `TypedDecode`: 4 tests

Test result:
- `cd runtime/go && go test ./lazuli/...` green
- Runtime package line: `ok   lazuli.dev/runtime/lazuli  (cached)`

Representative pilot handler diff:

```diff
- if ctx.User == nil {
-     return errors.New("not_authenticated")
- }
+ if err := lazuli.AuthGuard(ctx); err != nil {
+     return err
+ }
-
- raw, _ := json.Marshal(input)
- if err := json.Unmarshal(raw, &args); err != nil {
-     return lazuli.ValidationError(err)
- }
+ if err := lazuli.TypedDecode(input, &args); err != nil {
+     return err
+ }
```
