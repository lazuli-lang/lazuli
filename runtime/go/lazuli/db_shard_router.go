package lazuli

import (
	"context"
	"errors"
	"fmt"
	"hash/fnv"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"
)

var (
	// ErrInvalidDBShardKey is returned when a shard key is empty or contains
	// unsafe metadata.
	ErrInvalidDBShardKey = errors.New("lazuli: invalid db shard key")

	// ErrInvalidDBShardRoute is returned when shard router configuration is
	// incomplete or ambiguous.
	ErrInvalidDBShardRoute = errors.New("lazuli: invalid db shard route")

	// ErrDBShardRouteNotFound is returned when no range shard covers the
	// requested key.
	ErrDBShardRouteNotFound = errors.New("lazuli: db shard route not found")
)

// DBShardStrategy names the deterministic strategy used to select a shard.
type DBShardStrategy string

const (
	// DBShardStrategyHash chooses a shard by hashing the normalized key value.
	DBShardStrategyHash DBShardStrategy = "hash"

	// DBShardStrategyRange chooses the shard whose configured range contains
	// the normalized key value. Ranges compare key values lexicographically.
	DBShardStrategyRange DBShardStrategy = "range"
)

func (strategy DBShardStrategy) String() string {
	return string(strategy)
}

// DBShardKey identifies the logical value used to select a database shard.
// Name is optional metadata and must be a generated identifier when present.
type DBShardKey struct {
	Name  string
	Value string
}

// Normalize returns key with surrounding whitespace removed.
func (key DBShardKey) Normalize() DBShardKey {
	key.Name = strings.TrimSpace(key.Name)
	key.Value = strings.TrimSpace(key.Value)
	return key
}

// ValidateDBShardKey validates a shard key after normalization.
func ValidateDBShardKey(key DBShardKey) error {
	key = key.Normalize()
	if key.Name != "" && !validDBTenantIdentifier(key.Name) {
		return fmt.Errorf("%w: name %q", ErrInvalidDBShardKey, key.Name)
	}
	if err := validateDBShardValue("value", key.Value); err != nil {
		return err
	}
	return nil
}

// HashDBShardKey returns the stable FNV-1a hash used by the hash router.
func HashDBShardKey(key DBShardKey) (uint64, error) {
	key = key.Normalize()
	if err := ValidateDBShardKey(key); err != nil {
		return 0, err
	}

	hash := fnv.New64a()
	_, _ = hash.Write([]byte(key.Value))
	return hash.Sum64(), nil
}

// DBTenantShardKey returns the default shard key for a tenant route.
func DBTenantShardKey(tenant Tenant) DBShardKey {
	return DBShardKey{
		Name:  "tenant",
		Value: strconv.FormatInt(int64(tenant.OrgID), 10),
	}
}

// NewDBTenantShardRouteRequest returns a tenant-aware shard route request
// using the tenant's OrgID as the shard key.
func NewDBTenantShardRouteRequest(tenant Tenant) DBShardRouteRequest {
	tenantCopy := tenant
	return DBShardRouteRequest{
		Key:    DBTenantShardKey(tenant),
		Tenant: &tenantCopy,
	}
}

// DBShardRange declares the lexicographic key interval owned by a range shard.
// Start is inclusive and End is exclusive. Empty bounds are unbounded.
type DBShardRange struct {
	Start string
	End   string
}

// Normalize returns r with surrounding whitespace removed from each bound.
func (r DBShardRange) Normalize() DBShardRange {
	r.Start = strings.TrimSpace(r.Start)
	r.End = strings.TrimSpace(r.End)
	return r
}

// ContainsDBShardKey reports whether r contains key after normalization. The
// caller should validate key before using this helper for routing.
func (r DBShardRange) ContainsDBShardKey(key DBShardKey) bool {
	r = r.Normalize()
	value := key.Normalize().Value
	if r.Start != "" && value < r.Start {
		return false
	}
	if r.End != "" && value >= r.End {
		return false
	}
	return true
}

// ValidateDBShardRange validates range bounds after normalization.
func ValidateDBShardRange(r DBShardRange) error {
	r = r.Normalize()
	if r.Start == "" && r.End == "" {
		return nil
	}
	if err := validateOptionalDBShardValue("range start", r.Start); err != nil {
		return fmt.Errorf("%w: %w", ErrInvalidDBShardRoute, err)
	}
	if err := validateOptionalDBShardValue("range end", r.End); err != nil {
		return fmt.Errorf("%w: %w", ErrInvalidDBShardRoute, err)
	}
	if r.Start != "" && r.End != "" && r.Start >= r.End {
		return fmt.Errorf("%w: range start %q must be before end %q", ErrInvalidDBShardRoute, r.Start, r.End)
	}
	return nil
}

