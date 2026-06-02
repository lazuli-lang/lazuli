package lazuli

import (
	"reflect"
	"strings"
	"testing"
)

// userRow mirrors a generated resource row shape: an actor-system-gated
// secret column (`password_hash`) alongside ordinary columns. The `db`
// tags drive the projection's column enumeration exactly as the runtime
// reflects over the real `Query[A, R]` row type.
type userRow struct {
	ID           int64  `db:"id"`
	OrgID        int64  `db:"org_id"`
	Email        string `db:"email"`
	PasswordHash string `db:"password_hash" json:"-"`
	// A field with no db tag must be ignored by the column enumerator.
	Internal string
	// A db:"-" field must be skipped.
	Skipped string `db:"-"`
}

func systemReadPolicies() map[string]Policy {
	return map[string]Policy{
		"password_hash": {
			Name:  "@actor.system",
			Atoms: []PolicyAtom{{Namespace: "actor", Name: "system"}},
		},
	}
}

// TestReadProjectionGatesActorSystemColumnForNormalActor is the W1-2
// SEC-FIELDPOLICY-READ-NULL guard: a column with `read: @actor.system`
// MUST NOT be selected by value for a non-system actor — it is projected
// as `NULL AS "password_hash"` so the restricted value never leaves the
// database. A system actor DOES get the real column.
func TestReadProjectionGatesActorSystemColumnForNormalActor(t *testing.T) {
	res := &resourceErased{
		Name:              "User",
		FieldReadPolicies: systemReadPolicies(),
	}
	rowType := reflect.TypeOf(userRow{})

	userCtx := &Ctx{Actor: ActorUser, User: &User{ID: 1, Roles: []string{"admin"}}}
	projection := readProjection(userCtx, res, rowType)

	// Never SELECT *.
	if strings.Contains(projection, "*") {
		t.Fatalf("non-system projection must not be SELECT *, got: %q", projection)
	}
	// password_hash is a value (non-pointer) string field, so it is masked
	// with an empty-string literal (a SQL NULL cannot scan into a value
	// string). It must never be selected by its bare column.
	if !strings.Contains(projection, `'' AS "password_hash"`) {
		t.Fatalf("password_hash must be masked as '' for a normal actor, got: %q", projection)
	}
	// The bare value column must never appear unprefixed by the mask alias.
	// (The substring `"password_hash"` legitimately appears inside
	// `'' AS "password_hash"`; strip that occurrence before checking.)
	withoutMask := strings.ReplaceAll(projection, `'' AS "password_hash"`, "")
	if strings.Contains(withoutMask, `"password_hash"`) {
		t.Fatalf("password_hash value column must NOT appear in non-system projection: %q", projection)
	}
	// Ordinary columns are still projected by value.
	for _, col := range []string{`"id"`, `"org_id"`, `"email"`} {
		if !strings.Contains(projection, col) {
			t.Fatalf("expected ordinary column %s in projection, got: %q", col, projection)
		}
	}
	// Untagged / db:"-" fields are not enumerated.
	if strings.Contains(projection, "Internal") || strings.Contains(projection, "Skipped") {
		t.Fatalf("untagged/skipped fields must not appear: %q", projection)
	}
}

// TestReadProjectionAllowsActorSystemColumnForSystemActor proves the
// other side of the gate: the system actor (the only reader the policy
// admits) DOES get the real `password_hash` column, not a NULL.
func TestReadProjectionAllowsActorSystemColumnForSystemActor(t *testing.T) {
	res := &resourceErased{
		Name:              "User",
		FieldReadPolicies: systemReadPolicies(),
	}
	rowType := reflect.TypeOf(userRow{})

	sysCtx := &Ctx{Actor: ActorSystem}
	projection := readProjection(sysCtx, res, rowType)

	if strings.Contains(projection, "NULL AS") {
		t.Fatalf("system actor must read the real column, no NULL projection: %q", projection)
	}
	if !strings.Contains(projection, `"password_hash"`) {
		t.Fatalf("system actor projection must include the real password_hash column: %q", projection)
	}
}

