package lazuli

import (
	"errors"
	"testing"
)

func TestDBShardStrategyString(t *testing.T) {
	if got := DBShardStrategyHash.String(); got != "hash" {
		t.Fatalf("DBShardStrategyHash.String() = %q, want hash", got)
	}
	if got := DBShardStrategyRange.String(); got != "range" {
		t.Fatalf("DBShardStrategyRange.String() = %q, want range", got)
	}
}

func TestValidateDBShardKey(t *testing.T) {
	keys := []DBShardKey{
		{Name: "tenant", Value: "tenant-42"},
		{Name: "_resourceID", Value: " 00042 "},
		{Value: "global"},
	}

	for _, key := range keys {
		if err := ValidateDBShardKey(key); err != nil {
			t.Fatalf("ValidateDBShardKey(%#v) returned %v", key, err)
		}
	}
}

func TestValidateDBShardKeyRejectsInvalidKeys(t *testing.T) {
	tests := []struct {
		name string
		key  DBShardKey
	}{
		{name: "empty value", key: DBShardKey{Name: "tenant"}},
		{name: "blank value", key: DBShardKey{Name: "tenant", Value: " \t "}},
		{name: "invalid name", key: DBShardKey{Name: "tenant-id", Value: "42"}},
		{name: "control value", key: DBShardKey{Name: "tenant", Value: "tenant\n42"}},
		{name: "invalid utf8", key: DBShardKey{Name: "tenant", Value: string([]byte{0xff})}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateDBShardKey(tt.key)
			if !errors.Is(err, ErrInvalidDBShardKey) {
				t.Fatalf("ValidateDBShardKey(%#v) error = %v, want ErrInvalidDBShardKey", tt.key, err)
			}
		})
	}
}

func TestHashDBShardRouterRoutesDeterministically(t *testing.T) {
	shards := testDBShards()
	router, err := NewDBHashShardRouter(shards...)
	if err != nil {
		t.Fatalf("NewDBHashShardRouter returned error: %v", err)
	}

	key := DBShardKey{Name: "tenant", Value: "tenant-42"}
	hash, err := HashDBShardKey(key)
	if err != nil {
		t.Fatalf("HashDBShardKey returned error: %v", err)
	}
	wantShard := shards[int(hash%uint64(len(shards)))]

	first, err := router.RouteDBShard(t.Context(), DBShardRouteRequest{Key: key})
	if err != nil {
		t.Fatalf("RouteDBShard returned error: %v", err)
	}
	second, err := router.RouteDBShard(t.Context(), DBShardRouteRequest{Key: key})
	if err != nil {
		t.Fatalf("RouteDBShard second call returned error: %v", err)
	}

	if first.Shard != wantShard.Name || first.ShardIndex != int(hash%uint64(len(shards))) || first.Hash != hash {
		t.Fatalf("RouteDBShard hash route = %#v, want shard %q index %d hash %d", first, wantShard.Name, int(hash%uint64(len(shards))), hash)
	}
	if second.Shard != first.Shard || second.Hash != first.Hash {
		t.Fatalf("RouteDBShard is not deterministic: first=%#v second=%#v", first, second)
	}
	if first.Route != wantShard.Route {
		t.Fatalf("RouteDBShard Route = %#v, want %#v", first.Route, wantShard.Route)
	}
}

func TestRangeDBShardRouterRoutesByLexicographicRange(t *testing.T) {
	router, err := NewDBRangeShardRouter(
		DBShard{
			Name:  "shard-a",
			Route: DBTenantRoute{Database: "tenant_db_a"},
			Range: DBShardRange{Start: "0000", End: "5000"},
		},
		DBShard{
			Name:  "shard-b",
			Route: DBTenantRoute{Database: "tenant_db_b"},
			Range: DBShardRange{Start: "5000"},
		},
	)
	if err != nil {
		t.Fatalf("NewDBRangeShardRouter returned error: %v", err)
	}

	first, err := router.RouteDBShard(t.Context(), DBShardRouteRequest{Key: DBShardKey{Name: "tenant", Value: "0042"}})
	if err != nil {
		t.Fatalf("RouteDBShard first range returned error: %v", err)
	}
	if first.Shard != "shard-a" || first.Hash != 0 || first.Range != (DBShardRange{Start: "0000", End: "5000"}) {
		t.Fatalf("RouteDBShard first range = %#v, want shard-a range metadata", first)
	}

	second, err := router.RouteDBShard(t.Context(), DBShardRouteRequest{Key: DBShardKey{Name: "tenant", Value: "9000"}})
	if err != nil {
		t.Fatalf("RouteDBShard second range returned error: %v", err)
	}
	if second.Shard != "shard-b" || second.Route.Database != "tenant_db_b" {
		t.Fatalf("RouteDBShard second range = %#v, want shard-b", second)
	}
}