// DBShard describes one physical database shard and its tenant database/schema
// route. Range is only used by DBShardStrategyRange.
type DBShard struct {
	Name  string
	Route DBTenantRoute
	Range DBShardRange
}

// Normalize returns shard with normalized metadata fields.
func (shard DBShard) Normalize() DBShard {
	shard.Name = strings.TrimSpace(shard.Name)
	shard.Range = shard.Range.Normalize()
	return shard
}

// DBShardRouteRequest contains the key and optional tenant context for a shard
// lookup. If Key is empty and Tenant is present, RouteDBShard uses the tenant's
// OrgID as the shard key.
type DBShardRouteRequest struct {
	Key    DBShardKey
	Tenant *Tenant
}

// DBShardRoute is adapter-neutral metadata describing the selected shard.
type DBShardRoute struct {
	Shard      string
	ShardIndex int
	Strategy   DBShardStrategy
	Key        DBShardKey
	Tenant     *Tenant
	TenantKey  string
	Route      DBTenantRoute
	Hash       uint64
	Range      DBShardRange
}

// DBShardRouter selects a database shard using a deterministic strategy.
type DBShardRouter struct {
	strategy DBShardStrategy
	shards   []DBShard
}

// NewDBHashShardRouter returns a router that chooses shards by stable key hash.
func NewDBHashShardRouter(shards ...DBShard) (*DBShardRouter, error) {
	return newDBShardRouter(DBShardStrategyHash, shards)
}

// NewDBRangeShardRouter returns a router that chooses shards by configured
// non-overlapping key ranges.
func NewDBRangeShardRouter(shards ...DBShard) (*DBShardRouter, error) {
	return newDBShardRouter(DBShardStrategyRange, shards)
}

// Strategy returns the router's shard strategy.
func (router *DBShardRouter) Strategy() DBShardStrategy {
	if router == nil {
		return ""
	}
	return router.strategy
}

// Shards returns a copy of the configured shards.
func (router *DBShardRouter) Shards() []DBShard {
	if router == nil || len(router.shards) == 0 {
		return nil
	}
	shards := make([]DBShard, len(router.shards))
	copy(shards, router.shards)
	return shards
}

// RouteDBShard returns metadata for the shard selected by request.
func (router *DBShardRouter) RouteDBShard(ctx context.Context, request DBShardRouteRequest) (DBShardRoute, error) {
	if router == nil || len(router.shards) == 0 {
		return DBShardRoute{}, fmt.Errorf("%w: no shards configured", ErrInvalidDBShardRoute)
	}

	tenant := dbShardRouteRequestTenant(ctx, request)
	if request.Key == (DBShardKey{}) && tenant != nil {
		request.Key = DBTenantShardKey(*tenant)
	}
	key := request.Key.Normalize()
	if err := ValidateDBShardKey(key); err != nil {
		return DBShardRoute{}, err
	}

	switch router.strategy {
	case DBShardStrategyHash:
		hash, err := HashDBShardKey(key)
		if err != nil {
			return DBShardRoute{}, err
		}
		index := int(hash % uint64(len(router.shards)))
		return router.routeForShard(index, key, tenant, hash), nil
	case DBShardStrategyRange:
		for i, shard := range router.shards {
			if shard.Range.ContainsDBShardKey(key) {
				return router.routeForShard(i, key, tenant, 0), nil
			}
		}
		return DBShardRoute{}, fmt.Errorf("%w: key %q", ErrDBShardRouteNotFound, key.Value)
	default:
		return DBShardRoute{}, fmt.Errorf("%w: unsupported strategy %q", ErrInvalidDBShardRoute, router.strategy)
	}
}

type dbShardRouteContextKey struct{}

// WithDBShardRoute returns a child context carrying the selected shard route.
func WithDBShardRoute(ctx context.Context, route DBShardRoute) context.Context {
	if ctx == nil {
		ctx = context.Background()
	}
	return context.WithValue(ctx, dbShardRouteContextKey{}, cloneDBShardRoute(route))
}

// DBShardRouteFromContext reads the active shard route metadata from ctx.
func DBShardRouteFromContext(ctx context.Context) (DBShardRoute, bool) {
	if ctx == nil {
		return DBShardRoute{}, false
	}
	if lazuliCtx, ok := ctx.(*Ctx); ok {
		if lazuliCtx == nil || lazuliCtx.Context == nil {
			return DBShardRoute{}, false
		}
		ctx = lazuliCtx.Context
	}
	route, ok := ctx.Value(dbShardRouteContextKey{}).(DBShardRoute)
	if !ok {
		return DBShardRoute{}, false
	}
	return cloneDBShardRoute(route), true
}

