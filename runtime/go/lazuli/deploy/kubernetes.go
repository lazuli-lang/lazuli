package deploy

import (
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"unicode"
)

const (
	defaultKubernetesAppLabel    = "app"
	defaultKubernetesProtocol    = "TCP"
	defaultKubernetesServiceType = "ClusterIP"
)

// ErrInvalidKubernetesManifest reports an invalid Kubernetes manifest or helper
// input.
var ErrInvalidKubernetesManifest = errors.New("lazuli/deploy: invalid kubernetes manifest")

// KubernetesManifest is the subset of Kubernetes resources Lazuli can generate
// without a YAML dependency.
type KubernetesManifest struct {
	Deployments []KubernetesDeployment
	Services    []KubernetesService
	ConfigMaps  []KubernetesConfigMap
}

// KubernetesResource is implemented by the Kubernetes resource types accepted
// by NewKubernetesManifest.
type KubernetesResource interface {
	kubernetesResource()
}

// KubernetesDeployment describes an apps/v1 Deployment.
type KubernetesDeployment struct {
	Name        string
	Namespace   string
	Labels      map[string]string
	Annotations map[string]string
	Replicas    int
	Selector    map[string]string
	PodLabels   map[string]string
	Containers  []KubernetesContainer
}

// KubernetesContainer describes one Deployment container.
type KubernetesContainer struct {
	Name              string
	Image             string
	Env               []EnvVar
	EnvFromConfigMaps []string
	Ports             []KubernetesContainerPort
	Resources         KubernetesResourceRequirements
}

// KubernetesContainerPort exposes a named container port.
type KubernetesContainerPort struct {
	Name      string
	Container int
	Protocol  string
}

// KubernetesResourceRequirements describes CPU and memory requests and limits.
type KubernetesResourceRequirements struct {
	Requests KubernetesResourceList
	Limits   KubernetesResourceList
}

// KubernetesResourceList describes CPU and memory quantities.
type KubernetesResourceList struct {
	CPU    string
	Memory string
}

// KubernetesService describes a v1 Service.
type KubernetesService struct {
	Name        string
	Namespace   string
	Labels      map[string]string
	Annotations map[string]string
	Type        string
	Selector    map[string]string
	Ports       []KubernetesServicePort
}

// KubernetesServicePort maps a Service port to a numeric target port.
type KubernetesServicePort struct {
	Name       string
	Port       int
	TargetPort int
	Protocol   string
}

// KubernetesConfigMap describes a v1 ConfigMap.
type KubernetesConfigMap struct {
	Name        string
	Namespace   string
	Labels      map[string]string
	Annotations map[string]string
	Data        map[string]string
}

// NewKubernetesManifest returns a manifest with resources sorted at render time.
func NewKubernetesManifest(resources ...KubernetesResource) KubernetesManifest {
	var manifest KubernetesManifest
	for _, resource := range resources {
		switch value := resource.(type) {
		case KubernetesDeployment:
			manifest.Deployments = append(manifest.Deployments, value)
		case *KubernetesDeployment:
			if value != nil {
				manifest.Deployments = append(manifest.Deployments, *value)
			}
		case KubernetesService:
			manifest.Services = append(manifest.Services, value)
		case *KubernetesService:
			if value != nil {
				manifest.Services = append(manifest.Services, *value)
			}
		case KubernetesConfigMap:
			manifest.ConfigMaps = append(manifest.ConfigMaps, value)
		case *KubernetesConfigMap:
			if value != nil {
				manifest.ConfigMaps = append(manifest.ConfigMaps, *value)
			}
		}
	}
	return manifest
}

// Deployment returns a basic single-container Deployment.
func Deployment(name, image string) KubernetesDeployment {
	return KubernetesDeployment{
		Name:       name,
		Containers: []KubernetesContainer{Container(name, image)},
	}
}

// Container returns a Kubernetes container.
func Container(name, image string) KubernetesContainer {
	return KubernetesContainer{Name: name, Image: image}
}

// KubeContainerPort returns a TCP container port.
func KubeContainerPort(name string, container int) KubernetesContainerPort {
	return KubernetesContainerPort{Name: name, Container: container, Protocol: defaultKubernetesProtocol}
}

// ResourceRequirements returns CPU and memory requests and limits.
func ResourceRequirements(requestCPU, requestMemory, limitCPU, limitMemory string) KubernetesResourceRequirements {
	return KubernetesResourceRequirements{
		Requests: KubernetesResourceList{CPU: requestCPU, Memory: requestMemory},
		Limits:   KubernetesResourceList{CPU: limitCPU, Memory: limitMemory},
	}
}