func TestRangeDBShardRouterReturnsNotFoundForGaps(t *testing.T) {
	router, err := NewDBRangeShardRouter(
		DBShard{
			Name:  "shard-a",
			Route: DBTenantRoute{Schema: "tenant_a"},
			Range: DBShardRange{Start: "0000", End: "1000"},
		},
		DBShard{
			Name:  "shard-b",
			Route: DBTenantRoute{Schema: "tenant_b"},
			Range: DBShardRange{Start: "9000"},
		},
	)
	if err != nil {
		t.Fatalf("NewDBRangeShardRouter returned error: %v", err)
	}

	route, err := router.RouteDBShard(t.Context(), DBShardRouteRequest{Key: DBShardKey{Name: "tenant", Value: "5000"}})
	if !errors.Is(err, ErrDBShardRouteNotFound) {
		t.Fatalf("RouteDBShard gap route = %#v error = %v, want ErrDBShardRouteNotFound", route, err)
	}
}

func TestDBShardRouterRejectsInvalidConfiguration(t *testing.T) {
	tests := []struct {
		name string
		run  func() error
	}{
		{
			name: "no hash shards",
			run: func() error {
				_, err := NewDBHashShardRouter()
				return err
			},
		},
		{
			name: "empty shard name",
			run: func() error {
				_, err := NewDBHashShardRouter(DBShard{Route: DBTenantRoute{Schema: "tenant_a"}})
				return err
			},
		},
		{
			name: "invalid tenant route",
			run: func() error {
				_, err := NewDBHashShardRouter(DBShard{Name: "shard-a", Route: DBTenantRoute{Schema: "tenant-a"}})
				return err
			},
		},
		{
			name: "invalid range order",
			run: func() error {
				_, err := NewDBRangeShardRouter(DBShard{
					Name:  "shard-a",
					Route: DBTenantRoute{Schema: "tenant_a"},
					Range: DBShardRange{Start: "9000", End: "1000"},
				})
				return err
			},
		},
		{
			name: "invalid range bound",
			run: func() error {
				_, err := NewDBRangeShardRouter(DBShard{
					Name:  "shard-a",
					Route: DBTenantRoute{Schema: "tenant_a"},
					Range: DBShardRange{Start: "tenant\n0", End: "tenant_9"},
				})
				return err
			},
		},
		{
			name: "overlapping ranges",
			run: func() error {
				_, err := NewDBRangeShardRouter(
					DBShard{
						Name:  "shard-a",
						Route: DBTenantRoute{Schema: "tenant_a"},
						Range: DBShardRange{Start: "0000", End: "7000"},
					},
					DBShard{
						Name:  "shard-b",
						Route: DBTenantRoute{Schema: "tenant_b"},
						Range: DBShardRange{Start: "5000", End: "9000"},
					},
				)
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.run(); !errors.Is(err, ErrInvalidDBShardRoute) {
				t.Fatalf("%s error = %v, want ErrInvalidDBShardRoute", tt.name, err)
			}
		})
	}
}

