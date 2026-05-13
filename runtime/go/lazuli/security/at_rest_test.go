package security

import (
	"errors"
	"testing"
)

func TestAtRestPolicyDecisionUsesMostSpecificRule(t *testing.T) {
	t.Parallel()

	customerKey := AtRestKey("kms/customer-pii").WithProvider(" AWS.KMS ").WithVersion(" 2026-01 ")
	bucketKey := AtRestKey("projects/demo/keyRings/files/cryptoKeys/uploads").WithProvider("gcp.kms")
	policy := AtRestPolicy{
		DefaultRequirement: AtRestOptional,
		Rules: []AtRestRule{
			RequiredAtRest(ResourceAtRestTarget(" Customer "), customerKey, AtRestAES256GCM()),
			OptionalAtRest(FieldAtRestTarget("Customer", "marketing_opt_in")),
			RequiredAtRest(BucketAtRestTarget("uploads"), bucketKey, AtRestAlgorithm{
				Name:            " provider-managed ",
				ProviderManaged: true,
			}),
		},
	}

	decision, err := policy.Decision(FieldAtRestTarget("Customer", "tax_id"))
	if err != nil {
		t.Fatalf("Decision(resource field) error = %v", err)
	}
	if !decision.Required() {
		t.Fatal("tax_id decision was optional, want required")
	}
	if decision.RuleIndex != 0 {
		t.Fatalf("tax_id RuleIndex = %d, want 0", decision.RuleIndex)
	}
	if decision.KeyRef.Provider != "aws.kms" {
		t.Fatalf("tax_id key provider = %q, want aws.kms", decision.KeyRef.Provider)
	}
	if decision.KeyRef.Version != "2026-01" {
		t.Fatalf("tax_id key version = %q, want 2026-01", decision.KeyRef.Version)
	}
	if decision.Algorithm.Name != AtRestAlgorithmAES256GCM || decision.Algorithm.KeySizeBits != 256 {
		t.Fatalf("tax_id algorithm = %#v, want AES-256-GCM", decision.Algorithm)
	}

	decision, err = policy.Decision(FieldAtRestTarget("Customer", "marketing_opt_in"))
	if err != nil {
		t.Fatalf("Decision(field override) error = %v", err)
	}
	if decision.Required() {
		t.Fatal("marketing_opt_in decision was required, want optional")
	}
	if decision.RuleIndex != 1 {
		t.Fatalf("marketing_opt_in RuleIndex = %d, want 1", decision.RuleIndex)
	}
	if !decision.KeyRef.IsZero() || !decision.Algorithm.IsZero() {
		t.Fatalf("marketing_opt_in carried key/algorithm metadata: %#v %#v", decision.KeyRef, decision.Algorithm)
	}

	decision, err = policy.Decision(BucketAtRestTarget("uploads"))
	if err != nil {
		t.Fatalf("Decision(bucket) error = %v", err)
	}
	if !decision.Required() {
		t.Fatal("bucket decision was optional, want required")
	}
	if decision.RuleIndex != 2 {
		t.Fatalf("bucket RuleIndex = %d, want 2", decision.RuleIndex)
	}
	if !decision.Algorithm.ProviderManaged {
		t.Fatal("bucket algorithm ProviderManaged = false, want true")
	}

	required, err := policy.RequiresEncryption(ResourceAtRestTarget("Invoice"))
	if err != nil {
		t.Fatalf("RequiresEncryption(default) error = %v", err)
	}
	if required {
		t.Fatal("default requirement = required, want optional")
	}
}

func TestAtRestPolicyDefaultDecision(t *testing.T) {
	t.Parallel()

	policy := AtRestPolicy{
		DefaultRequirement: AtRestRequired,
		DefaultKeyRef:      AtRestKey("kms/default"),
		DefaultAlgorithm:   AtRestAES256GCM(),
	}

	decision, err := DecideAtRest(policy, FieldAtRestTarget("Order", "card_token"))
	if err != nil {
		t.Fatalf("DecideAtRest(default) error = %v", err)
	}
	if !decision.Required() {
		t.Fatal("default decision was optional, want required")
	}
	if decision.RuleIndex != -1 {
		t.Fatalf("default RuleIndex = %d, want -1", decision.RuleIndex)
	}
	if decision.Target != FieldAtRestTarget("Order", "card_token") {
		t.Fatalf("default Target = %#v", decision.Target)
	}
}