// ClusterIPService returns a basic ClusterIP Service.
func ClusterIPService(name string, port, targetPort int) KubernetesService {
	return KubernetesService{
		Name:  name,
		Type:  defaultKubernetesServiceType,
		Ports: []KubernetesServicePort{KubeServicePort("", port, targetPort)},
	}
}

// KubeServicePort returns a TCP Service port.
func KubeServicePort(name string, port, targetPort int) KubernetesServicePort {
	return KubernetesServicePort{Name: name, Port: port, TargetPort: targetPort, Protocol: defaultKubernetesProtocol}
}

// ConfigMap returns a ConfigMap with copied data.
func ConfigMap(name string, data map[string]string) KubernetesConfigMap {
	return KubernetesConfigMap{Name: name, Data: copyKubernetesStringMap(data)}
}

// Validate checks the Kubernetes manifest.
func (m KubernetesManifest) Validate() error {
	return ValidateKubernetesManifest(m)
}

// Render renders the Kubernetes manifest as deterministic YAML.
func (m KubernetesManifest) Render() (string, error) {
	return RenderKubernetesManifest(m)
}

// ValidateKubernetesManifest validates a KubernetesManifest.
func ValidateKubernetesManifest(manifest KubernetesManifest) error {
	_, err := normalizeKubernetesManifest(manifest)
	return err
}

// RenderKubernetesManifest renders a KubernetesManifest as deterministic YAML.
func RenderKubernetesManifest(manifest KubernetesManifest) (string, error) {
	normalized, err := normalizeKubernetesManifest(manifest)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	first := true
	writeDocument := func(write func(*strings.Builder)) {
		if !first {
			b.WriteString("---\n")
		}
		first = false
		write(&b)
	}

	for _, configMap := range normalized.ConfigMaps {
		writeDocument(func(out *strings.Builder) {
			writeKubernetesConfigMap(out, configMap)
		})
	}
	for _, deployment := range normalized.Deployments {
		writeDocument(func(out *strings.Builder) {
			writeKubernetesDeployment(out, deployment)
		})
	}
	for _, service := range normalized.Services {
		writeDocument(func(out *strings.Builder) {
			writeKubernetesService(out, service)
		})
	}

	return b.String(), nil
}

func (KubernetesDeployment) kubernetesResource() {}
func (KubernetesService) kubernetesResource()    {}
func (KubernetesConfigMap) kubernetesResource()  {}

func normalizeKubernetesManifest(manifest KubernetesManifest) (KubernetesManifest, error) {
	var errs []error
	if len(manifest.Deployments)+len(manifest.Services)+len(manifest.ConfigMaps) == 0 {
		errs = append(errs, invalidKubernetes("resources", "at least one resource is required"))
	}

	normalized := KubernetesManifest{
		Deployments: make([]KubernetesDeployment, 0, len(manifest.Deployments)),
		Services:    make([]KubernetesService, 0, len(manifest.Services)),
		ConfigMaps:  make([]KubernetesConfigMap, 0, len(manifest.ConfigMaps)),
	}
	seen := map[string]struct{}{}

	for i, configMap := range manifest.ConfigMaps {
		field := fmt.Sprintf("config_maps[%d]", i)
		out, configMapErrs := normalizeKubernetesConfigMap(configMap, field)
		errs = append(errs, configMapErrs...)
		errs = append(errs, checkDuplicateKubernetesResource(seen, "ConfigMap", out.Namespace, out.Name, field)...)
		normalized.ConfigMaps = append(normalized.ConfigMaps, out)
	}
	for i, deployment := range manifest.Deployments {
		field := fmt.Sprintf("deployments[%d]", i)
		out, deploymentErrs := normalizeKubernetesDeployment(deployment, field)
		errs = append(errs, deploymentErrs...)
		errs = append(errs, checkDuplicateKubernetesResource(seen, "Deployment", out.Namespace, out.Name, field)...)
		normalized.Deployments = append(normalized.Deployments, out)
	}
	for i, service := range manifest.Services {
		field := fmt.Sprintf("services[%d]", i)
		out, serviceErrs := normalizeKubernetesService(service, field)
		errs = append(errs, serviceErrs...)
		errs = append(errs, checkDuplicateKubernetesResource(seen, "Service", out.Namespace, out.Name, field)...)
		normalized.Services = append(normalized.Services, out)
	}

	if err := errors.Join(errs...); err != nil {
		return KubernetesManifest{}, err
	}

	sort.SliceStable(normalized.ConfigMaps, func(i, j int) bool {
		return lessKubernetesObject(normalized.ConfigMaps[i].Namespace, normalized.ConfigMaps[i].Name, normalized.ConfigMaps[j].Namespace, normalized.ConfigMaps[j].Name)
	})
	sort.SliceStable(normalized.Deployments, func(i, j int) bool {
		return lessKubernetesObject(normalized.Deployments[i].Namespace, normalized.Deployments[i].Name, normalized.Deployments[j].Namespace, normalized.Deployments[j].Name)
	})
	sort.SliceStable(normalized.Services, func(i, j int) bool {
		return lessKubernetesObject(normalized.Services[i].Namespace, normalized.Services[i].Name, normalized.Services[j].Namespace, normalized.Services[j].Name)
	})

	return normalized, nil
}

