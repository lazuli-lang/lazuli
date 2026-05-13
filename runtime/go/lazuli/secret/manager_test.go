package secret_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/secret"
)

func TestManagerDescriptorPathAppliesVersionPinAndRendersTemplate(t *testing.T) {
	descriptor := secret.ManagerDescriptor{
		Provider:     " aws.secretsmanager ",
		Source:       " platform ",
		PathTemplate: "/{provider}/{source}/{name}/{version}/{revision}",
		VersionPin: secret.VersionPin{
			Label:    " active ",
			Revision: " 42 ",
		},
		Rotation: secret.RotationMetadata{
			Purpose:       " platform.credentials ",
			Cadence:       30 * 24 * time.Hour,
			Overlap:       time.Hour,
			LastRotatedAt: time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC),
			NextRotatesAt: time.Date(2026, 2, 1, 0, 0, 0, 0, time.UTC),
		},
	}

	if err := descriptor.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	ref := descriptor.Ref(" env.API_TOKEN ")
	if ref.Name != " env.API_TOKEN " {
		t.Fatalf("Ref() name = %q, want original name before path normalization", ref.Name)
	}
	if ref.Version != "active" {
		t.Fatalf("Ref() version = %q, want active", ref.Version)
	}

	got, err := descriptor.Path(secret.Env(" env.API_TOKEN "))
	if err != nil {
		t.Fatalf("Path() error = %v", err)
	}
	want := "/aws.secretsmanager/platform/API_TOKEN/active/42"
	if got != want {
		t.Fatalf("Path() = %q, want %q", got, want)
	}

	got, err = descriptor.Path(secret.Ref("API_TOKEN").WithVersion("previous"))
	if err != nil {
		t.Fatalf("Path() explicit version error = %v", err)
	}
	want = "/aws.secretsmanager/platform/API_TOKEN/previous/42"
	if got != want {
		t.Fatalf("Path() explicit version = %q, want %q", got, want)
	}
}

func TestRenderManagerPathRequiresReferencedValues(t *testing.T) {
	_, err := secret.RenderManagerPath("/{source}/{name}/{version}", secret.ManagerPathValues{
		Source: "platform",
		Name:   "API_TOKEN",
	})
	if !errors.Is(err, secret.ErrManagerPathTemplateInvalid) {
		t.Fatalf("RenderManagerPath() error = %v, want ErrManagerPathTemplateInvalid", err)
	}

	got, err := secret.RenderManagerPath("/{source}/{name}", secret.ManagerPathValues{
		Source: " platform ",
		Name:   " env.API_TOKEN ",
	})
	if err != nil {
		t.Fatalf("RenderManagerPath() without version error = %v", err)
	}
	if got != "/platform/API_TOKEN" {
		t.Fatalf("RenderManagerPath() = %q, want /platform/API_TOKEN", got)
	}

	template := secret.PathTemplate("/{source}/{name}")
	if err := template.Validate(); err != nil {
		t.Fatalf("PathTemplate.Validate() error = %v", err)
	}
	got, err = template.Render(secret.ManagerPathValues{
		Source: "platform",
		Name:   "DB_PASSWORD",
	})
	if err != nil {
		t.Fatalf("PathTemplate.Render() error = %v", err)
	}
	if got != "/platform/DB_PASSWORD" {
		t.Fatalf("PathTemplate.Render() = %q, want /platform/DB_PASSWORD", got)
	}
}

func TestManagerCatalogLookupAndDuplicateValidation(t *testing.T) {
	catalog := secret.ManagerCatalog{Descriptors: []secret.ManagerDescriptor{
		secret.Manager("gcp.secretmanager", "tenant", "projects/demo/secrets/{name}"),
		secret.Manager("aws.secretsmanager", " platform ", "/secrets/{source}/{name}"),
	}}

	if err := catalog.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	descriptor, ok := catalog.LookupSource("platform")
	if !ok {
		t.Fatal("LookupSource() did not find platform")
	}
	if descriptor.Provider != "aws.secretsmanager" {
		t.Fatalf("LookupSource() provider = %q, want aws.secretsmanager", descriptor.Provider)
	}
	if descriptor.Source != "platform" {
		t.Fatalf("LookupSource() source = %q, want normalized platform", descriptor.Source)
	}

	catalog.Descriptors = append(catalog.Descriptors, secret.Manager("local", "tenant", "/{name}"))
	if err := secret.ValidateManagerCatalog(catalog); !errors.Is(err, secret.ErrDuplicateManagerSource) {
		t.Fatalf("ValidateManagerCatalog() error = %v, want ErrDuplicateManagerSource", err)
	}
}

