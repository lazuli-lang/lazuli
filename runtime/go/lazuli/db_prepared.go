package lazuli

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"sort"
	"sync"

	"github.com/jackc/pgx/v5/pgconn"
)

var (
	errNilPreparedStatementCache    = errors.New("lazuli: nil prepared statement cache")
	errNilPreparedStatementPreparer = errors.New("lazuli: nil prepared statement preparer")
)

// PreparedStatementPreparer is the minimal pgx-like API needed to prepare a
// named statement.
type PreparedStatementPreparer interface {
	Prepare(context.Context, string, string) (*pgconn.StatementDescription, error)
}

// PreparedStatementDeallocator is implemented by pgx connections that can
// release named prepared statements.
type PreparedStatementDeallocator interface {
	Deallocate(context.Context, string) error
}

// PreparedStatementCache prepares named SQL statements once per cache and
// reuses the returned statement description for later calls with the same
// name/sql pair. Scope each cache to one physical connection or transaction,
// because Postgres prepared statements are connection-local.
type PreparedStatementCache struct {
	mu         sync.Mutex
	statements map[string]preparedStatementCacheEntry
}

type preparedStatementCacheEntry struct {
	sql         string
	description *pgconn.StatementDescription
}

// NewPreparedStatementCache returns an empty prepared statement cache.
func NewPreparedStatementCache() *PreparedStatementCache {
	return &PreparedStatementCache{}
}

// Prepare prepares name/sql on db once and returns the cached statement
// description on later calls with the same name/sql pair.
func (cache *PreparedStatementCache) Prepare(ctx context.Context, db PreparedStatementPreparer, name, sql string) (*pgconn.StatementDescription, error) {
	if cache == nil {
		return nil, errNilPreparedStatementCache
	}
	if isNilPreparedStatementValue(db) {
		return nil, errNilPreparedStatementPreparer
	}
	if name == "" {
		return nil, errors.New("lazuli: prepared statement name is empty")
	}
	if sql == "" {
		return nil, errors.New("lazuli: prepared statement SQL is empty")
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	cache.mu.Lock()
	defer cache.mu.Unlock()

	if cache.statements == nil {
		cache.statements = make(map[string]preparedStatementCacheEntry)
	}

	if statement, ok := cache.statements[name]; ok {
		if statement.sql != sql {
			return nil, fmt.Errorf("lazuli: prepared statement %q already cached with different SQL", name)
		}
		return statement.description, nil
	}

	description, err := db.Prepare(ctx, name, sql)
	if err != nil {
		return nil, err
	}

	cache.statements[name] = preparedStatementCacheEntry{
		sql:         sql,
		description: description,
	}
	return description, nil
}

// Close deallocates every cached prepared statement when db supports
// PreparedStatementDeallocator. If db does not support deallocation, Close
// clears only the local cache.
func (cache *PreparedStatementCache) Close(ctx context.Context, db any) error {
	if cache == nil {
		return errNilPreparedStatementCache
	}

	cache.mu.Lock()
	defer cache.mu.Unlock()

	if len(cache.statements) == 0 {
		return nil
	}

	deallocator, ok := db.(PreparedStatementDeallocator)
	if !ok || isNilPreparedStatementValue(deallocator) {
		cache.statements = nil
		return nil
	}

	names := make([]string, 0, len(cache.statements))
	for name := range cache.statements {
		names = append(names, name)
	}
	sort.Strings(names)

	var closeErr error
	for _, name := range names {
		if err := deallocator.Deallocate(ctx, name); err != nil {
			closeErr = errors.Join(closeErr, err)
			continue
		}
		delete(cache.statements, name)
	}
	if len(cache.statements) == 0 {
		cache.statements = nil
	}
	return closeErr
}

func isNilPreparedStatementValue(value any) bool {
	if value == nil {
		return true
	}

	reflected := reflect.ValueOf(value)
	switch reflected.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Ptr, reflect.Slice:
		return reflected.IsNil()
	default:
		return false
	}
}