func normalizeKubernetesConfigMap(configMap KubernetesConfigMap, field string) (KubernetesConfigMap, []error) {
	var errs []error
	configMap.Name = strings.TrimSpace(configMap.Name)
	configMap.Namespace = strings.TrimSpace(configMap.Namespace)

	errs = append(errs, validateKubernetesResourceName(configMap.Name, field+".name")...)
	errs = append(errs, validateKubernetesNamespace(configMap.Namespace, field+".namespace")...)

	labels, labelErrs := normalizeKubernetesLabels(configMap.Labels, field+".labels")
	errs = append(errs, labelErrs...)
	configMap.Labels = labels

	annotations, annotationErrs := normalizeKubernetesAnnotations(configMap.Annotations, field+".annotations")
	errs = append(errs, annotationErrs...)
	configMap.Annotations = annotations

	data, dataErrs := normalizeKubernetesConfigMapData(configMap.Data, field+".data")
	errs = append(errs, dataErrs...)
	configMap.Data = data

	return configMap, errs
}

func normalizeKubernetesDeployment(deployment KubernetesDeployment, field string) (KubernetesDeployment, []error) {
	var errs []error
	deployment.Name = strings.TrimSpace(deployment.Name)
	deployment.Namespace = strings.TrimSpace(deployment.Namespace)

	errs = append(errs, validateKubernetesResourceName(deployment.Name, field+".name")...)
	errs = append(errs, validateKubernetesNamespace(deployment.Namespace, field+".namespace")...)
	if deployment.Replicas < 0 {
		errs = append(errs, invalidKubernetes(field+".replicas", "must be greater than or equal to 0"))
	}
	if deployment.Replicas == 0 {
		deployment.Replicas = 1
	}

	selector := deployment.Selector
	if len(selector) == 0 && deployment.Name != "" {
		selector = map[string]string{defaultKubernetesAppLabel: deployment.Name}
	}
	normalizedSelector, selectorErrs := normalizeKubernetesLabels(selector, field+".selector")
	errs = append(errs, selectorErrs...)
	if len(normalizedSelector) == 0 {
		errs = append(errs, invalidKubernetes(field+".selector", "at least one selector label is required"))
	}
	deployment.Selector = normalizedSelector

	labels, labelErrs := normalizeKubernetesLabels(deployment.Labels, field+".labels")
	errs = append(errs, labelErrs...)
	if len(labels) == 0 {
		labels = copyKubernetesStringMap(normalizedSelector)
	}
	deployment.Labels = labels

	podLabels := deployment.PodLabels
	if len(podLabels) == 0 {
		podLabels = copyKubernetesStringMap(normalizedSelector)
	}
	normalizedPodLabels, podLabelErrs := normalizeKubernetesLabels(podLabels, field+".pod_labels")
	errs = append(errs, podLabelErrs...)
	for key, value := range normalizedSelector {
		if normalizedPodLabels[key] != value {
			errs = append(errs, invalidKubernetes(field+".pod_labels", fmt.Sprintf("selector %s=%q does not match pod label", key, value)))
		}
	}
	deployment.PodLabels = normalizedPodLabels

	annotations, annotationErrs := normalizeKubernetesAnnotations(deployment.Annotations, field+".annotations")
	errs = append(errs, annotationErrs...)
	deployment.Annotations = annotations

	if len(deployment.Containers) == 0 {
		errs = append(errs, invalidKubernetes(field+".containers", "at least one container is required"))
	}
	containers := make([]KubernetesContainer, 0, len(deployment.Containers))
	containerNames := map[string]struct{}{}
	for i, container := range deployment.Containers {
		itemField := fmt.Sprintf("%s.containers[%d]", field, i)
		normalized, containerErrs := normalizeKubernetesContainer(container, itemField)
		errs = append(errs, containerErrs...)
		if normalized.Name != "" {
			if _, ok := containerNames[normalized.Name]; ok {
				errs = append(errs, invalidKubernetes(itemField+".name", fmt.Sprintf("duplicate container %q", normalized.Name)))
			}
			containerNames[normalized.Name] = struct{}{}
		}
		containers = append(containers, normalized)
	}
	sort.SliceStable(containers, func(i, j int) bool {
		return containers[i].Name < containers[j].Name
	})
	deployment.Containers = containers

	return deployment, errs
}

