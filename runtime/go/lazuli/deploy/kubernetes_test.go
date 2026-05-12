package deploy_test

import (
	"errors"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/deploy"
)

func TestRenderKubernetesManifestBuildsAppResources(t *testing.T) {
	configMap := deploy.ConfigMap("api-config", map[string]string{
		"REDIS_URL": "redis://redis:6379/0",
		"APP_ENV":   "production",
	})
	configMap.Namespace = "prod"
	configMap.Labels = map[string]string{
		"tier":                   "backend",
		"app.kubernetes.io/name": "api",
	}

	deployment := deploy.Deployment("api", "ghcr.io/acme/api:1.2.3")
	deployment.Namespace = "prod"
	deployment.Replicas = 2
	deployment.Labels = map[string]string{
		"tier":                   "backend",
		"app.kubernetes.io/name": "api",
	}
	deployment.Annotations = map[string]string{
		"example.com/revision": "abc123",
	}
	deployment.Selector = map[string]string{"app": "api"}
	deployment.PodLabels = map[string]string{
		"tier": "backend",
		"app":  "api",
	}
	deployment.Containers[0].Env = deploy.EnvFromMap(map[string]string{
		"DATABASE_URL": "postgres://lazuli:secret@postgres:5432/lazuli?sslmode=disable",
		"APP_ENV":      "production",
	})
	deployment.Containers[0].EnvFromConfigMaps = []string{"api-config"}
	deployment.Containers[0].Ports = []deploy.KubernetesContainerPort{
		deploy.KubeContainerPort("http", 8080),
	}
	deployment.Containers[0].Resources = deploy.ResourceRequirements("100m", "128Mi", "500m", "512Mi")

	service := deploy.ClusterIPService("api", 80, 8080)
	service.Namespace = "prod"
	service.Labels = map[string]string{
		"tier":                   "backend",
		"app.kubernetes.io/name": "api",
	}
	service.Selector = map[string]string{"app": "api"}
	service.Ports[0].Name = "http"

	got, err := deploy.RenderKubernetesManifest(deploy.NewKubernetesManifest(service, deployment, configMap))
	if err != nil {
		t.Fatalf("RenderKubernetesManifest() error = %v", err)
	}

	want := `apiVersion: "v1"
kind: "ConfigMap"
metadata:
  name: "api-config"
  namespace: "prod"
  labels:
    app.kubernetes.io/name: "api"
    tier: "backend"
data:
  APP_ENV: "production"
  REDIS_URL: "redis://redis:6379/0"
---
apiVersion: "apps/v1"
kind: "Deployment"
metadata:
  name: "api"
  namespace: "prod"
  labels:
    app.kubernetes.io/name: "api"
    tier: "backend"
  annotations:
    example.com/revision: "abc123"
spec:
  replicas: 2
  selector:
    matchLabels:
      app: "api"
  template:
    metadata:
      labels:
        app: "api"
        tier: "backend"
    spec:
      containers:
        - name: "api"
          image: "ghcr.io/acme/api:1.2.3"
          env:
            - name: "APP_ENV"
              value: "production"
            - name: "DATABASE_URL"
              value: "postgres://lazuli:secret@postgres:5432/lazuli?sslmode=disable"
          envFrom:
            - configMapRef:
                name: "api-config"
          ports:
            - name: "http"
              containerPort: 8080
              protocol: "TCP"
          resources:
            requests:
              cpu: "100m"
              memory: "128Mi"
            limits:
              cpu: "500m"
              memory: "512Mi"
---
apiVersion: "v1"
kind: "Service"
metadata:
  name: "api"
  namespace: "prod"
  labels:
    app.kubernetes.io/name: "api"
    tier: "backend"
spec:
  type: "ClusterIP"
  selector:
    app: "api"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
`
	if got != want {
		t.Fatalf("RenderKubernetesManifest() =\n%s\nwant\n%s", got, want)
	}
}

func TestValidateKubernetesManifestRejectsInvalidValues(t *testing.T) {
	err := deploy.ValidateKubernetesManifest(deploy.KubernetesManifest{
		ConfigMaps: []deploy.KubernetesConfigMap{
			{
				Name: "bad_config",
				Data: map[string]string{"bad key": "value"},
			},
			deploy.ConfigMap("settings", nil),
			deploy.ConfigMap("settings", map[string]string{"OK": "1"}),
		},
		Deployments: []deploy.KubernetesDeployment{
			{
				Name:     "api",
				Replicas: -1,
				Selector: map[string]string{"app": "api"},
				PodLabels: map[string]string{
					"app": "worker",
				},
				Containers: []deploy.KubernetesContainer{
					{
						Name:  "api",
						Image: "bad image",
						Env: []deploy.EnvVar{
							deploy.Env("APP_ENV", "production"),
							deploy.Env("APP_ENV", "duplicate"),
						},
						Ports: []deploy.KubernetesContainerPort{
							{Container: 0},
						},
						Resources: deploy.ResourceRequirements("bad cpu", "128Mi", "", ""),
					},
				},
			},
			{
				Name: "worker",
			},
		},
		Services: []deploy.KubernetesService{
			{
				Name: "api",
				Ports: []deploy.KubernetesServicePort{
					{Port: 70000},
				},
			},
			deploy.ClusterIPService("api", 80, 8080),
		},
	})
	if !errors.Is(err, deploy.ErrInvalidKubernetesManifest) {
		t.Fatalf("ValidateKubernetesManifest() error = %v, want ErrInvalidKubernetesManifest", err)
	}
	for _, fragment := range []string{
		"config_maps[0].name",
		"data key",
		"duplicate ConfigMap",
		"deployments[0].replicas",
		"selector app=\"api\" does not match pod label",
		"containers[0].image",
		"duplicate \"APP_ENV\"",
		"ports[0].container",
		"resources.requests.cpu",
		"deployments[1].containers",
		"services[0].ports[0].port",
		"duplicate Service",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateKubernetesManifest() error = %v, want fragment %q", err, fragment)
		}
	}
}
