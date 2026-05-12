// Package geo contains pure helpers for Lazuli geospatial query generation.
//
// The helpers build PostGIS geography fragments for generated list queries.
// They do not open database connections; callers splice the returned SQL into
// their query builder and append Args to the query's bind values.
package geo

import (
	"errors"
	"fmt"
	"math"
	"strings"
)

var (
	// ErrInvalidLatitude is returned when a latitude is NaN, infinite, or
	// outside the inclusive WGS84 range [-90, 90].
	ErrInvalidLatitude = errors.New("lazuli/geo: latitude must be finite and between -90 and 90")
	// ErrInvalidLongitude is returned when a longitude is NaN, infinite, or
	// outside the inclusive WGS84 range [-180, 180].
	ErrInvalidLongitude = errors.New("lazuli/geo: longitude must be finite and between -180 and 180")
	// ErrInvalidRadius is returned when a radius is NaN, infinite, zero, or
	// negative. PostGIS geography distance arguments are measured in meters.
	ErrInvalidRadius = errors.New("lazuli/geo: radius must be finite and greater than zero meters")
	// ErrInvalidPlaceholder is returned when a fragment's first placeholder is
	// not a positive pgx/PostgreSQL placeholder index.
	ErrInvalidPlaceholder = errors.New("lazuli/geo: first placeholder must be greater than zero")
	// ErrInvalidColumn is returned when a column identifier is empty or not a
	// dotted ASCII SQL identifier controlled by generated code.
	ErrInvalidColumn = errors.New("lazuli/geo: column must be a dotted SQL identifier")
)

// Point is a WGS84 latitude/longitude coordinate.
//
// Lat is the Y coordinate and Lng is the X coordinate. SQL fragments built by
// this package bind longitude first because PostGIS ST_MakePoint expects X/Y.
type Point struct {
	Lat float64 `json:"lat"`
	Lng float64 `json:"lng"`
}

// NewPoint validates and returns a WGS84 point.
func NewPoint(lat, lng float64) (Point, error) {
	p := Point{Lat: lat, Lng: lng}
	return p, p.Validate()
}

// Validate checks that p is within the inclusive WGS84 coordinate bounds.
func (p Point) Validate() error {
	if err := ValidateLatitude(p.Lat); err != nil {
		return err
	}
	if err := ValidateLongitude(p.Lng); err != nil {
		return err
	}
	return nil
}

// ValidateLatitude checks that lat is finite and within [-90, 90].
func ValidateLatitude(lat float64) error {
	if math.IsNaN(lat) || math.IsInf(lat, 0) || lat < -90 || lat > 90 {
		return ErrInvalidLatitude
	}
	return nil
}

// ValidateLongitude checks that lng is finite and within [-180, 180].
func ValidateLongitude(lng float64) error {
	if math.IsNaN(lng) || math.IsInf(lng, 0) || lng < -180 || lng > 180 {
		return ErrInvalidLongitude
	}
	return nil
}

// ValidateRadiusMeters checks that radiusMeters is a positive finite PostGIS
// geography radius in meters.
func ValidateRadiusMeters(radiusMeters float64) error {
	if math.IsNaN(radiusMeters) || math.IsInf(radiusMeters, 0) || radiusMeters <= 0 {
		return ErrInvalidRadius
	}
	return nil
}

// RadiusMetersFromKilometers validates kilometers and converts it to meters.
//
// Hostpoint radius query params are expressed in kilometers, while PostGIS
// geography ST_DWithin expects meters.
func RadiusMetersFromKilometers(kilometers float64) (float64, error) {
	if math.IsNaN(kilometers) || math.IsInf(kilometers, 0) || kilometers <= 0 {
		return 0, ErrInvalidRadius
	}
	meters := kilometers * 1000
	if math.IsInf(meters, 0) {
		return 0, ErrInvalidRadius
	}
	return meters, nil
}

// Fragment is a SQL fragment plus the bind values referenced by it.
type Fragment struct {
	SQL  string
	Args []any
}