func normalizeKubernetesContainer(container KubernetesContainer, field string) (KubernetesContainer, []error) {
	var errs []error
	container.Name = strings.TrimSpace(container.Name)
	container.Image = strings.TrimSpace(container.Image)

	if !validKubernetesDNSLabel(container.Name) {
		errs = append(errs, invalidKubernetes(field+".name", fmt.Sprintf("invalid container name %q", container.Name)))
	}
	if !safeImageRef(container.Image) {
		errs = append(errs, invalidKubernetes(field+".image", "must be a non-empty image reference without unsafe characters"))
	}

	env, err := normalizeEnv(container.Env, field+".env")
	if err != nil {
		errs = append(errs, invalidKubernetes(field+".env", err.Error()))
	}
	container.Env = env

	envFrom, envFromErrs := normalizeKubernetesNameList(container.EnvFromConfigMaps, field+".env_from_config_maps")
	errs = append(errs, envFromErrs...)
	container.EnvFromConfigMaps = envFrom

	ports, portErrs := normalizeKubernetesContainerPorts(container.Ports, field+".ports")
	errs = append(errs, portErrs...)
	container.Ports = ports

	resources, resourceErrs := normalizeKubernetesResources(container.Resources, field+".resources")
	errs = append(errs, resourceErrs...)
	container.Resources = resources

	return container, errs
}

func normalizeKubernetesService(service KubernetesService, field string) (KubernetesService, []error) {
	var errs []error
	service.Name = strings.TrimSpace(service.Name)
	service.Namespace = strings.TrimSpace(service.Namespace)
	service.Type = strings.TrimSpace(service.Type)
	if service.Type == "" {
		service.Type = defaultKubernetesServiceType
	}

	errs = append(errs, validateKubernetesResourceName(service.Name, field+".name")...)
	errs = append(errs, validateKubernetesNamespace(service.Namespace, field+".namespace")...)
	if !validKubernetesServiceType(service.Type) {
		errs = append(errs, invalidKubernetes(field+".type", "must be ClusterIP, NodePort, or LoadBalancer"))
	}

	selector := service.Selector
	if len(selector) == 0 && service.Name != "" {
		selector = map[string]string{defaultKubernetesAppLabel: service.Name}
	}
	normalizedSelector, selectorErrs := normalizeKubernetesLabels(selector, field+".selector")
	errs = append(errs, selectorErrs...)
	if len(normalizedSelector) == 0 {
		errs = append(errs, invalidKubernetes(field+".selector", "at least one selector label is required"))
	}
	service.Selector = normalizedSelector

	labels, labelErrs := normalizeKubernetesLabels(service.Labels, field+".labels")
	errs = append(errs, labelErrs...)
	if len(labels) == 0 {
		labels = copyKubernetesStringMap(normalizedSelector)
	}
	service.Labels = labels

	annotations, annotationErrs := normalizeKubernetesAnnotations(service.Annotations, field+".annotations")
	errs = append(errs, annotationErrs...)
	service.Annotations = annotations

	if len(service.Ports) == 0 {
		errs = append(errs, invalidKubernetes(field+".ports", "at least one port is required"))
	}
	ports, portErrs := normalizeKubernetesServicePorts(service.Ports, field+".ports")
	errs = append(errs, portErrs...)
	service.Ports = ports

	return service, errs
}

func checkDuplicateKubernetesResource(seen map[string]struct{}, kind, namespace, name, field string) []error {
	if name == "" {
		return nil
	}
	key := kind + "\x00" + namespace + "\x00" + name
	if _, ok := seen[key]; ok {
		return []error{invalidKubernetes(field+".name", fmt.Sprintf("duplicate %s %q", kind, name))}
	}
	seen[key] = struct{}{}
	return nil
}

func lessKubernetesObject(leftNamespace, leftName, rightNamespace, rightName string) bool {
	if leftNamespace != rightNamespace {
		return leftNamespace < rightNamespace
	}
	return leftName < rightName
}

