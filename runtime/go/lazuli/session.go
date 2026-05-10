package lazuli

import (
	"net/http"
	"strconv"
	"strings"
)

// populateDevSession fills the request Ctx from `X-Lazuli-*` headers. This
// is the **dev-mode auth** for the spike — real cookie/JWT/HMAC sessions
// arrive in a future cut and replace this function without changing the
// `Ctx` shape downstream.
//
// Recognised headers:
//
//	X-Lazuli-Actor    "user" | "system" | "anonymous" (default)
//	X-Lazuli-User-ID  numeric user id (sets Actor=user when present)
//	X-Lazuli-Org-ID   numeric org id (populates Tenant)
//	X-Lazuli-Email    user email
//	X-Lazuli-Roles    CSV of role names ("admin,sales")
//
// When `X-Lazuli-User-ID` is present the actor is forced to `user` and the
// `Ctx.User` struct is built. When only `X-Lazuli-Actor: system` is sent,
// the request runs as the system actor (no user). Without any header the
// request is anonymous.
func populateDevSession(r *http.Request, ctx *Ctx) {
	if actor := r.Header.Get("X-Lazuli-Actor"); actor == "system" {
		ctx.Actor = ActorSystem
		return
	}

	userIDStr := r.Header.Get("X-Lazuli-User-ID")
	if userIDStr == "" {
		return
	}
	userID, err := strconv.ParseInt(userIDStr, 10, 64)
	if err != nil {
		return
	}

	orgID, _ := strconv.ParseInt(r.Header.Get("X-Lazuli-Org-ID"), 10, 64)

	ctx.Actor = ActorUser
	ctx.User = &User{
		ID:    userID,
		OrgID: orgID,
		Email: r.Header.Get("X-Lazuli-Email"),
		Roles: parseRolesCSV(r.Header.Get("X-Lazuli-Roles")),
	}
	if orgID > 0 {
		ctx.Tenant = &Tenant{OrgID: orgID}
	}
}

// parseRolesCSV splits "admin, sales,  ops" into ["admin", "sales", "ops"],
// dropping empty entries and trimming whitespace.
func parseRolesCSV(s string) []string {
	if s == "" {
		return nil
	}
	parts := strings.Split(s, ",")
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		if t := strings.TrimSpace(p); t != "" {
			out = append(out, t)
		}
	}
	return out
}
