package jsonx

import (
	"errors"
	"strings"
	"testing"
)

func TestNormalizeNullPolicyCanonicalizesAliases(t *testing.T) {
	tests := []struct {
		name   string
		policy NullPolicy
		want   NullPolicy
	}{
		{name: "empty default", policy: "", want: NullPolicyDefault},
		{name: "trim and case", policy: " Include ", want: NullPolicyInclude},
		{name: "omit alias", policy: "omit", want: NullPolicyOmitEmpty},
		{name: "omit empty alias", policy: "omit_empty", want: NullPolicyOmitEmpty},
		{name: "hyphen null as empty", policy: "null-as-empty", want: NullPolicyNullAsEmpty},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := NormalizeNullPolicy(tt.policy)
			if err != nil {
				t.Fatalf("NormalizeNullPolicy(%q) error = %v", tt.policy, err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeNullPolicy(%q) = %q, want %q", tt.policy, got, tt.want)
			}
		})
	}
}

func TestNormalizeNullPolicyRejectsUnknownPolicy(t *testing.T) {
	_, err := NormalizeNullPolicy("always")
	if !errors.Is(err, ErrInvalidNullPolicy) {
		t.Fatalf("NormalizeNullPolicy error = %v, want ErrInvalidNullPolicy", err)
	}
}

func TestValidateFieldMetadata(t *testing.T) {
	tests := []struct {
		name  string
		field FieldMetadata
		want  error
	}{
		{
			name:  "valid nullable null policy",
			field: FieldMetadata{Name: "Email", JSONName: "email", Policy: NullPolicyNull, Nullable: true},
		},
		{
			name:  "missing json name",
			field: FieldMetadata{Name: "Email"},
			want:  ErrInvalidFieldMetadata,
		},
		{
			name:  "invalid policy",
			field: FieldMetadata{JSONName: "email", Policy: "bad"},
			want:  ErrInvalidFieldMetadata,
		},
		{
			name:  "required omitempty",
			field: FieldMetadata{JSONName: "email", Policy: NullPolicyOmitEmpty, Required: true},
			want:  ErrInvalidFieldMetadata,
		},
		{
			name:  "null policy requires nullable",
			field: FieldMetadata{JSONName: "email", Policy: NullPolicyNull},
			want:  ErrInvalidFieldMetadata,
		},
		{
			name:  "control character json name",
			field: FieldMetadata{JSONName: "bad\nname"},
			want:  ErrInvalidFieldMetadata,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.field.Validate()
			if tt.want == nil {
				if err != nil {
					t.Fatalf("Validate() error = %v", err)
				}
				return
			}
			if !errors.Is(err, tt.want) {
				t.Fatalf("Validate() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestValidateFieldsRejectsDuplicateJSONNames(t *testing.T) {
	err := ValidateFields([]FieldMetadata{
		{Name: "First", JSONName: "email"},
		{Name: "Second", JSONName: "email"},
	})
	if !errors.Is(err, ErrInvalidFieldMetadata) {
		t.Fatalf("ValidateFields() error = %v, want ErrInvalidFieldMetadata", err)
	}
	if !strings.Contains(err.Error(), "duplicates field[0]") {
		t.Fatalf("ValidateFields() error = %q, want duplicate detail", err)
	}
}

func TestDecideField(t *testing.T) {
	tests := []struct {
		name  string
		field FieldMetadata
		state FieldState
		want  FieldDecision
	}{
		{
			name:  "default includes value",
			field: FieldMetadata{JSONName: "name"},
			want:  FieldDecisionInclude,
		},
		{
			name:  "omitempty omits empty value",
			field: FieldMetadata{JSONName: "nickname", Policy: NullPolicyOmitEmpty},
			state: FieldState{Empty: true},
			want:  FieldDecisionOmit,
		},
		{
			name:  "include keeps empty value",
			field: FieldMetadata{JSONName: "nickname", Policy: NullPolicyInclude},
			state: FieldState{Empty: true},
			want:  FieldDecisionInclude,
		},
		{
			name:  "null policy emits null for empty nullable value",
			field: FieldMetadata{JSONName: "deleted_at", Policy: NullPolicyNull, Nullable: true},
			state: FieldState{Empty: true},
			want:  FieldDecisionNull,
		},
		{
			name:  "nullable null emits null by default",
			field: FieldMetadata{JSONName: "deleted_at", Nullable: true},
			state: FieldState{Null: true},
			want:  FieldDecisionNull,
		},
		{
			name:  "null as empty preserves field",
			field: FieldMetadata{JSONName: "items", Policy: NullPolicyNullAsEmpty},
			state: FieldState{Null: true},
			want:  FieldDecisionNullAsEmpty,
		},
		{
			name:  "absent optional omits field",
			field: FieldMetadata{JSONName: "email"},
			state: FieldState{Absent: true},
			want:  FieldDecisionOmit,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := DecideField(tt.field, tt.state)
			if err != nil {
				t.Fatalf("DecideField() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("DecideField() = %s, want %s", got, tt.want)
			}
		})
	}
}

func TestDecideFieldReportsRequiredAbsentAndDisallowedNull(t *testing.T) {
	_, err := DecideField(FieldMetadata{JSONName: "id", Required: true}, FieldState{Absent: true})
	if !errors.Is(err, ErrJSONFieldRequired) {
		t.Fatalf("DecideField(required absent) error = %v, want ErrJSONFieldRequired", err)
	}

	_, err = DecideField(FieldMetadata{JSONName: "id"}, FieldState{Null: true})
	if !errors.Is(err, ErrJSONNullDisallowed) {
		t.Fatalf("DecideField(non-nullable null) error = %v, want ErrJSONNullDisallowed", err)
	}
}

func TestDecideFieldRejectsInvalidState(t *testing.T) {
	_, err := DecideField(FieldMetadata{JSONName: "name"}, FieldState{Null: true, Empty: true})
	if !errors.Is(err, ErrInvalidFieldState) {
		t.Fatalf("DecideField() error = %v, want ErrInvalidFieldState", err)
	}
}

func TestDecideNull(t *testing.T) {
	tests := []struct {
		name  string
		field FieldMetadata
		want  NullDecision
	}{
		{
			name:  "non nullable rejects",
			field: FieldMetadata{JSONName: "id"},
			want:  NullDecisionReject,
		},
		{
			name:  "nullable accepts",
			field: FieldMetadata{JSONName: "deleted_at", Nullable: true},
			want:  NullDecisionAccept,
		},
		{
			name:  "null as empty accepts as empty",
			field: FieldMetadata{JSONName: "items", Policy: NullPolicyNullAsEmpty},
			want:  NullDecisionAsEmpty,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := DecideNull(tt.field)
			if err != nil {
				t.Fatalf("DecideNull() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("DecideNull() = %s, want %s", got, tt.want)
			}
		})
	}
}