func normalizeKubernetesLabels(values map[string]string, field string) (map[string]string, []error) {
	var errs []error
	out := make(map[string]string, len(values))
	for _, rawKey := range sortedMapKeys(values) {
		key := strings.TrimSpace(rawKey)
		value := values[rawKey]
		itemField := field + "." + rawKey
		if !validKubernetesLabelKey(key) {
			errs = append(errs, invalidKubernetes(itemField, "label key is invalid"))
			continue
		}
		if !validKubernetesLabelValue(value) {
			errs = append(errs, invalidKubernetes(itemField, fmt.Sprintf("label value %q is invalid", value)))
			continue
		}
		if _, ok := out[key]; ok {
			errs = append(errs, invalidKubernetes(itemField, fmt.Sprintf("duplicate label key %q", key)))
			continue
		}
		out[key] = value
	}
	return out, errs
}

func normalizeKubernetesAnnotations(values map[string]string, field string) (map[string]string, []error) {
	var errs []error
	out := make(map[string]string, len(values))
	for _, rawKey := range sortedMapKeys(values) {
		key := strings.TrimSpace(rawKey)
		value := values[rawKey]
		itemField := field + "." + rawKey
		if !validKubernetesLabelKey(key) {
			errs = append(errs, invalidKubernetes(itemField, "annotation key is invalid"))
			continue
		}
		if hasControlRune(value) {
			errs = append(errs, invalidKubernetes(itemField, "annotation value cannot contain control characters"))
			continue
		}
		if _, ok := out[key]; ok {
			errs = append(errs, invalidKubernetes(itemField, fmt.Sprintf("duplicate annotation key %q", key)))
			continue
		}
		out[key] = value
	}
	return out, errs
}

func normalizeKubernetesConfigMapData(values map[string]string, field string) (map[string]string, []error) {
	var errs []error
	out := make(map[string]string, len(values))
	for _, rawKey := range sortedMapKeys(values) {
		key := strings.TrimSpace(rawKey)
		itemField := field + "." + rawKey
		if !validKubernetesDataKey(key) {
			errs = append(errs, invalidKubernetes(itemField, "data key must contain only letters, digits, '.', '_', or '-'"))
			continue
		}
		if _, ok := out[key]; ok {
			errs = append(errs, invalidKubernetes(itemField, fmt.Sprintf("duplicate data key %q", key)))
			continue
		}
		out[key] = values[rawKey]
	}
	return out, errs
}