// DWithin builds a PostGIS ST_DWithin predicate for a geography point column.
//
// column must be a generated SQL identifier such as "coordinates" or
// "property.coordinates"; it is quoted segment-by-segment. firstPlaceholder is
// the PostgreSQL placeholder index assigned to longitude. The returned Args are
// ordered as longitude, latitude, radiusMeters.
func DWithin(column string, center Point, radiusMeters float64, firstPlaceholder int) (Fragment, error) {
	quotedColumn, err := quoteDottedIdent(column)
	if err != nil {
		return Fragment{}, err
	}
	if err := center.Validate(); err != nil {
		return Fragment{}, err
	}
	if err := ValidateRadiusMeters(radiusMeters); err != nil {
		return Fragment{}, err
	}
	if firstPlaceholder <= 0 {
		return Fragment{}, ErrInvalidPlaceholder
	}

	pointExpr := postGISPointExpr(firstPlaceholder)
	radiusPlaceholder := placeholder(firstPlaceholder + 2)

	var b strings.Builder
	b.Grow(len("ST_DWithin(, , )") + len(quotedColumn) + len(pointExpr) + len(radiusPlaceholder))
	b.WriteString("ST_DWithin(")
	b.WriteString(quotedColumn)
	b.WriteString(", ")
	b.WriteString(pointExpr)
	b.WriteString(", ")
	b.WriteString(radiusPlaceholder)
	b.WriteString(")")

	return Fragment{
		SQL:  b.String(),
		Args: []any{center.Lng, center.Lat, radiusMeters},
	}, nil
}

// Distance builds a PostGIS ST_Distance expression for a geography point column.
//
// The returned fragment is suitable for SELECT projections or ORDER BY clauses.
// Args are ordered as longitude, latitude.
func Distance(column string, center Point, firstPlaceholder int) (Fragment, error) {
	quotedColumn, err := quoteDottedIdent(column)
	if err != nil {
		return Fragment{}, err
	}
	if err := center.Validate(); err != nil {
		return Fragment{}, err
	}
	if firstPlaceholder <= 0 {
		return Fragment{}, ErrInvalidPlaceholder
	}

	pointExpr := postGISPointExpr(firstPlaceholder)

	var b strings.Builder
	b.Grow(len("ST_Distance(, )") + len(quotedColumn) + len(pointExpr))
	b.WriteString("ST_Distance(")
	b.WriteString(quotedColumn)
	b.WriteString(", ")
	b.WriteString(pointExpr)
	b.WriteString(")")

	return Fragment{
		SQL:  b.String(),
		Args: []any{center.Lng, center.Lat},
	}, nil
}

// OrderByDistance builds an ascending ORDER BY fragment using ST_Distance.
func OrderByDistance(column string, center Point, firstPlaceholder int) (Fragment, error) {
	fragment, err := Distance(column, center, firstPlaceholder)
	if err != nil {
		return Fragment{}, err
	}
	fragment.SQL += " ASC"
	return fragment, nil
}

func postGISPointExpr(firstPlaceholder int) string {
	return fmt.Sprintf(
		"ST_SetSRID(ST_MakePoint(%s, %s), 4326)::geography",
		placeholder(firstPlaceholder),
		placeholder(firstPlaceholder+1),
	)
}

func placeholder(n int) string {
	return fmt.Sprintf("$%d", n)
}

func quoteDottedIdent(name string) (string, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return "", ErrInvalidColumn
	}

	parts := strings.Split(name, ".")
	quoted := make([]string, 0, len(parts))
	for _, part := range parts {
		if !isSQLIdent(part) {
			return "", ErrInvalidColumn
		}
		quoted = append(quoted, `"`+part+`"`)
	}
	return strings.Join(quoted, "."), nil
}

func isSQLIdent(s string) bool {
	if s == "" {
		return false
	}
	for _, r := range s {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= 'A' && r <= 'Z':
		case r >= '0' && r <= '9':
		case r == '_':
		default:
			return false
		}
	}
	return true
}
