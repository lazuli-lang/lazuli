package lazuli

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"strconv"
	"strings"
)

func cacheKeyFor(ctx *Ctx, queryName string, args any) (string, error) {
	argsHash, err := hashArgs(args)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString(queryName)
	b.WriteString("|")
	if ctx != nil && ctx.Tenant != nil {
		b.WriteString("t")
		b.WriteString(strconv.FormatInt(int64(ctx.Tenant.OrgID), 10))
	} else {
		b.WriteString("t-")
	}
	b.WriteString("|")
	// SECURITY (M-5): include actor + user in cache key so per-actor
	// views don't pollute across anonymous requests.
	actor := ActorAnonymous
	if ctx != nil && ctx.Actor != "" {
		actor = ctx.Actor
	}
	b.WriteString(string(actor))
	b.WriteString("|")
	if ctx != nil && ctx.User != nil {
		b.WriteString("u")
		b.WriteString(strconv.FormatInt(int64(ctx.User.ID), 10))
	} else {
		b.WriteString("u-")
	}
	b.WriteString("|")
	b.WriteString(argsHash)
	return b.String(), nil
}

func hashArgs(args any) (string, error) {
	buf, err := json.Marshal(args)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(buf)
	return hex.EncodeToString(sum[:]), nil
}