func newDBShardRouter(strategy DBShardStrategy, shards []DBShard) (*DBShardRouter, error) {
	if strategy != DBShardStrategyHash && strategy != DBShardStrategyRange {
		return nil, fmt.Errorf("%w: unsupported strategy %q", ErrInvalidDBShardRoute, strategy)
	}
	if len(shards) == 0 {
		return nil, fmt.Errorf("%w: at least one shard required", ErrInvalidDBShardRoute)
	}

	normalized := make([]DBShard, len(shards))
	for i, shard := range shards {
		shard = shard.Normalize()
		if err := validateDBShard(shard); err != nil {
			return nil, fmt.Errorf("shard %d: %w", i, err)
		}
		if strategy == DBShardStrategyRange {
			if err := ValidateDBShardRange(shard.Range); err != nil {
				return nil, fmt.Errorf("shard %d: %w", i, err)
			}
			for j := 0; j < i; j++ {
				if dbShardRangesOverlap(normalized[j].Range, shard.Range) {
					return nil, fmt.Errorf("%w: overlapping ranges for shards %q and %q", ErrInvalidDBShardRoute, normalized[j].Name, shard.Name)
				}
			}
		}
		normalized[i] = shard
	}

	return &DBShardRouter{
		strategy: strategy,
		shards:   normalized,
	}, nil
}

func validateDBShard(shard DBShard) error {
	if shard.Name == "" {
		return fmt.Errorf("%w: shard name required", ErrInvalidDBShardRoute)
	}
	if !utf8.ValidString(shard.Name) {
		return fmt.Errorf("%w: shard name must be valid utf-8", ErrInvalidDBShardRoute)
	}
	for _, r := range shard.Name {
		if unicode.IsControl(r) {
			return fmt.Errorf("%w: shard name contains control character", ErrInvalidDBShardRoute)
		}
	}
	if err := ValidateDBTenantRoute(shard.Route); err != nil {
		return fmt.Errorf("%w: %w", ErrInvalidDBShardRoute, err)
	}
	return nil
}

func validateOptionalDBShardValue(kind, value string) error {
	if value == "" {
		return nil
	}
	return validateDBShardValue(kind, value)
}

func validateDBShardValue(kind, value string) error {
	if value == "" {
		return fmt.Errorf("%w: %s required", ErrInvalidDBShardKey, kind)
	}
	if !utf8.ValidString(value) {
		return fmt.Errorf("%w: %s must be valid utf-8", ErrInvalidDBShardKey, kind)
	}
	for _, r := range value {
		if unicode.IsControl(r) {
			return fmt.Errorf("%w: %s contains control character", ErrInvalidDBShardKey, kind)
		}
	}
	return nil
}

func dbShardRangesOverlap(left, right DBShardRange) bool {
	left = left.Normalize()
	right = right.Normalize()
	return dbShardRangeStartsBeforeEnd(left.Start, right.End) &&
		dbShardRangeStartsBeforeEnd(right.Start, left.End)
}

func dbShardRangeStartsBeforeEnd(start, end string) bool {
	return end == "" || start == "" || start < end
}

func (router *DBShardRouter) routeForShard(index int, key DBShardKey, tenant *Tenant, hash uint64) DBShardRoute {
	shard := router.shards[index]
	return DBShardRoute{
		Shard:      shard.Name,
		ShardIndex: index,
		Strategy:   router.strategy,
		Key:        key,
		Tenant:     cloneTenant(tenant),
		TenantKey:  dbShardTenantKey(tenant),
		Route:      shard.Route,
		Hash:       hash,
		Range:      shard.Range,
	}
}

func dbShardRouteRequestTenant(ctx context.Context, request DBShardRouteRequest) *Tenant {
	if request.Tenant != nil {
		return cloneTenant(request.Tenant)
	}
	if lazuliCtx, ok := ctx.(*Ctx); ok {
		if lazuliCtx == nil || lazuliCtx.Tenant == nil {
			return nil
		}
		return cloneTenant(lazuliCtx.Tenant)
	}
	return nil
}

func dbShardTenantKey(tenant *Tenant) string {
	if tenant == nil {
		return ""
	}
	return strconv.FormatInt(int64(tenant.OrgID), 10)
}

func cloneDBShardRoute(route DBShardRoute) DBShardRoute {
	route.Key = route.Key.Normalize()
	route.Range = route.Range.Normalize()
	route.Tenant = cloneTenant(route.Tenant)
	return route
}

func cloneTenant(tenant *Tenant) *Tenant {
	if tenant == nil {
		return nil
	}
	tenantCopy := *tenant
	return &tenantCopy
}
