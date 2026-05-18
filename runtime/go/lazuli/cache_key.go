package lazuli

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"strconv"
)

func cacheKeyFor(ctx *Ctx, queryName string, args any) (string, error) {
	tenant := "-"
	if ctx != nil && ctx.Tenant != nil {
		tenant = strconv.FormatInt(int64(ctx.Tenant.OrgID), 10)
	}
	argsHash, err := hashArgs(args)
	if err != nil {
		return "", err
	}
	return queryName + "|" + tenant + "|" + argsHash, nil
}

func hashArgs(args any) (string, error) {
	buf, err := json.Marshal(args)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(buf)
	return hex.EncodeToString(sum[:]), nil
}
