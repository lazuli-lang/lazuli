package deploy_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestRenderHorizontalPodAutoscalerRendersDeterministically(t *testing.T) {
	hpa := deploy.DeploymentAutoscaler("api", 2, 10,
		deploy.CustomAverageValue("http_requests_per_second", "100"),
		deploy.MemoryAverageValue("512Mi"),
		deploy.CPUUtilization(70),
	)
	hpa.Namespace = "production"

	got, err := deploy.RenderHorizontalPodAutoscaler(hpa)
	if err != nil {
		t.Fatalf("RenderHorizontalPodAutoscaler() error = %v", err)
	}

	want := `apiVersion: "autoscaling/v2"
kind: "HorizontalPodAutoscaler"
metadata:
  name: "api"
  namespace: "production"
spec:
  scaleTargetRef:
    apiVersion: "apps/v1"
    kind: "Deployment"
    name: "api"
  minReplicas: 2
  maxReplicas: 10
  metrics:
    - type: "Resource"
      resource:
        name: "cpu"
        target:
          type: "Utilization"
          averageUtilization: 70
    - type: "Resource"
      resource:
        name: "memory"
        target:
          type: "AverageValue"
          averageValue: "512Mi"
    - type: "Pods"
      pods:
        metric:
          name: "http_requests_per_second"
        target:
          type: "AverageValue"
          averageValue: "100"
`
	if got != want {
		t.Fatalf("RenderHorizontalPodAutoscaler() =\n%s\nwant\n%s", got, want)
	}

	reordered := deploy.DeploymentAutoscaler("api", 2, 10,
		deploy.CPUUtilization(70),
		deploy.CustomAverageValue("http_requests_per_second", "100"),
		deploy.MemoryAverageValue("512Mi"),
	)
	reordered.Namespace = "production"

	gotAgain, err := deploy.RenderHorizontalPodAutoscaler(reordered)
	if err != nil {
		t.Fatalf("RenderHorizontalPodAutoscaler(second) error = %v", err)
	}
	if gotAgain != got {
		t.Fatalf("RenderHorizontalPodAutoscaler(second) =\n%s\nwant deterministic output\n%s", gotAgain, got)
	}
}

func TestRenderHorizontalPodAutoscalerSupportsCustomTarget(t *testing.T) {
	got, err := deploy.HorizontalPodAutoscaler{
		Name:        "api-scaler",
		TargetRef:   deploy.ScaleTarget("example.dev/v1", "WorkerPool", "api-workers"),
		MinReplicas: 1,
		MaxReplicas: 4,
		Metrics: []deploy.AutoscalingMetric{
			deploy.ResourceUtilization("cpu", 80),
		},
	}.Render()
	if err != nil {
		t.Fatalf("Render() error = %v", err)
	}

	for _, fragment := range []string{
		`name: "api-scaler"`,
		`apiVersion: "example.dev/v1"`,
		`kind: "WorkerPool"`,
		`name: "api-workers"`,
	} {
		if !strings.Contains(got, fragment) {
			t.Fatalf("Render() =\n%s\nwant fragment %q", got, fragment)
		}
	}
}

func TestValidateHorizontalPodAutoscalerRejectsInvalidValues(t *testing.T) {
	tests := []struct {
		name   string
		hpa    deploy.HorizontalPodAutoscaler
		fields []string
	}{
		{
			name: "invalid replica range",
			hpa: deploy.DeploymentAutoscaler("api", 3, 2,
				deploy.CPUUtilization(70),
			),
			fields: []string{"max_replicas"},
		},
		{
			name: "missing metrics",
			hpa:  deploy.DeploymentAutoscaler("api", 1, 3),
			fields: []string{
				"metrics",
			},
		},
		{
			name: "invalid cpu target",
			hpa: deploy.DeploymentAutoscaler("api", 1, 3,
				deploy.CPUUtilization(0),
			),
			fields: []string{"average_utilization"},
		},
		{
			name: "invalid target ref",
			hpa: deploy.HorizontalPodAutoscaler{
				Name:        "api",
				TargetRef:   deploy.ScaleTarget("apps/v1", "Deployment", "Bad Target"),
				MinReplicas: 1,
				MaxReplicas: 3,
				Metrics: []deploy.AutoscalingMetric{
					deploy.CPUUtilization(70),
				},
			},
			fields: []string{"target_ref.name"},
		},
		{
			name: "invalid custom metric target",
			hpa: deploy.DeploymentAutoscaler("api", 1, 3,
				deploy.AutoscalingMetric{
					Type: "Pods",
					Pods: &deploy.PodsMetricSource{
						Name: "queue_depth",
						Target: deploy.MetricTarget{
							Type:               "Utilization",
							AverageUtilization: 70,
						},
					},
				},
			),
			fields: []string{"target.type"},
		},
		{
			name: "duplicate metric",
			hpa: deploy.DeploymentAutoscaler("api", 1, 3,
				deploy.MemoryUtilization(75),
				deploy.MemoryUtilization(75),
			),
			fields: []string{"duplicate metric"},
		},
		{
			name: "invalid metadata",
			hpa: deploy.HorizontalPodAutoscaler{
				Name:        "Bad Name",
				Namespace:   "bad namespace",
				MinReplicas: 1,
				MaxReplicas: 3,
				Metrics: []deploy.AutoscalingMetric{
					deploy.CPUUtilization(70),
				},
			},
			fields: []string{"metadata.name", "metadata.namespace", "target_ref.name"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := deploy.ValidateHorizontalPodAutoscaler(tt.hpa)
			if !errors.Is(err, deploy.ErrInvalidAutoscalingConfig) {
				t.Fatalf("ValidateHorizontalPodAutoscaler() error = %v, want ErrInvalidAutoscalingConfig", err)
			}
			for _, field := range tt.fields {
				if !strings.Contains(err.Error(), field) {
					t.Fatalf("ValidateHorizontalPodAutoscaler() error = %v, want field %q", err, field)
				}
			}
		})
	}
}