func normalizeKubernetesNameList(values []string, field string) ([]string, []error) {
	var errs []error
	out := make([]string, 0, len(values))
	seen := map[string]struct{}{}
	for i, value := range values {
		value = strings.TrimSpace(value)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if !validKubernetesName(value) {
			errs = append(errs, invalidKubernetes(itemField, fmt.Sprintf("invalid name %q", value)))
			continue
		}
		if _, ok := seen[value]; ok {
			errs = append(errs, invalidKubernetes(itemField, fmt.Sprintf("duplicate value %q", value)))
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out, errs
}

func normalizeKubernetesContainerPorts(ports []KubernetesContainerPort, field string) ([]KubernetesContainerPort, []error) {
	var errs []error
	out := make([]KubernetesContainerPort, 0, len(ports))
	seenPorts := map[string]struct{}{}
	seenNames := map[string]struct{}{}
	for i, port := range ports {
		itemField := fmt.Sprintf("%s[%d]", field, i)
		port.Name = strings.TrimSpace(port.Name)
		port.Protocol = normalizeKubernetesProtocol(port.Protocol)
		if port.Container < 1 || port.Container > 65535 {
			errs = append(errs, invalidKubernetes(itemField+".container", "must be between 1 and 65535"))
			continue
		}
		if !validKubernetesProtocol(port.Protocol) {
			errs = append(errs, invalidKubernetes(itemField+".protocol", "must be TCP, UDP, or SCTP"))
			continue
		}
		if port.Name != "" {
			if !validKubernetesPortName(port.Name) {
				errs = append(errs, invalidKubernetes(itemField+".name", fmt.Sprintf("invalid port name %q", port.Name)))
				continue
			}
			if _, ok := seenNames[port.Name]; ok {
				errs = append(errs, invalidKubernetes(itemField+".name", fmt.Sprintf("duplicate port name %q", port.Name)))
				continue
			}
			seenNames[port.Name] = struct{}{}
		}
		key := strconv.Itoa(port.Container) + "/" + port.Protocol
		if _, ok := seenPorts[key]; ok {
			errs = append(errs, invalidKubernetes(itemField, fmt.Sprintf("duplicate port %q", key)))
			continue
		}
		seenPorts[key] = struct{}{}
		out = append(out, port)
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Container != out[j].Container {
			return out[i].Container < out[j].Container
		}
		if out[i].Protocol != out[j].Protocol {
			return out[i].Protocol < out[j].Protocol
		}
		return out[i].Name < out[j].Name
	})
	return out, errs
}

func normalizeKubernetesServicePorts(ports []KubernetesServicePort, field string) ([]KubernetesServicePort, []error) {
	var errs []error
	out := make([]KubernetesServicePort, 0, len(ports))
	seenPorts := map[string]struct{}{}
	seenNames := map[string]struct{}{}
	for i, port := range ports {
		itemField := fmt.Sprintf("%s[%d]", field, i)
		port.Name = strings.TrimSpace(port.Name)
		port.Protocol = normalizeKubernetesProtocol(port.Protocol)
		if port.TargetPort == 0 {
			port.TargetPort = port.Port
		}
		if port.Port < 1 || port.Port > 65535 {
			errs = append(errs, invalidKubernetes(itemField+".port", "must be between 1 and 65535"))
			continue
		}
		if port.TargetPort < 1 || port.TargetPort > 65535 {
			errs = append(errs, invalidKubernetes(itemField+".target_port", "must be between 1 and 65535"))
			continue
		}
		if !validKubernetesProtocol(port.Protocol) {
			errs = append(errs, invalidKubernetes(itemField+".protocol", "must be TCP, UDP, or SCTP"))
			continue
		}
		if len(ports) > 1 && port.Name == "" {
			errs = append(errs, invalidKubernetes(itemField+".name", "name is required when a service has multiple ports"))
			continue
		}
		if port.Name != "" {
			if !validKubernetesPortName(port.Name) {
				errs = append(errs, invalidKubernetes(itemField+".name", fmt.Sprintf("invalid port name %q", port.Name)))
				continue
			}
			if _, ok := seenNames[port.Name]; ok {
				errs = append(errs, invalidKubernetes(itemField+".name", fmt.Sprintf("duplicate port name %q", port.Name)))
				continue
			}
			seenNames[port.Name] = struct{}{}
		}
		key := strconv.Itoa(port.Port) + "/" + port.Protocol
		if _, ok := seenPorts[key]; ok {
			errs = append(errs, invalidKubernetes(itemField, fmt.Sprintf("duplicate port %q", key)))
			continue
		}
		seenPorts[key] = struct{}{}
		out = append(out, port)
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Port != out[j].Port {
			return out[i].Port < out[j].Port
		}
		if out[i].TargetPort != out[j].TargetPort {
			return out[i].TargetPort < out[j].TargetPort
		}
		if out[i].Protocol != out[j].Protocol {
			return out[i].Protocol < out[j].Protocol
		}
		return out[i].Name < out[j].Name
	})
	return out, errs
}

func normalizeKubernetesResources(resources KubernetesResourceRequirements, field string) (KubernetesResourceRequirements, []error) {
	var errs []error
	resources.Requests.CPU = strings.TrimSpace(resources.Requests.CPU)
	resources.Requests.Memory = strings.TrimSpace(resources.Requests.Memory)
	resources.Limits.CPU = strings.TrimSpace(resources.Limits.CPU)
	resources.Limits.Memory = strings.TrimSpace(resources.Limits.Memory)

	for _, quantity := range []struct {
		name  string
		value string
	}{
		{name: "requests.cpu", value: resources.Requests.CPU},
		{name: "requests.memory", value: resources.Requests.Memory},
		{name: "limits.cpu", value: resources.Limits.CPU},
		{name: "limits.memory", value: resources.Limits.Memory},
	} {
		if quantity.value != "" && !validKubernetesQuantity(quantity.value) {
			errs = append(errs, invalidKubernetes(field+"."+quantity.name, "quantity cannot contain whitespace, quotes, or control characters"))
		}
	}
	return resources, errs
}

func validateKubernetesResourceName(name, field string) []error {
	if !validKubernetesName(name) {
		return []error{invalidKubernetes(field, fmt.Sprintf("invalid resource name %q", name))}
	}
	return nil
}

func validateKubernetesNamespace(namespace, field string) []error {
	if namespace != "" && !validKubernetesDNSLabel(namespace) {
		return []error{invalidKubernetes(field, fmt.Sprintf("invalid namespace %q", namespace))}
	}
	return nil
}