func TestAtRestPolicyValidationRejectsInvalidPolicies(t *testing.T) {
	t.Parallel()

	validKey := AtRestKey("kms/customer")
	validAlgorithm := AtRestAES256GCM()
	tests := []struct {
		name   string
		policy AtRestPolicy
	}{
		{
			name: "required default without key",
			policy: AtRestPolicy{
				DefaultRequirement: AtRestRequired,
				DefaultAlgorithm:   validAlgorithm,
			},
		},
		{
			name: "required rule without algorithm",
			policy: AtRestPolicy{Rules: []AtRestRule{
				{
					Target:      FieldAtRestTarget("Customer", "ssn"),
					Requirement: AtRestRequired,
					KeyRef:      validKey,
				},
			}},
		},
		{
			name: "key without algorithm",
			policy: AtRestPolicy{Rules: []AtRestRule{
				{
					Target: FieldAtRestTarget("Customer", "ssn"),
					KeyRef: validKey,
				},
			}},
		},
		{
			name: "unknown requirement",
			policy: AtRestPolicy{Rules: []AtRestRule{
				{
					Target:      ResourceAtRestTarget("Customer"),
					Requirement: AtRestRequirement(99),
				},
			}},
		},
		{
			name: "duplicate normalized target",
			policy: AtRestPolicy{Rules: []AtRestRule{
				OptionalAtRest(ResourceAtRestTarget("Customer")),
				OptionalAtRest(ResourceAtRestTarget(" Customer ")),
			}},
		},
		{
			name: "field without resource",
			policy: AtRestPolicy{Rules: []AtRestRule{
				OptionalAtRest(AtRestTarget{Field: "ssn"}),
			}},
		},
		{
			name: "target mixes bucket and field",
			policy: AtRestPolicy{Rules: []AtRestRule{
				OptionalAtRest(AtRestTarget{Bucket: "uploads", Resource: "Customer", Field: "avatar"}),
			}},
		},
		{
			name: "unsafe target name",
			policy: AtRestPolicy{Rules: []AtRestRule{
				OptionalAtRest(FieldAtRestTarget("Customer", "bad field")),
			}},
		},
		{
			name: "unsafe key ref",
			policy: AtRestPolicy{Rules: []AtRestRule{
				RequiredAtRest(FieldAtRestTarget("Customer", "ssn"), AtRestKey("bad key"), validAlgorithm),
			}},
		},
		{
			name: "unsafe algorithm",
			policy: AtRestPolicy{Rules: []AtRestRule{
				RequiredAtRest(FieldAtRestTarget("Customer", "ssn"), validKey, AtRestAlgorithm{Name: "aes 256"}),
			}},
		},
		{
			name: "wrong aes key size",
			policy: AtRestPolicy{Rules: []AtRestRule{
				RequiredAtRest(FieldAtRestTarget("Customer", "ssn"), validKey, AtRestAlgorithm{Name: AtRestAlgorithmAES256GCM, KeySizeBits: 128}),
			}},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			err := tt.policy.Validate()
			if !errors.Is(err, ErrAtRestPolicyInvalid) {
				t.Fatalf("Validate() error = %v, want ErrAtRestPolicyInvalid", err)
			}
		})
	}
}

