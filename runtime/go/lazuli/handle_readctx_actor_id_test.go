package lazuli

import "testing"

// BUG #17 — `ctx.actor.id` lowers to FromCtx("actor.id"). readCtx must
// resolve it as an alias of `actor.user_id` (the acting user's row id),
// returning ctx.User.ID. Before the fix this hit the default arm and
// 500'd with "unknown ctx path: actor.id", breaking every
// `where id = ctx.actor.id` update at the WHERE-source resolution.
func TestReadCtxActorIDResolvesToUserID(t *testing.T) {
	ctx := &Ctx{User: &User{ID: 42, OrgID: 7}}

	got, err := readCtx(ctx, "actor.id")
	if err != nil {
		t.Fatalf("readCtx(actor.id) returned error: %v", err)
	}
	if got != ID(42) {
		t.Fatalf("readCtx(actor.id) = %v, want 42 (ctx.User.ID)", got)
	}

	// It is a true alias of actor.user_id.
	uid, err := readCtx(ctx, "actor.user_id")
	if err != nil {
		t.Fatalf("readCtx(actor.user_id) returned error: %v", err)
	}
	if got != uid {
		t.Fatalf("actor.id (%v) must equal actor.user_id (%v)", got, uid)
	}
}

// With no authenticated user, actor.id 401s (mirrors actor.user_id), not
// 500s — the policy layer treats it as "no actor".
func TestReadCtxActorIDWithoutUserIs401(t *testing.T) {
	ctx := &Ctx{}
	_, err := readCtx(ctx, "actor.id")
	if err == nil {
		t.Fatal("readCtx(actor.id) with no user should error")
	}
	le, ok := err.(*Error)
	if !ok {
		t.Fatalf("expected *Error, got %T", err)
	}
	if le.Status != 401 {
		t.Fatalf("readCtx(actor.id) with no user: status = %d, want 401", le.Status)
	}
}