func writeKubernetesConfigMap(b *strings.Builder, configMap KubernetesConfigMap) {
	writeScalar(b, 0, "apiVersion", "v1")
	writeScalar(b, 0, "kind", "ConfigMap")
	writeKubernetesMetadata(b, configMap.Name, configMap.Namespace, configMap.Labels, configMap.Annotations)
	if len(configMap.Data) == 0 {
		b.WriteString("data: {}\n")
		return
	}
	writeKubernetesStringMap(b, 0, "data", configMap.Data)
}

func writeKubernetesDeployment(b *strings.Builder, deployment KubernetesDeployment) {
	writeScalar(b, 0, "apiVersion", "apps/v1")
	writeScalar(b, 0, "kind", "Deployment")
	writeKubernetesMetadata(b, deployment.Name, deployment.Namespace, deployment.Labels, deployment.Annotations)
	b.WriteString("spec:\n")
	writeInt(b, 2, "replicas", deployment.Replicas)
	b.WriteString("  selector:\n")
	writeKubernetesStringMap(b, 4, "matchLabels", deployment.Selector)
	b.WriteString("  template:\n")
	b.WriteString("    metadata:\n")
	writeKubernetesStringMap(b, 6, "labels", deployment.PodLabels)
	b.WriteString("    spec:\n")
	b.WriteString("      containers:\n")
	for _, container := range deployment.Containers {
		b.WriteString("        - ")
		b.WriteString("name: ")
		b.WriteString(quoteYAML(container.Name))
		b.WriteByte('\n')
		writeScalar(b, 10, "image", container.Image)
		if len(container.Env) > 0 {
			b.WriteString("          env:\n")
			for _, env := range container.Env {
				b.WriteString("            - name: ")
				b.WriteString(quoteYAML(env.Name))
				b.WriteByte('\n')
				b.WriteString("              value: ")
				b.WriteString(quoteYAML(env.Value))
				b.WriteByte('\n')
			}
		}
		if len(container.EnvFromConfigMaps) > 0 {
			b.WriteString("          envFrom:\n")
			for _, name := range container.EnvFromConfigMaps {
				b.WriteString("            - configMapRef:\n")
				b.WriteString("                name: ")
				b.WriteString(quoteYAML(name))
				b.WriteByte('\n')
			}
		}
		if len(container.Ports) > 0 {
			b.WriteString("          ports:\n")
			for _, port := range container.Ports {
				b.WriteString("            - ")
				if port.Name != "" {
					b.WriteString("name: ")
					b.WriteString(quoteYAML(port.Name))
					b.WriteByte('\n')
					writeInt(b, 14, "containerPort", port.Container)
				} else {
					b.WriteString("containerPort: ")
					b.WriteString(strconv.Itoa(port.Container))
					b.WriteByte('\n')
				}
				writeScalar(b, 14, "protocol", port.Protocol)
			}
		}
		writeKubernetesResources(b, 10, container.Resources)
	}
}

func writeKubernetesService(b *strings.Builder, service KubernetesService) {
	writeScalar(b, 0, "apiVersion", "v1")
	writeScalar(b, 0, "kind", "Service")
	writeKubernetesMetadata(b, service.Name, service.Namespace, service.Labels, service.Annotations)
	b.WriteString("spec:\n")
	writeScalar(b, 2, "type", service.Type)
	writeKubernetesStringMap(b, 2, "selector", service.Selector)
	b.WriteString("  ports:\n")
	for _, port := range service.Ports {
		b.WriteString("    - ")
		if port.Name != "" {
			b.WriteString("name: ")
			b.WriteString(quoteYAML(port.Name))
			b.WriteByte('\n')
			writeInt(b, 6, "port", port.Port)
		} else {
			b.WriteString("port: ")
			b.WriteString(strconv.Itoa(port.Port))
			b.WriteByte('\n')
		}
		writeInt(b, 6, "targetPort", port.TargetPort)
		writeScalar(b, 6, "protocol", port.Protocol)
	}
}

func writeKubernetesMetadata(b *strings.Builder, name, namespace string, labels, annotations map[string]string) {
	b.WriteString("metadata:\n")
	writeScalar(b, 2, "name", name)
	if namespace != "" {
		writeScalar(b, 2, "namespace", namespace)
	}
	writeKubernetesStringMap(b, 2, "labels", labels)
	writeKubernetesStringMap(b, 2, "annotations", annotations)
}

func writeKubernetesResources(b *strings.Builder, indent int, resources KubernetesResourceRequirements) {
	if emptyKubernetesResourceList(resources.Requests) && emptyKubernetesResourceList(resources.Limits) {
		return
	}
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString("resources:\n")
	writeKubernetesResourceList(b, indent+2, "requests", resources.Requests)
	writeKubernetesResourceList(b, indent+2, "limits", resources.Limits)
}

