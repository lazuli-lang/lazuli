package deploy_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestRenderTerraformModulePlanIsDeterministicAndRedacted(t *testing.T) {
	plan := deploy.TerraformModulePlan{
		Providers: []deploy.TerraformProviderDescriptor{
			deploy.TerraformProvider("google", "hashicorp/google", "~> 5.0",
				deploy.TerraformInput("project", "lazuli-prod"),
				deploy.TerraformSensitiveInput("credentials_json", "placeholder-credential"),
			),
			deploy.TerraformProvider("random", "hashicorp/random", "3.6.0"),
		},
		Modules: []deploy.TerraformModuleDescriptor{
			{
				Name:    "service",
				Source:  deploy.TerraformSource("./modules/service"),
				Version: deploy.TerraformVersion(""),
				Providers: []string{
					"google",
				},
				Inputs: []deploy.TerraformVariableDescriptor{
					deploy.TerraformInput("labels", map[string]any{
						"tier": "api",
						"env":  "prod",
					}),
					{Name: "replicas", Type: deploy.TerraformValueNumber, Value: 3},
					deploy.TerraformSensitiveInput("database_url", "placeholder-database-url"),
				},
				Outputs: []deploy.TerraformOutputDescriptor{
					{Name: "url", Description: "Service URL"},
				},
			},
		},
	}

	got, err := deploy.RenderTerraformModulePlan(plan)
	if err != nil {
		t.Fatalf("RenderTerraformModulePlan() error = %v", err)
	}

	want := `terraform {
  required_providers {
    google = {
      source = "hashicorp/google"
      version = "~> 5.0"
    }
    random = {
      source = "hashicorp/random"
      version = "3.6.0"
    }
  }
}

provider "google" {
  credentials_json = sensitive("[REDACTED]")
  project = "lazuli-prod"
}

provider "random" {
}

module "service" {
  source = "./modules/service"
  providers = {
    google = google
  }
  database_url = sensitive("[REDACTED]")
  labels = { env = "prod", tier = "api" }
  replicas = 3
}

output "service_url" {
  value = module.service.url
  description = "Service URL"
}
`
	if got != want {
		t.Fatalf("RenderTerraformModulePlan() =\n%s\nwant\n%s", got, want)
	}
	for _, fragment := range []string{"placeholder-credential", "placeholder-database-url"} {
		if strings.Contains(got, fragment) {
			t.Fatalf("RenderTerraformModulePlan() leaked sensitive fragment %q in:\n%s", fragment, got)
		}
	}
}

func TestTerraformModulePlanSafeSummaryRedactsAndCopies(t *testing.T) {
	plan := deploy.TerraformModulePlan{
		Providers: []deploy.TerraformProviderDescriptor{
			deploy.TerraformProvider("google", "hashicorp/google", "~> 5.0",
				deploy.TerraformSensitiveInput("credentials_json", "placeholder-credential"),
			),
		},
		Modules: []deploy.TerraformModuleDescriptor{
			{
				Name:      "service",
				Source:    deploy.TerraformSource("./modules/service"),
				Providers: []string{"google"},
				Inputs: []deploy.TerraformVariableDescriptor{
					deploy.TerraformInput("name", "api"),
					deploy.TerraformSensitiveInput("database_url", "placeholder-database-url"),
				},
				Outputs: []deploy.TerraformOutputDescriptor{
					{Name: "url"},
				},
			},
		},
	}

	summary, err := plan.SafeSummary()
	if err != nil {
		t.Fatalf("SafeSummary() error = %v", err)
	}

	if got := summary.Providers[0].Inputs["credentials_json"]; got != deploy.DefaultTerraformSensitiveMask {
		t.Fatalf("provider sensitive summary = %q", got)
	}
	if got := summary.Modules[0].Inputs["database_url"]; got != deploy.DefaultTerraformSensitiveMask {
		t.Fatalf("module sensitive summary = %q", got)
	}
	if got := summary.Modules[0].Inputs["name"]; got != `"api"` {
		t.Fatalf("module literal summary = %q", got)
	}

	summary.Modules[0].Providers[0] = "mutated"
	again, err := plan.SafeSummary()
	if err != nil {
		t.Fatalf("SafeSummary() second error = %v", err)
	}
	if !reflect.DeepEqual(again.Modules[0].Providers, []string{"google"}) {
		t.Fatalf("SafeSummary() returned shared providers slice: %#v", again.Modules[0].Providers)
	}
}

func TestTerraformVariableRedactedValueValidatesAndMasks(t *testing.T) {
	got, err := (deploy.TerraformVariableDescriptor{
		Name:      "zones",
		Type:      deploy.TerraformValueList,
		Value:     []string{"us-central1-a", "us-central1-b"},
		Sensitive: true,
	}).RedactedValue()
	if err != nil {
		t.Fatalf("RedactedValue() error = %v", err)
	}
	if got != deploy.DefaultTerraformSensitiveMask {
		t.Fatalf("RedactedValue() = %q", got)
	}

	_, err = (deploy.TerraformVariableDescriptor{
		Name:  "replicas",
		Type:  deploy.TerraformValueNumber,
		Value: "three",
	}).RedactedValue()
	if !errors.Is(err, deploy.ErrInvalidTerraformPlan) {
		t.Fatalf("RedactedValue(invalid) error = %v, want ErrInvalidTerraformPlan", err)
	}
}

func TestValidateTerraformModulePlanRejectsInvalidDescriptors(t *testing.T) {
	err := deploy.ValidateTerraformModulePlan(deploy.TerraformModulePlan{
		Providers: []deploy.TerraformProviderDescriptor{
			deploy.TerraformProvider("google", "hashicorp/google", "~> 5.0"),
			deploy.TerraformProvider("google", "hashicorp/google", "~> 5.1"),
			{Name: "1bad", Source: deploy.TerraformSource("hashicorp/bad")},
		},
		Modules: []deploy.TerraformModuleDescriptor{
			{
				Name:   "service",
				Source: deploy.TerraformSource("./modules/service"),
				Inputs: []deploy.TerraformVariableDescriptor{
					{Name: "name", Value: "api"},
					{Name: "name", Value: "duplicate"},
					{Name: "missing", Required: true},
				},
				Providers: []string{"google", "google"},
				Outputs: []deploy.TerraformOutputDescriptor{
					{Name: "url"},
					{Name: "url"},
				},
			},
			{
				Name: "bad module",
			},
		},
	})
	if !errors.Is(err, deploy.ErrInvalidTerraformPlan) {
		t.Fatalf("ValidateTerraformModulePlan() error = %v, want ErrInvalidTerraformPlan", err)
	}

	for _, fragment := range []string{
		"duplicate",
		"1bad",
		"value is required",
		"providers[1]",
		"source is required",
		"bad module",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateTerraformModulePlan() error = %v, want fragment %q", err, fragment)
		}
	}
}

func TestValidateTerraformModulePlanRequiresWork(t *testing.T) {
	err := deploy.ValidateTerraformModulePlan(deploy.TerraformModulePlan{})
	if !errors.Is(err, deploy.ErrInvalidTerraformPlan) {
		t.Fatalf("ValidateTerraformModulePlan(empty) error = %v, want ErrInvalidTerraformPlan", err)
	}
}
