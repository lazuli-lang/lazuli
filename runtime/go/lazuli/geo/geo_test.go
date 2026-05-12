package geo_test

import (
	"errors"
	"math"
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/geo"
)

func TestNewPointValidatesWGS84Bounds(t *testing.T) {
	t.Parallel()

	point, err := geo.NewPoint(-23.55052, -46.633308)
	if err != nil {
		t.Fatalf("NewPoint(valid) error = %v", err)
	}
	if point.Lat != -23.55052 || point.Lng != -46.633308 {
		t.Fatalf("NewPoint(valid) = %#v", point)
	}

	cases := []struct {
		name    string
		lat     float64
		lng     float64
		wantErr error
	}{
		{name: "latitude below range", lat: -90.0001, lng: 0, wantErr: geo.ErrInvalidLatitude},
		{name: "latitude above range", lat: 90.0001, lng: 0, wantErr: geo.ErrInvalidLatitude},
		{name: "latitude NaN", lat: math.NaN(), lng: 0, wantErr: geo.ErrInvalidLatitude},
		{name: "longitude below range", lat: 0, lng: -180.0001, wantErr: geo.ErrInvalidLongitude},
		{name: "longitude above range", lat: 0, lng: 180.0001, wantErr: geo.ErrInvalidLongitude},
		{name: "longitude infinite", lat: 0, lng: math.Inf(1), wantErr: geo.ErrInvalidLongitude},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			_, err := geo.NewPoint(tc.lat, tc.lng)
			if !errors.Is(err, tc.wantErr) {
				t.Fatalf("NewPoint(%v, %v) error = %v, want %v", tc.lat, tc.lng, err, tc.wantErr)
			}
		})
	}
}

func TestValidateCoordinateEdgesAreInclusive(t *testing.T) {
	t.Parallel()

	for _, lat := range []float64{-90, 0, 90} {
		if err := geo.ValidateLatitude(lat); err != nil {
			t.Fatalf("ValidateLatitude(%v) error = %v", lat, err)
		}
	}
	for _, lng := range []float64{-180, 0, 180} {
		if err := geo.ValidateLongitude(lng); err != nil {
			t.Fatalf("ValidateLongitude(%v) error = %v", lng, err)
		}
	}
}

func TestRadiusMetersFromKilometers(t *testing.T) {
	t.Parallel()

	meters, err := geo.RadiusMetersFromKilometers(12.5)
	if err != nil {
		t.Fatalf("RadiusMetersFromKilometers(valid) error = %v", err)
	}
	if meters != 12500 {
		t.Fatalf("RadiusMetersFromKilometers(valid) = %v, want 12500", meters)
	}

	for _, radius := range []float64{0, -1, math.NaN(), math.Inf(1), math.MaxFloat64} {
		if _, err := geo.RadiusMetersFromKilometers(radius); !errors.Is(err, geo.ErrInvalidRadius) {
			t.Fatalf("RadiusMetersFromKilometers(%v) error = %v, want ErrInvalidRadius", radius, err)
		}
	}
}

func TestDWithinBuildsPostGISRadiusPredicate(t *testing.T) {
	t.Parallel()

	center := geo.Point{Lat: -23.55052, Lng: -46.633308}

	fragment, err := geo.DWithin("property.coordinates", center, 2500, 3)
	if err != nil {
		t.Fatalf("DWithin(valid) error = %v", err)
	}

	wantSQL := `ST_DWithin("property"."coordinates", ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography, $5)`
	if fragment.SQL != wantSQL {
		t.Fatalf("DWithin(valid).SQL = %q, want %q", fragment.SQL, wantSQL)
	}
	wantArgs := []any{-46.633308, -23.55052, float64(2500)}
	if !reflect.DeepEqual(fragment.Args, wantArgs) {
		t.Fatalf("DWithin(valid).Args = %#v, want %#v", fragment.Args, wantArgs)
	}
}

func TestDistanceBuildsPostGISDistanceExpression(t *testing.T) {
	t.Parallel()

	center := geo.Point{Lat: 40.7128, Lng: -74.0060}

	fragment, err := geo.Distance("coordinates", center, 1)
	if err != nil {
		t.Fatalf("Distance(valid) error = %v", err)
	}

	wantSQL := `ST_Distance("coordinates", ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography)`
	if fragment.SQL != wantSQL {
		t.Fatalf("Distance(valid).SQL = %q, want %q", fragment.SQL, wantSQL)
	}
	wantArgs := []any{-74.0060, 40.7128}
	if !reflect.DeepEqual(fragment.Args, wantArgs) {
		t.Fatalf("Distance(valid).Args = %#v, want %#v", fragment.Args, wantArgs)
	}
}

func TestOrderByDistanceAddsAscendingDirection(t *testing.T) {
	t.Parallel()

	fragment, err := geo.OrderByDistance("coordinates", geo.Point{Lat: 1, Lng: 2}, 6)
	if err != nil {
		t.Fatalf("OrderByDistance(valid) error = %v", err)
	}

	wantSQL := `ST_Distance("coordinates", ST_SetSRID(ST_MakePoint($6, $7), 4326)::geography) ASC`
	if fragment.SQL != wantSQL {
		t.Fatalf("OrderByDistance(valid).SQL = %q, want %q", fragment.SQL, wantSQL)
	}
}

func TestFragmentsRejectInvalidInputs(t *testing.T) {
	t.Parallel()

	center := geo.Point{Lat: 1, Lng: 2}

	cases := []struct {
		name    string
		build   func() (geo.Fragment, error)
		wantErr error
	}{
		{
			name: "suspicious column",
			build: func() (geo.Fragment, error) {
				return geo.DWithin("coordinates;DROP", center, 1000, 1)
			},
			wantErr: geo.ErrInvalidColumn,
		},
		{
			name: "empty column segment",
			build: func() (geo.Fragment, error) {
				return geo.Distance("property..coordinates", center, 1)
			},
			wantErr: geo.ErrInvalidColumn,
		},
		{
			name: "invalid point",
			build: func() (geo.Fragment, error) {
				return geo.DWithin("coordinates", geo.Point{Lat: 91, Lng: 0}, 1000, 1)
			},
			wantErr: geo.ErrInvalidLatitude,
		},
		{
			name: "invalid radius",
			build: func() (geo.Fragment, error) {
				return geo.DWithin("coordinates", center, 0, 1)
			},
			wantErr: geo.ErrInvalidRadius,
		},
		{
			name: "invalid placeholder",
			build: func() (geo.Fragment, error) {
				return geo.Distance("coordinates", center, 0)
			},
			wantErr: geo.ErrInvalidPlaceholder,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			_, err := tc.build()
			if !errors.Is(err, tc.wantErr) {
				t.Fatalf("%s error = %v, want %v", tc.name, err, tc.wantErr)
			}
		})
	}
}