func writeKubernetesResourceList(b *strings.Builder, indent int, key string, resources KubernetesResourceList) {
	if emptyKubernetesResourceList(resources) {
		return
	}
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(":\n")
	if resources.CPU != "" {
		writeScalar(b, indent+2, "cpu", resources.CPU)
	}
	if resources.Memory != "" {
		writeScalar(b, indent+2, "memory", resources.Memory)
	}
}

func writeKubernetesStringMap(b *strings.Builder, indent int, key string, values map[string]string) {
	if len(values) == 0 {
		return
	}
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(":\n")
	for _, mapKey := range sortedMapKeys(values) {
		b.WriteString(strings.Repeat(" ", indent+2))
		b.WriteString(mapKey)
		b.WriteString(": ")
		b.WriteString(quoteYAML(values[mapKey]))
		b.WriteByte('\n')
	}
}

func writeInt(b *strings.Builder, indent int, key string, value int) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(": ")
	b.WriteString(strconv.Itoa(value))
	b.WriteByte('\n')
}

func copyKubernetesStringMap(values map[string]string) map[string]string {
	if len(values) == 0 {
		return nil
	}
	out := make(map[string]string, len(values))
	for key, value := range values {
		out[key] = value
	}
	return out
}

func emptyKubernetesResourceList(resources KubernetesResourceList) bool {
	return resources.CPU == "" && resources.Memory == ""
}

func normalizeKubernetesProtocol(protocol string) string {
	protocol = strings.ToUpper(strings.TrimSpace(protocol))
	if protocol == "" {
		return defaultKubernetesProtocol
	}
	return protocol
}

func validKubernetesName(value string) bool {
	if value == "" || len(value) > 253 {
		return false
	}
	for _, part := range strings.Split(value, ".") {
		if !validKubernetesDNSLabel(part) {
			return false
		}
	}
	return true
}

func validKubernetesDNSLabel(value string) bool {
	if value == "" || len(value) > 63 {
		return false
	}
	for i, r := range value {
		ok := r >= 'a' && r <= 'z' || r >= '0' && r <= '9' || r == '-'
		if !ok {
			return false
		}
		if (i == 0 || i == len(value)-1) && !(r >= 'a' && r <= 'z' || r >= '0' && r <= '9') {
			return false
		}
	}
	return true
}

func validKubernetesLabelKey(value string) bool {
	if value == "" {
		return false
	}
	parts := strings.Split(value, "/")
	if len(parts) > 2 {
		return false
	}
	name := parts[len(parts)-1]
	if !validKubernetesLabelName(name) {
		return false
	}
	if len(parts) == 2 {
		return validKubernetesName(parts[0])
	}
	return true
}

func validKubernetesLabelName(value string) bool {
	if value == "" || len(value) > 63 {
		return false
	}
	for i, r := range value {
		ok := r >= 'a' && r <= 'z' ||
			r >= 'A' && r <= 'Z' ||
			r >= '0' && r <= '9' ||
			r == '-' || r == '_' || r == '.'
		if !ok {
			return false
		}
		if (i == 0 || i == len(value)-1) && !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9') {
			return false
		}
	}
	return true
}

func validKubernetesLabelValue(value string) bool {
	return value == "" || validKubernetesLabelName(value)
}

func validKubernetesDataKey(value string) bool {
	if value == "" {
		return false
	}
	for _, r := range value {
		if !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '.' || r == '_' || r == '-') {
			return false
		}
	}
	return true
}

func validKubernetesPortName(value string) bool {
	if value == "" || len(value) > 15 {
		return false
	}
	hasLetter := false
	for i, r := range value {
		ok := r >= 'a' && r <= 'z' || r >= '0' && r <= '9' || r == '-'
		if !ok {
			return false
		}
		if r >= 'a' && r <= 'z' {
			hasLetter = true
		}
		if (i == 0 || i == len(value)-1) && !(r >= 'a' && r <= 'z' || r >= '0' && r <= '9') {
			return false
		}
	}
	return hasLetter
}

func validKubernetesProtocol(protocol string) bool {
	return protocol == "TCP" || protocol == "UDP" || protocol == "SCTP"
}

func validKubernetesServiceType(value string) bool {
	return value == "ClusterIP" || value == "NodePort" || value == "LoadBalancer"
}

func validKubernetesQuantity(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) || r == '"' || r == '\'' {
			return false
		}
	}
	return true
}

func invalidKubernetes(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidKubernetesManifest, field, detail)
}