// TestReadProjectionFallsBackToStarForNonStructRow confirms scalar /
// dynamic shapes (no enumerable db columns) keep the prior `SELECT *`
// behaviour — field policies have no column surface to gate there.
func TestReadProjectionFallsBackToStarForNonStructRow(t *testing.T) {
	res := &resourceErased{Name: "User"}
	if got := readProjection(&Ctx{}, res, reflect.TypeOf("")); got != "*" {
		t.Fatalf("scalar row type should fall back to *, got %q", got)
	}
	if got := readProjection(&Ctx{}, res, reflect.TypeOf(int64(0))); got != "*" {
		t.Fatalf("scalar row type should fall back to *, got %q", got)
	}
}

// TestReadProjectionNoPoliciesProjectsAllColumns confirms a resource with
// no field read policies projects every declared column by value (never
// SELECT *, never NULL) — the common case.
func TestReadProjectionNoPoliciesProjectsAllColumns(t *testing.T) {
	res := &resourceErased{Name: "User"}
	projection := readProjection(&Ctx{Actor: ActorUser}, res, reflect.TypeOf(userRow{}))
	if strings.Contains(projection, "*") || strings.Contains(projection, "NULL AS") {
		t.Fatalf("unpoliced resource should project explicit columns by value, got: %q", projection)
	}
	for _, col := range []string{`"id"`, `"email"`, `"password_hash"`} {
		if !strings.Contains(projection, col) {
			t.Fatalf("expected column %s in projection, got: %q", col, projection)
		}
	}
}

// TestDbColumnsOf covers tag-option stripping, db:"-" skipping, pointer
// unwrapping, and the time.Time scalar carve-out.
func TestDbColumnsOf(t *testing.T) {
	type geoRow struct {
		ID  int64  `db:"id"`
		Loc string `db:"loc,type:geography(point,4326)"`
		Hid string `db:"-"`
		Pln string
	}
	cols := dbColumnsOf(reflect.TypeOf(&geoRow{}))
	wantNames := []string{"id", "loc"}
	gotNames := make([]string, len(cols))
	for i, c := range cols {
		gotNames[i] = c.name
	}
	if !reflect.DeepEqual(gotNames, wantNames) {
		t.Fatalf("dbColumnsOf names: want %v, got %v", wantNames, gotNames)
	}
}

// TestMaskedProjectionByFieldKind asserts the masking literal is chosen so
// pgx can scan it: pointer fields mask to NULL; value scalars mask to a
// typed zero literal (never the real column).
func TestMaskedProjectionByFieldKind(t *testing.T) {
	type maskRow struct {
		Secret    string  `db:"secret"`     // value string -> ''
		OptSecret *string `db:"opt_secret"` // pointer -> NULL
		Count     int64   `db:"count"`      // value int -> 0
		Flag      bool    `db:"flag"`       // value bool -> FALSE
	}
	res := &resourceErased{
		Name: "M",
		FieldReadPolicies: map[string]Policy{
			"secret":     {Name: "@actor.system", Atoms: []PolicyAtom{{Namespace: "actor", Name: "system"}}},
			"opt_secret": {Name: "@actor.system", Atoms: []PolicyAtom{{Namespace: "actor", Name: "system"}}},
			"count":      {Name: "@actor.system", Atoms: []PolicyAtom{{Namespace: "actor", Name: "system"}}},
			"flag":       {Name: "@actor.system", Atoms: []PolicyAtom{{Namespace: "actor", Name: "system"}}},
		},
	}
	proj := readProjection(&Ctx{Actor: ActorUser}, res, reflect.TypeOf(maskRow{}))
	for _, want := range []string{
		`'' AS "secret"`,
		`NULL AS "opt_secret"`,
		`0 AS "count"`,
		`FALSE AS "flag"`,
	} {
		if !strings.Contains(proj, want) {
			t.Fatalf("expected mask %q in projection, got: %q", want, proj)
		}
	}
}