func TestManagerDescriptorValidateRejectsInvalidInputs(t *testing.T) {
	base := secret.Manager("local", "platform", "/{source}/{name}")
	tests := []struct {
		name       string
		descriptor secret.ManagerDescriptor
		want       error
	}{
		{
			name:       "missing provider",
			descriptor: secret.Manager("", "platform", "/{name}"),
			want:       secret.ErrManagerProviderRequired,
		},
		{
			name:       "missing source",
			descriptor: secret.Manager("local", "", "/{name}"),
			want:       secret.ErrManagerSourceRequired,
		},
		{
			name:       "missing template",
			descriptor: secret.Manager("local", "platform", ""),
			want:       secret.ErrManagerPathTemplateRequired,
		},
		{
			name:       "unknown placeholder",
			descriptor: secret.Manager("local", "platform", "/{tenant}/{name}"),
			want:       secret.ErrManagerPathTemplateInvalid,
		},
		{
			name:       "missing revision pin",
			descriptor: secret.Manager("local", "platform", "/{name}/{revision}"),
			want:       secret.ErrManagerPathTemplateInvalid,
		},
		{
			name: "bad version pin",
			descriptor: withVersionPin(base, secret.VersionPin{
				Label: "bad label",
			}),
			want: secret.ErrManagerVersionPinInvalid,
		},
		{
			name: "bad rotation metadata",
			descriptor: withRotation(base, secret.RotationMetadata{
				Purpose: "platform",
				Cadence: time.Hour,
				Overlap: time.Hour,
			}),
			want: secret.ErrManagerRotationInvalid,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.descriptor.Validate(); !errors.Is(err, tt.want) {
				t.Fatalf("Validate() error = %v, want %v", err, tt.want)
			}
		})
	}
}

func TestVersionPinAndRotationMetadataValidation(t *testing.T) {
	pin := secret.PinVersion(" active ")
	if err := pin.Validate(); err != nil {
		t.Fatalf("VersionPin.Validate() error = %v", err)
	}
	if pin.IsZero() {
		t.Fatal("VersionPin.IsZero() = true, want false")
	}

	ref := secret.Ref("api.key")
	if got := pin.Apply(ref); got.Version != "active" {
		t.Fatalf("Apply() version = %q, want active", got.Version)
	}
	if got := secret.PinVersion("active").Apply(ref.WithVersion("previous")); got.Version != "previous" {
		t.Fatalf("Apply() explicit version = %q, want previous", got.Version)
	}
	if err := (secret.VersionPin{Revision: "bad revision"}).Validate(); !errors.Is(err, secret.ErrManagerVersionPinInvalid) {
		t.Fatalf("VersionPin.Validate() invalid error = %v, want ErrManagerVersionPinInvalid", err)
	}

	rotation := secret.RotationMetadata{
		Purpose:       "api.key",
		Cadence:       time.Hour,
		LastRotatedAt: time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC),
		NextRotatesAt: time.Date(2026, 1, 1, 1, 0, 0, 0, time.UTC),
	}
	if err := rotation.Validate(); err != nil {
		t.Fatalf("RotationMetadata.Validate() error = %v", err)
	}
	if rotation.IsZero() {
		t.Fatal("RotationMetadata.IsZero() = true, want false")
	}
}

func withVersionPin(descriptor secret.ManagerDescriptor, pin secret.VersionPin) secret.ManagerDescriptor {
	descriptor.VersionPin = pin
	return descriptor
}

func withRotation(descriptor secret.ManagerDescriptor, rotation secret.RotationMetadata) secret.ManagerDescriptor {
	descriptor.Rotation = rotation
	return descriptor
}