func TestDBShardRouterAddsTenantMetadata(t *testing.T) {
	router, err := NewDBHashShardRouter(testDBShards()...)
	if err != nil {
		t.Fatalf("NewDBHashShardRouter returned error: %v", err)
	}

	tenant := Tenant{OrgID: 42}
	request := NewDBTenantShardRouteRequest(tenant)
	route, err := router.RouteDBShard(t.Context(), request)
	if err != nil {
		t.Fatalf("RouteDBShard returned error: %v", err)
	}

	if route.Tenant == nil || route.Tenant.OrgID != tenant.OrgID {
		t.Fatalf("RouteDBShard tenant metadata = %#v, want org 42", route.Tenant)
	}
	if route.Tenant == request.Tenant {
		t.Fatal("RouteDBShard reused request tenant pointer, want copy")
	}
	if route.TenantKey != "42" {
		t.Fatalf("RouteDBShard TenantKey = %q, want 42", route.TenantKey)
	}
	if route.Key != (DBShardKey{Name: "tenant", Value: "42"}) {
		t.Fatalf("RouteDBShard Key = %#v, want tenant key", route.Key)
	}
}

func TestDBShardRouterUsesCtxTenantWhenRequestKeyIsEmpty(t *testing.T) {
	router, err := NewDBHashShardRouter(testDBShards()...)
	if err != nil {
		t.Fatalf("NewDBHashShardRouter returned error: %v", err)
	}

	ctx := &Ctx{
		Context: t.Context(),
		Tenant:  &Tenant{OrgID: 7},
	}
	route, err := router.RouteDBShard(ctx, DBShardRouteRequest{})
	if err != nil {
		t.Fatalf("RouteDBShard returned error: %v", err)
	}

	if route.Tenant == nil || route.Tenant.OrgID != 7 {
		t.Fatalf("RouteDBShard tenant metadata = %#v, want org 7", route.Tenant)
	}
	if route.Key != (DBShardKey{Name: "tenant", Value: "7"}) {
		t.Fatalf("RouteDBShard Key = %#v, want tenant key from context", route.Key)
	}
}

func TestDBShardRouterReturnsShardCopies(t *testing.T) {
	router, err := NewDBHashShardRouter(testDBShards()...)
	if err != nil {
		t.Fatalf("NewDBHashShardRouter returned error: %v", err)
	}

	shards := router.Shards()
	shards[0].Name = "changed"
	shards[0].Route.Schema = "changed"

	got := router.Shards()
	if got[0].Name != "shard-a" || got[0].Route.Schema != "tenant_a" {
		t.Fatalf("Shards()[0] = %#v, want original shard copy", got[0])
	}
}

func TestDBShardRouteContextHelpers(t *testing.T) {
	router, err := NewDBHashShardRouter(testDBShards()...)
	if err != nil {
		t.Fatalf("NewDBHashShardRouter returned error: %v", err)
	}
	route, err := router.RouteDBShard(t.Context(), NewDBTenantShardRouteRequest(Tenant{OrgID: 11}))
	if err != nil {
		t.Fatalf("RouteDBShard returned error: %v", err)
	}

	ctx := WithDBShardRoute(nil, route)
	got, ok := DBShardRouteFromContext(ctx)
	if !ok {
		t.Fatal("DBShardRouteFromContext(ctx) ok = false, want true")
	}
	if got.Shard != route.Shard || got.TenantKey != "11" || got.Tenant == nil || got.Tenant.OrgID != 11 {
		t.Fatalf("DBShardRouteFromContext(ctx) = %#v, want stored route", got)
	}

	got.Tenant.OrgID = 99
	again, ok := DBShardRouteFromContext(&Ctx{Context: ctx})
	if !ok {
		t.Fatal("DBShardRouteFromContext(*Ctx) ok = false, want true")
	}
	if again.Tenant.OrgID != 11 {
		t.Fatalf("DBShardRouteFromContext returned mutable tenant pointer: %#v", again.Tenant)
	}

	if empty, ok := DBShardRouteFromContext(nil); ok || empty != (DBShardRoute{}) {
		t.Fatalf("DBShardRouteFromContext(nil) = %#v, %v; want zero, false", empty, ok)
	}
}

func testDBShards() []DBShard {
	return []DBShard{
		{
			Name:  "shard-a",
			Route: DBTenantRoute{Database: "tenant_db_a", Schema: "tenant_a"},
		},
		{
			Name:  "shard-b",
			Route: DBTenantRoute{Database: "tenant_db_b", Schema: "tenant_b"},
		},
		{
			Name:  "shard-c",
			Route: DBTenantRoute{Database: "tenant_db_c", Schema: "tenant_c"},
		},
	}
}
