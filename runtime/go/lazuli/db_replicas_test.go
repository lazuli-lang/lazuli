package lazuli

import (
	"context"
	"reflect"
	"testing"
)

func TestDBRoleString(t *testing.T) {
	if got := DBRolePrimary.String(); got != "primary" {
		t.Fatalf("DBRolePrimary.String() = %q, want primary", got)
	}
	if got := DBRoleReplica.String(); got != "replica" {
		t.Fatalf("DBRoleReplica.String() = %q, want replica", got)
	}
}

func TestDBReplicaRouterRoutesPrimaryAndReplicasRoundRobin(t *testing.T) {
	router := NewDBReplicaRouter(
		DBRouteTarget{Name: "primary", Handle: "primary"},
		DBRouteTarget{Name: "replica-a", Handle: "a"},
		DBRouteTarget{Name: "replica-b", Handle: "b"},
	)

	var _ DBRouter = router

	if got := router.RouteDB(context.Background(), DBRolePrimary); got.Name != "primary" || got.Role != DBRolePrimary || got.Handle != "primary" {
		t.Fatalf("RouteDB(primary) = %#v, want primary target", got)
	}

	names := []string{
		router.RouteDB(context.Background(), DBRoleReplica).Name,
		router.RouteDB(context.Background(), DBRoleReplica).Name,
		router.RouteDB(context.Background(), DBRoleReplica).Name,
		router.RouteDB(context.Background(), DBRoleReplica).Name,
	}
	assertDBRouteNames(t, names, []string{"replica-a", "replica-b", "replica-a", "replica-b"})
}

func TestDBReplicaRouterFiltersUnhealthyReplicas(t *testing.T) {
	healthy := true
	router := NewDBReplicaRouter(
		DBRouteTarget{Name: "primary"},
		DBRouteTarget{
			Name:   "replica-a",
			Health: DBHealthStatusFunc(func(context.Context) bool { return false }),
		},
		DBRouteTarget{
			Name:   "replica-b",
			Health: DBHealthStatusFunc(func(context.Context) bool { return healthy }),
		},
		DBRouteTarget{Name: "replica-c"},
	)

	gotHealthy := router.HealthyReplicas(context.Background())
	assertDBRouteNames(t, dbRouteNames(gotHealthy), []string{"replica-b", "replica-c"})

	names := []string{
		router.RouteDB(context.Background(), DBRoleReplica).Name,
		router.RouteDB(context.Background(), DBRoleReplica).Name,
		router.RouteDB(context.Background(), DBRoleReplica).Name,
	}
	assertDBRouteNames(t, names, []string{"replica-b", "replica-c", "replica-b"})

	healthy = false
	got := router.RouteDB(context.Background(), DBRoleReplica)
	if got.Name != "replica-c" {
		t.Fatalf("RouteDB(replica) with one healthy replica = %#v, want replica-c", got)
	}
}

func TestDBReplicaRouterFallsBackToPrimaryWhenNoHealthyReplica(t *testing.T) {
	router := NewDBReplicaRouter(
		DBRouteTarget{Name: "primary", Handle: "primary"},
		DBRouteTarget{
			Name:   "replica-a",
			Handle: "a",
			Health: DBHealthStatusFunc(func(context.Context) bool { return false }),
		},
	)

	if replica, ok := router.NextReplica(context.Background()); ok || replica != (DBRouteTarget{}) {
		t.Fatalf("NextReplica() = %#v, %v; want zero target, false", replica, ok)
	}
	got := router.RouteDB(context.Background(), DBRoleReplica)
	if got.Name != "primary" || got.Role != DBRolePrimary || got.Handle != "primary" {
		t.Fatalf("RouteDB(replica) = %#v, want primary fallback", got)
	}
}

func TestPinDBPrimaryForcesReplicaReadsToPrimary(t *testing.T) {
	router := NewDBReplicaRouter(
		DBRouteTarget{Name: "primary"},
		DBRouteTarget{Name: "replica-a"},
	)
	ctx := PinDBPrimary(context.Background())

	if !DBPrimaryPinned(ctx) {
		t.Fatal("DBPrimaryPinned(ctx) = false, want true")
	}
	got := router.RouteDB(ctx, DBRoleReplica)
	if got.Name != "primary" || got.Role != DBRolePrimary {
		t.Fatalf("RouteDB(replica) with primary pin = %#v, want primary", got)
	}
}

func TestDBReplicaContextAndHealthHelpersHandleNilInputs(t *testing.T) {
	if DBPrimaryPinned(nil) {
		t.Fatal("DBPrimaryPinned(nil) = true, want false")
	}
	if DBPrimaryPinned((*Ctx)(nil)) {
		t.Fatal("DBPrimaryPinned((*Ctx)(nil)) = true, want false")
	}
	if DBPrimaryPinned(&Ctx{}) {
		t.Fatal("DBPrimaryPinned(&Ctx{}) = true, want false")
	}
	if !DBPrimaryPinned(PinDBPrimary(nil)) {
		t.Fatal("DBPrimaryPinned(PinDBPrimary(nil)) = false, want true")
	}
	if !DBPrimaryPinned(&Ctx{Context: PinDBPrimary(context.Background())}) {
		t.Fatal("DBPrimaryPinned(&Ctx{Context: pinned}) = false, want true")
	}

	target := DBRouteTarget{Name: "replica", Health: nil}
	if !target.IsHealthy(nil) {
		t.Fatal("DBRouteTarget.IsHealthy(nil) = false, want true for nil health checker")
	}

	var fn DBHealthStatusFunc
	target.Health = fn
	if !target.IsHealthy(nil) {
		t.Fatal("DBRouteTarget.IsHealthy(nil) = false, want true for typed nil health checker")
	}
}

func TestDBReplicaRouterReturnsReplicaCopies(t *testing.T) {
	router := NewDBReplicaRouter(
		DBRouteTarget{Name: "primary"},
		DBRouteTarget{Name: "replica-a"},
	)

	replicas := router.Replicas()
	replicas[0].Name = "changed"

	got := router.Replicas()
	if got[0].Name != "replica-a" {
		t.Fatalf("Replicas()[0].Name = %q, want replica-a", got[0].Name)
	}
	if got[0].Role != DBRoleReplica {
		t.Fatalf("Replicas()[0].Role = %q, want replica", got[0].Role)
	}
}

func dbRouteNames(targets []DBRouteTarget) []string {
	names := make([]string, len(targets))
	for i, target := range targets {
		names[i] = target.Name
	}
	return names
}

func assertDBRouteNames(t *testing.T, got, want []string) {
	t.Helper()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("route names = %#v, want %#v", got, want)
	}
}
