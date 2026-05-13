package deploy_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestRenderHelmChartMetadataRendersDeterministically(t *testing.T) {
	metadata := deploy.ChartMetadata("checkout-api", "0.3.1")
	metadata.Description = "Checkout API chart"
	metadata.AppVersion = "1.2.3"
	metadata.KubeVersion = ">=1.28.0-0"
	metadata.Keywords = []string{"lazuli", "api", "api"}
	metadata.Maintainers = []deploy.HelmChartMaintainer{
		deploy.Maintainer("Zoe", "", "https://example.com/zoe"),
		deploy.Maintainer("Ada", "ada@example.com", ""),
	}
	metadata.Annotations = map[string]string{
		"org.opencontainers.image.source": "https://example.com/repo",
		"lazuli.dev/runtime":              "go",
	}

	got, err := deploy.RenderHelmChartMetadata(metadata)
	if err != nil {
		t.Fatalf("RenderHelmChartMetadata() error = %v", err)
	}

	want := `apiVersion: "v2"
name: "checkout-api"
description: "Checkout API chart"
type: "application"
version: "0.3.1"
appVersion: "1.2.3"
kubeVersion: ">=1.28.0-0"
keywords:
  - "api"
  - "lazuli"
maintainers:
  - name: "Ada"
    email: "ada@example.com"
  - name: "Zoe"
    url: "https://example.com/zoe"
annotations:
  lazuli.dev/runtime: "go"
  org.opencontainers.image.source: "https://example.com/repo"
`
	if got != want {
		t.Fatalf("RenderHelmChartMetadata() =\n%s\nwant\n%s", got, want)
	}
}

func TestPlanHelmTemplateFilesNormalizesValuesAndFiles(t *testing.T) {
	schema := deploy.ValuesSchema(
		deploy.HelmValue(".Values.service.port", deploy.HelmValueInteger),
		deploy.HelmValue("image.repository", deploy.HelmValueString),
		deploy.HelmValue("image.tag", deploy.HelmValueString),
		deploy.HelmValue("replicaCount", deploy.HelmValueInteger),
	)
	schema.Fields[0].Default = "80"
	schema.Fields[1].Required = true
	schema.Fields[2].Default = "1.2.3"
	schema.Fields[3].Default = "2"

	manifest := deploy.NewHelmChart(
		deploy.ChartMetadata("checkout-api", "0.1.0"),
		schema,
		deploy.HelmTemplate("templates/service.yaml", "v1", "Service", ".Values.service.port"),
		deploy.HelmTemplate("templates/deployment.yaml", "apps/v1", "Deployment", "image.tag", "image.repository", "replicaCount", "image.tag"),
	)

	templates, err := deploy.PlanHelmTemplateFiles(manifest)
	if err != nil {
		t.Fatalf("PlanHelmTemplateFiles() error = %v", err)
	}
	wantTemplates := []deploy.HelmTemplateFilePlan{
		{
			Path:       "templates/deployment.yaml",
			APIVersion: "apps/v1",
			Kind:       "Deployment",
			Values:     []string{"image.repository", "image.tag", "replicaCount"},
		},
		{
			Path:       "templates/service.yaml",
			APIVersion: "v1",
			Kind:       "Service",
			Values:     []string{"service.port"},
		},
	}
	if !reflect.DeepEqual(templates, wantTemplates) {
		t.Fatalf("PlanHelmTemplateFiles() = %#v, want %#v", templates, wantTemplates)
	}

	files, err := manifest.FilePlan()
	if err != nil {
		t.Fatalf("FilePlan() error = %v", err)
	}
	wantFiles := []deploy.HelmChartFilePlan{
		{Path: "Chart.yaml", Role: "metadata", Description: "Helm chart metadata."},
		{Path: "values.yaml", Role: "values", Description: "Default values described by the chart values schema."},
		{Path: "templates/deployment.yaml", Role: "template", Description: "Deployment manifest template plan."},
		{Path: "templates/service.yaml", Role: "template", Description: "Service manifest template plan."},
	}
	if !reflect.DeepEqual(files, wantFiles) {
		t.Fatalf("FilePlan() = %#v, want %#v", files, wantFiles)
	}

	defaults, err := schema.Defaults()
	if err != nil {
		t.Fatalf("Defaults() error = %v", err)
	}
	wantDefaults := map[string]string{
		"image.tag":    "1.2.3",
		"replicaCount": "2",
		"service.port": "80",
	}
	if !reflect.DeepEqual(defaults, wantDefaults) {
		t.Fatalf("Defaults() = %#v, want %#v", defaults, wantDefaults)
	}
}

func TestValidateHelmValuesSchemaRejectsInvalidFields(t *testing.T) {
	err := deploy.ValidateHelmValuesSchema(deploy.ValuesSchema(
		deploy.HelmValue("image..tag", deploy.HelmValueString),
		deploy.HelmValue("replicaCount", deploy.HelmValueInteger),
		deploy.HelmValue("replicaCount", deploy.HelmValueInteger),
		deploy.HelmValue("mode", deploy.HelmValueObject),
	))
	if !errors.Is(err, deploy.ErrInvalidHelmChart) {
		t.Fatalf("ValidateHelmValuesSchema() error = %v, want ErrInvalidHelmChart", err)
	}
	for _, fragment := range []string{
		"values.fields[0].path",
		"duplicate values path",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateHelmValuesSchema() error = %v, want fragment %q", err, fragment)
		}
	}

	schema := deploy.ValuesSchema(
		deploy.HelmValue("replicaCount", deploy.HelmValueInteger),
		deploy.HelmValue("mode", deploy.HelmValueObject),
	)
	schema.Fields[0].Default = "two"
	schema.Fields[1].Enum = []string{"prod"}

	err = deploy.ValidateHelmValuesSchema(schema)
	if !errors.Is(err, deploy.ErrInvalidHelmChart) {
		t.Fatalf("ValidateHelmValuesSchema(defaults) error = %v, want ErrInvalidHelmChart", err)
	}
	for _, fragment := range []string{
		"values.fields[0].default",
		"values.fields[1].enum",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateHelmValuesSchema(defaults) error = %v, want fragment %q", err, fragment)
		}
	}
}

func TestValidateHelmChartRejectsInvalidMetadataAndTemplates(t *testing.T) {
	schema := deploy.ValuesSchema(
		deploy.HelmValue("image.repository", deploy.HelmValueString),
	)
	err := deploy.ValidateHelmChart(deploy.NewHelmChart(
		deploy.HelmChartMetadata{
			Name:    "Bad_Name",
			Version: "1",
		},
		schema,
		deploy.HelmTemplate("deployment.yaml", "apps/v1", "Deployment", "image.tag"),
		deploy.HelmTemplate("templates/service.yaml", "", "Service", "image.repository"),
		deploy.HelmTemplate("templates/service.yaml", "v1", "Service", "image.repository"),
	))
	if !errors.Is(err, deploy.ErrInvalidHelmChart) {
		t.Fatalf("ValidateHelmChart() error = %v, want ErrInvalidHelmChart", err)
	}
	for _, fragment := range []string{
		"metadata.name",
		"metadata.version",
		"templates[0].path",
		"unknown values path",
		"templates[1].api_version",
		"duplicate template path",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateHelmChart() error = %v, want fragment %q", err, fragment)
		}
	}
}
