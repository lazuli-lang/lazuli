package lazuli

import (
	"context"
	"errors"
	"reflect"

	"lazuli.dev/runtime/lazuli/observability"
)

// DBPinger is the minimal interface implemented by database pools that can
// report whether they are reachable.
type DBPinger interface {
	Ping(context.Context) error
}

// ErrNilDB is returned when a DB health helper receives a nil database pool.
var ErrNilDB = errors.New("lazuli: database pool is nil")

// PingDB pings db with ctx and returns the ping result.
func PingDB(ctx context.Context, db DBPinger) error {
	if isNilDBPinger(db) {
		return ErrNilDB
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	return db.Ping(ctx)
}

// DBHealthCheck returns a readiness probe that pings db.
//
// An empty name is reported as "db".
func DBHealthCheck(name string, db DBPinger) observability.ReadinessProbe {
	if name == "" {
		name = "db"
	}
	return dbHealthCheck{name: name, db: db}
}

type dbHealthCheck struct {
	name string
	db   DBPinger
}

func (check dbHealthCheck) Name() string {
	return check.name
}

func (check dbHealthCheck) Check(ctx context.Context) error {
	return PingDB(ctx, check.db)
}

func isNilDBPinger(db DBPinger) bool {
	if db == nil {
		return true
	}

	value := reflect.ValueOf(db)
	switch value.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Ptr, reflect.Slice:
		return value.IsNil()
	default:
		return false
	}
}