func TestAtRestValueHelpers(t *testing.T) {
	t.Parallel()

	if got := AtRestRequired.String(); got != "required" {
		t.Fatalf("AtRestRequired.String() = %q, want required", got)
	}
	if got := AtRestRequirement(99).String(); got != "unknown" {
		t.Fatalf("unknown requirement String() = %q, want unknown", got)
	}
	if !AtRestRequired.Required() {
		t.Fatal("AtRestRequired.Required() = false, want true")
	}
	if AtRestOptional.Required() {
		t.Fatal("AtRestOptional.Required() = true, want false")
	}

	target := FieldAtRestTarget(" Customer ", " ssn ").Normalize()
	if target.Kind() != AtRestTargetField {
		t.Fatalf("target Kind() = %s, want field", target.Kind())
	}
	if got := target.String(); got != "resource=Customer,field=ssn" {
		t.Fatalf("target String() = %q, want resource=Customer,field=ssn", got)
	}
	if got := BucketAtRestTarget("uploads").Kind().String(); got != "bucket" {
		t.Fatalf("bucket kind String() = %q, want bucket", got)
	}
	if got := AtRestTargetKind(99).String(); got != "unknown" {
		t.Fatalf("unknown kind String() = %q, want unknown", got)
	}

	key := AtRestKey(" kms/customer ").WithProvider(" AWS.KMS ").WithVersion(" current ").Normalize()
	if key.Provider != "aws.kms" || key.Name != "kms/customer" || key.Version != "current" {
		t.Fatalf("normalized key = %#v", key)
	}
	if got := key.String(); got != "provider=aws.kms,name=kms/customer,version=current" {
		t.Fatalf("key String() = %q", got)
	}
	if got := (AtRestKeyRef{}).String(); got != "unset" {
		t.Fatalf("empty key String() = %q, want unset", got)
	}

	algorithm := AtRestAlgorithm{Name: " AES-256-GCM ", KeySizeBits: 256, Envelope: true}.Normalize()
	if algorithm.Name != AtRestAlgorithmAES256GCM {
		t.Fatalf("normalized algorithm name = %q", algorithm.Name)
	}
	if algorithm.IsZero() {
		t.Fatal("algorithm IsZero() = true, want false")
	}
	if !(AtRestAlgorithm{}).IsZero() {
		t.Fatal("zero algorithm IsZero() = false, want true")
	}
}

func TestAtRestRuleMatches(t *testing.T) {
	t.Parallel()

	resourceRule := OptionalAtRest(ResourceAtRestTarget("Customer"))
	if !resourceRule.Matches(ResourceAtRestTarget("Customer")) {
		t.Fatal("resource rule did not match resource target")
	}
	if !resourceRule.Matches(FieldAtRestTarget("Customer", "email")) {
		t.Fatal("resource rule did not match field target")
	}
	if resourceRule.Matches(FieldAtRestTarget("Invoice", "email")) {
		t.Fatal("resource rule matched another resource")
	}

	fieldRule := OptionalAtRest(FieldAtRestTarget("Customer", "email"))
	if !fieldRule.Matches(FieldAtRestTarget("Customer", "email")) {
		t.Fatal("field rule did not match exact field")
	}
	if fieldRule.Matches(ResourceAtRestTarget("Customer")) {
		t.Fatal("field rule matched resource target")
	}

	bucketRule := OptionalAtRest(BucketAtRestTarget("uploads"))
	if !bucketRule.Matches(BucketAtRestTarget("uploads")) {
		t.Fatal("bucket rule did not match bucket target")
	}
	if bucketRule.Matches(FieldAtRestTarget("uploads", "file")) {
		t.Fatal("bucket rule matched field target")
	}
}

func TestAtRestDecisionValidate(t *testing.T) {
	t.Parallel()

	decision := AtRestDecision{
		Target:      FieldAtRestTarget("Customer", "ssn"),
		Requirement: AtRestRequired,
		KeyRef:      AtRestKey("kms/customer"),
		Algorithm:   AtRestAES256GCM(),
		RuleIndex:   -1,
	}
	if err := decision.Validate(); err != nil {
		t.Fatalf("Validate(valid) error = %v", err)
	}

	decision.KeyRef = AtRestKeyRef{}
	if err := decision.Validate(); !errors.Is(err, ErrAtRestPolicyInvalid) {
		t.Fatalf("Validate(missing key) error = %v, want ErrAtRestPolicyInvalid", err)
	}
}
