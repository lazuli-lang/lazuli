package lazuli

import (
	"context"
	"sync"
)

// DBRole names the database role requested for an operation.
type DBRole string

const (
	// DBRolePrimary routes to the write-capable primary database.
	DBRolePrimary DBRole = "primary"
	// DBRoleReplica routes to a read replica when one is available and healthy.
	DBRoleReplica DBRole = "replica"
)

func (role DBRole) String() string {
	return string(role)
}

type dbPrimaryPinContextKey struct{}

// PinDBPrimary returns a context that forces later replica reads to use the
// primary database. Call it after a write when read-your-writes consistency is
// needed for the rest of the request.
func PinDBPrimary(ctx context.Context) context.Context {
	if ctx == nil {
		ctx = context.Background()
	}
	return context.WithValue(ctx, dbPrimaryPinContextKey{}, true)
}

// DBPrimaryPinned reports whether ctx forces replica reads to use primary.
func DBPrimaryPinned(ctx context.Context) bool {
	if ctx == nil {
		return false
	}
	if lazuliCtx, ok := ctx.(*Ctx); ok {
		if lazuliCtx == nil || lazuliCtx.Context == nil {
			return false
		}
		ctx = lazuliCtx.Context
	}
	pinned, _ := ctx.Value(dbPrimaryPinContextKey{}).(bool)
	return pinned
}

// DBHealthStatus is the minimal health contract used for replica filtering.
type DBHealthStatus interface {
	Healthy(context.Context) bool
}

// DBHealthStatusFunc adapts a function into DBHealthStatus.
type DBHealthStatusFunc func(context.Context) bool

// Healthy returns f(ctx). A nil function is treated as healthy so typed nil
// health functions behave the same as an omitted health checker.
func (f DBHealthStatusFunc) Healthy(ctx context.Context) bool {
	if f == nil {
		return true
	}
	if ctx == nil {
		ctx = context.Background()
	}
	return f(ctx)
}

// DBRouteTarget describes one database handle available to the router.
//
// Handle is intentionally adapter-neutral: callers may store *pgxpool.Pool,
// *sql.DB, generated query wrappers, or test fakes.
type DBRouteTarget struct {
	Name   string
	Role   DBRole
	Handle any
	Health DBHealthStatus
}

// IsHealthy reports whether target is healthy. Targets without a health
// checker are considered healthy.
func (target DBRouteTarget) IsHealthy(ctx context.Context) bool {
	if target.Health == nil {
		return true
	}
	if ctx == nil {
		ctx = context.Background()
	}
	return target.Health.Healthy(ctx)
}

// DBRouter is the minimal database routing contract used by generated code.
type DBRouter interface {
	RouteDB(context.Context, DBRole) DBRouteTarget
}

// DBReplicaRouter routes writes to primary and reads to healthy replicas using
// round-robin selection. If no healthy replica is available, reads fall back to
// primary.
type DBReplicaRouter struct {
	mu          sync.Mutex
	primary     DBRouteTarget
	replicas    []DBRouteTarget
	nextReplica uint64
}

// NewDBReplicaRouter returns a router for primary and replicas.
func NewDBReplicaRouter(primary DBRouteTarget, replicas ...DBRouteTarget) *DBReplicaRouter {
	router := &DBReplicaRouter{
		primary: normalizeDBRouteTarget(primary, DBRolePrimary),
	}
	if len(replicas) > 0 {
		router.replicas = make([]DBRouteTarget, len(replicas))
		for i, replica := range replicas {
			router.replicas[i] = normalizeDBRouteTarget(replica, DBRoleReplica)
		}
	}
	return router
}

// RouteDB returns the target for role. Replica requests use a healthy replica
// unless ctx is pinned to primary or no healthy replica exists.
func (router *DBReplicaRouter) RouteDB(ctx context.Context, role DBRole) DBRouteTarget {
	if router == nil {
		return DBRouteTarget{Role: DBRolePrimary}
	}
	if role == DBRoleReplica && !DBPrimaryPinned(ctx) {
		if replica, ok := router.NextReplica(ctx); ok {
			return replica
		}
	}
	return router.Primary()
}

// Primary returns the configured primary target.
func (router *DBReplicaRouter) Primary() DBRouteTarget {
	if router == nil {
		return DBRouteTarget{Role: DBRolePrimary}
	}
	return normalizeDBRouteTarget(router.primary, DBRolePrimary)
}

// Replicas returns a copy of the configured replica targets.
func (router *DBReplicaRouter) Replicas() []DBRouteTarget {
	if router == nil || len(router.replicas) == 0 {
		return nil
	}
	replicas := make([]DBRouteTarget, len(router.replicas))
	for i, replica := range router.replicas {
		replicas[i] = normalizeDBRouteTarget(replica, DBRoleReplica)
	}
	return replicas
}

// HealthyReplicas returns the configured replicas whose health checker passes.
func (router *DBReplicaRouter) HealthyReplicas(ctx context.Context) []DBRouteTarget {
	if router == nil {
		return nil
	}

	replicas := router.Replicas()
	healthy := make([]DBRouteTarget, 0, len(replicas))
	for _, replica := range replicas {
		if replica.IsHealthy(ctx) {
			healthy = append(healthy, replica)
		}
	}
	return healthy
}

// NextReplica returns the next healthy replica in round-robin order.
func (router *DBReplicaRouter) NextReplica(ctx context.Context) (DBRouteTarget, bool) {
	replicas := router.HealthyReplicas(ctx)
	if len(replicas) == 0 {
		return DBRouteTarget{}, false
	}

	router.mu.Lock()
	index := int(router.nextReplica % uint64(len(replicas)))
	router.nextReplica++
	router.mu.Unlock()

	return replicas[index], true
}

func normalizeDBRouteTarget(target DBRouteTarget, role DBRole) DBRouteTarget {
	target.Role = role
	return target
}
