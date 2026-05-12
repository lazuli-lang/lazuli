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
	defaultHPAAPIVersion         = "autoscaling/v2"
	defaultScaleTargetAPIVersion = "apps/v1"
	defaultScaleTargetKind       = "Deployment"

	metricTypeResource = "Resource"
	metricTypePods     = "Pods"

	metricTargetTypeUtilization  = "Utilization"
	metricTargetTypeAverageValue = "AverageValue"

	resourceMetricCPU    = "cpu"
	resourceMetricMemory = "memory"
)

// ErrInvalidAutoscalingConfig reports an invalid autoscaling manifest or helper
// input.
var ErrInvalidAutoscalingConfig = errors.New("lazuli/deploy: invalid autoscaling config")

// HorizontalPodAutoscaler is the Kubernetes HPA subset Lazuli can render
// without a YAML dependency.
type HorizontalPodAutoscaler struct {
	Name        string
	Namespace   string
	TargetRef   ScaleTargetRef
	MinReplicas int
	MaxReplicas int
	Metrics     []AutoscalingMetric
}

// ScaleTargetRef identifies the Kubernetes workload scaled by an HPA.
type ScaleTargetRef struct {
	APIVersion string
	Kind       string
	Name       string
}

// AutoscalingMetric is one HPA metric source.
type AutoscalingMetric struct {
	Type     string
	Resource *ResourceMetricSource
	Pods     *PodsMetricSource
}

// ResourceMetricSource describes a CPU, memory, or other Kubernetes resource
// metric.
type ResourceMetricSource struct {
	Name   string
	Target MetricTarget
}

// PodsMetricSource describes a custom per-pod metric.
type PodsMetricSource struct {
	Name   string
	Target MetricTarget
}

// MetricTarget describes how a metric value should be evaluated by the HPA.
type MetricTarget struct {
	Type               string
	AverageUtilization int
	AverageValue       string
}

// DeploymentAutoscaler returns an HPA targeting an apps/v1 Deployment with the
// same name as the HPA.
func DeploymentAutoscaler(name string, minReplicas, maxReplicas int, metrics ...AutoscalingMetric) HorizontalPodAutoscaler {
	return HorizontalPodAutoscaler{
		Name:        name,
		TargetRef:   DeploymentTarget(name),
		MinReplicas: minReplicas,
		MaxReplicas: maxReplicas,
		Metrics:     append([]AutoscalingMetric(nil), metrics...),
	}
}

// DeploymentTarget returns a target ref for an apps/v1 Deployment.
func DeploymentTarget(name string) ScaleTargetRef {
	return ScaleTargetRef{
		APIVersion: defaultScaleTargetAPIVersion,
		Kind:       defaultScaleTargetKind,
		Name:       name,
	}
}

// ScaleTarget returns a custom scale target ref.
func ScaleTarget(apiVersion, kind, name string) ScaleTargetRef {
	return ScaleTargetRef{APIVersion: apiVersion, Kind: kind, Name: name}
}

// CPUUtilization returns a CPU resource metric using average utilization.
func CPUUtilization(percent int) AutoscalingMetric {
	return ResourceUtilization(resourceMetricCPU, percent)
}

// MemoryUtilization returns a memory resource metric using average utilization.
func MemoryUtilization(percent int) AutoscalingMetric {
	return ResourceUtilization(resourceMetricMemory, percent)
}

// MemoryAverageValue returns a memory resource metric using an average quantity
// such as 512Mi.
func MemoryAverageValue(quantity string) AutoscalingMetric {
	return ResourceAverageValue(resourceMetricMemory, quantity)
}

// ResourceUtilization returns a resource metric using average utilization.
func ResourceUtilization(name string, percent int) AutoscalingMetric {
	return AutoscalingMetric{
		Type: metricTypeResource,
		Resource: &ResourceMetricSource{
			Name: name,
			Target: MetricTarget{
				Type:               metricTargetTypeUtilization,
				AverageUtilization: percent,
			},
		},
	}
}

// ResourceAverageValue returns a resource metric using an average quantity such
// as 500m, 512Mi, or 100.
func ResourceAverageValue(name, quantity string) AutoscalingMetric {
	return AutoscalingMetric{
		Type: metricTypeResource,
		Resource: &ResourceMetricSource{
			Name: name,
			Target: MetricTarget{
				Type:         metricTargetTypeAverageValue,
				AverageValue: quantity,
			},
		},
	}
}

// CustomAverageValue returns a per-pod custom metric using an average quantity.
func CustomAverageValue(name, quantity string) AutoscalingMetric {
	return AutoscalingMetric{
		Type: metricTypePods,
		Pods: &PodsMetricSource{
			Name: name,
			Target: MetricTarget{
				Type:         metricTargetTypeAverageValue,
				AverageValue: quantity,
			},
		},
	}
}

// Validate checks the HPA config.
func (hpa HorizontalPodAutoscaler) Validate() error {
	return ValidateHorizontalPodAutoscaler(hpa)
}

// Render renders the HPA config as deterministic YAML.
func (hpa HorizontalPodAutoscaler) Render() (string, error) {
	return RenderHorizontalPodAutoscaler(hpa)
}

// ValidateHorizontalPodAutoscaler validates an HPA config after defaults are
// applied.
func ValidateHorizontalPodAutoscaler(hpa HorizontalPodAutoscaler) error {
	_, err := normalizeHorizontalPodAutoscaler(hpa)
	return err
}

// RenderHorizontalPodAutoscaler renders an HPA config as deterministic YAML.
func RenderHorizontalPodAutoscaler(hpa HorizontalPodAutoscaler) (string, error) {
	normalized, err := normalizeHorizontalPodAutoscaler(hpa)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	writeScalar(&b, 0, "apiVersion", defaultHPAAPIVersion)
	writeScalar(&b, 0, "kind", "HorizontalPodAutoscaler")
	b.WriteString("metadata:\n")
	writeScalar(&b, 2, "name", normalized.Name)
	if normalized.Namespace != "" {
		writeScalar(&b, 2, "namespace", normalized.Namespace)
	}
	b.WriteString("spec:\n")
	b.WriteString("  scaleTargetRef:\n")
	writeScalar(&b, 4, "apiVersion", normalized.TargetRef.APIVersion)
	writeScalar(&b, 4, "kind", normalized.TargetRef.Kind)
	writeScalar(&b, 4, "name", normalized.TargetRef.Name)
	writeIntScalar(&b, 2, "minReplicas", normalized.MinReplicas)
	writeIntScalar(&b, 2, "maxReplicas", normalized.MaxReplicas)
	b.WriteString("  metrics:\n")
	for _, metric := range normalized.Metrics {
		b.WriteString("    - type: ")
		b.WriteString(quoteYAML(metric.Type))
		b.WriteByte('\n')
		if metric.Type == metricTypeResource {
			writeResourceMetric(&b, metric.Resource)
		} else {
			writePodsMetric(&b, metric.Pods)
		}
	}
	return b.String(), nil
}

func normalizeHorizontalPodAutoscaler(hpa HorizontalPodAutoscaler) (HorizontalPodAutoscaler, error) {
	var errs []error

	hpa.Name = strings.TrimSpace(hpa.Name)
	hpa.Namespace = strings.TrimSpace(hpa.Namespace)
	hpa.TargetRef.APIVersion = strings.TrimSpace(hpa.TargetRef.APIVersion)
	hpa.TargetRef.Kind = strings.TrimSpace(hpa.TargetRef.Kind)
	hpa.TargetRef.Name = strings.TrimSpace(hpa.TargetRef.Name)
	if hpa.TargetRef.APIVersion == "" {
		hpa.TargetRef.APIVersion = defaultScaleTargetAPIVersion
	}
	if hpa.TargetRef.Kind == "" {
		hpa.TargetRef.Kind = defaultScaleTargetKind
	}
	if hpa.TargetRef.Name == "" {
		hpa.TargetRef.Name = hpa.Name
	}

	errs = append(errs, validateRequiredToken("metadata.name", hpa.Name)...)
	if hpa.Namespace != "" {
		errs = append(errs, validateRequiredToken("metadata.namespace", hpa.Namespace)...)
	}
	errs = append(errs, validateRequiredToken("target_ref.api_version", hpa.TargetRef.APIVersion)...)
	errs = append(errs, validateRequiredToken("target_ref.kind", hpa.TargetRef.Kind)...)
	errs = append(errs, validateRequiredToken("target_ref.name", hpa.TargetRef.Name)...)
	if hpa.MinReplicas < 1 {
		errs = append(errs, invalidAutoscalingConfig("min_replicas", "must be at least 1"))
	}
	if hpa.MaxReplicas < 1 {
		errs = append(errs, invalidAutoscalingConfig("max_replicas", "must be at least 1"))
	} else if hpa.MinReplicas > 0 && hpa.MaxReplicas < hpa.MinReplicas {
		errs = append(errs, invalidAutoscalingConfig("max_replicas", "must be greater than or equal to min_replicas"))
	}
	if len(hpa.Metrics) == 0 {
		errs = append(errs, invalidAutoscalingConfig("metrics", "at least one metric is required"))
	}

	metrics := make([]AutoscalingMetric, 0, len(hpa.Metrics))
	for i, metric := range hpa.Metrics {
		normalized, metricErrs := normalizeAutoscalingMetric(metric, fmt.Sprintf("metrics[%d]", i))
		errs = append(errs, metricErrs...)
		if len(metricErrs) == 0 {
			metrics = append(metrics, normalized)
		}
	}
	sort.SliceStable(metrics, func(i, j int) bool {
		return autoscalingMetricSortKey(metrics[i]) < autoscalingMetricSortKey(metrics[j])
	})
	errs = append(errs, validateUniqueMetrics(metrics)...)
	hpa.Metrics = metrics

	if err := errors.Join(errs...); err != nil {
		return HorizontalPodAutoscaler{}, err
	}
	return hpa, nil
}

func normalizeAutoscalingMetric(metric AutoscalingMetric, field string) (AutoscalingMetric, []error) {
	var errs []error

	metric.Type = strings.TrimSpace(metric.Type)
	sourceCount := 0
	if metric.Resource != nil {
		sourceCount++
	}
	if metric.Pods != nil {
		sourceCount++
	}
	if metric.Type == "" && sourceCount == 1 {
		if metric.Resource != nil {
			metric.Type = metricTypeResource
		} else {
			metric.Type = metricTypePods
		}
	}
	if sourceCount != 1 {
		errs = append(errs, invalidAutoscalingConfig(field, "exactly one metric source is required"))
	}

	switch metric.Type {
	case metricTypeResource:
		if metric.Resource == nil {
			errs = append(errs, invalidAutoscalingConfig(field+".resource", "value is required"))
			return metric, errs
		}
		resource := *metric.Resource
		normalized, resourceErrs := normalizeResourceMetric(resource, field+".resource")
		errs = append(errs, resourceErrs...)
		metric.Resource = &normalized
		metric.Pods = nil
	case metricTypePods:
		if metric.Pods == nil {
			errs = append(errs, invalidAutoscalingConfig(field+".pods", "value is required"))
			return metric, errs
		}
		pods := *metric.Pods
		normalized, podsErrs := normalizePodsMetric(pods, field+".pods")
		errs = append(errs, podsErrs...)
		metric.Pods = &normalized
		metric.Resource = nil
	default:
		errs = append(errs, invalidAutoscalingConfig(field+".type", "must be Resource or Pods"))
	}

	return metric, errs
}

func normalizeResourceMetric(metric ResourceMetricSource, field string) (ResourceMetricSource, []error) {
	var errs []error
	metric.Name = strings.TrimSpace(metric.Name)
	errs = append(errs, validateRequiredToken(field+".name", metric.Name)...)

	target, targetErrs := normalizeMetricTarget(metric.Target, field+".target")
	errs = append(errs, targetErrs...)
	if target.Type != "" && target.Type != metricTargetTypeUtilization && target.Type != metricTargetTypeAverageValue {
		errs = append(errs, invalidAutoscalingConfig(field+".target.type", "resource metrics support Utilization or AverageValue"))
	}
	metric.Target = target
	return metric, errs
}

func normalizePodsMetric(metric PodsMetricSource, field string) (PodsMetricSource, []error) {
	var errs []error
	metric.Name = strings.TrimSpace(metric.Name)
	errs = append(errs, validateRequiredToken(field+".name", metric.Name)...)

	target, targetErrs := normalizeMetricTarget(metric.Target, field+".target")
	errs = append(errs, targetErrs...)
	if target.Type != "" && target.Type != metricTargetTypeAverageValue {
		errs = append(errs, invalidAutoscalingConfig(field+".target.type", "pods metrics support AverageValue"))
	}
	metric.Target = target
	return metric, errs
}

func normalizeMetricTarget(target MetricTarget, field string) (MetricTarget, []error) {
	var errs []error
	target.Type = strings.TrimSpace(target.Type)
	target.AverageValue = strings.TrimSpace(target.AverageValue)
	if target.Type == "" {
		if target.AverageUtilization > 0 {
			target.Type = metricTargetTypeUtilization
		} else if target.AverageValue != "" {
			target.Type = metricTargetTypeAverageValue
		}
	}

	switch target.Type {
	case metricTargetTypeUtilization:
		if target.AverageUtilization < 1 {
			errs = append(errs, invalidAutoscalingConfig(field+".average_utilization", "must be at least 1"))
		}
		if target.AverageValue != "" {
			errs = append(errs, invalidAutoscalingConfig(field, "Utilization target only supports average_utilization"))
		}
	case metricTargetTypeAverageValue:
		errs = append(errs, validateRequiredToken(field+".average_value", target.AverageValue)...)
		if target.AverageUtilization != 0 {
			errs = append(errs, invalidAutoscalingConfig(field, "AverageValue target only supports average_value"))
		}
	default:
		errs = append(errs, invalidAutoscalingConfig(field+".type", "must be Utilization or AverageValue"))
	}
	return target, errs
}

func validateUniqueMetrics(metrics []AutoscalingMetric) []error {
	var errs []error
	seen := make(map[string]struct{}, len(metrics))
	for _, metric := range metrics {
		key := autoscalingMetricSortKey(metric)
		if _, ok := seen[key]; ok {
			errs = append(errs, invalidAutoscalingConfig("metrics", fmt.Sprintf("duplicate metric %q", key)))
			continue
		}
		seen[key] = struct{}{}
	}
	return errs
}

func writeResourceMetric(b *strings.Builder, metric *ResourceMetricSource) {
	b.WriteString("      resource:\n")
	writeScalar(b, 8, "name", metric.Name)
	writeMetricTarget(b, 8, metric.Target)
}

func writePodsMetric(b *strings.Builder, metric *PodsMetricSource) {
	b.WriteString("      pods:\n")
	b.WriteString("        metric:\n")
	writeScalar(b, 10, "name", metric.Name)
	writeMetricTarget(b, 8, metric.Target)
}

func writeMetricTarget(b *strings.Builder, indent int, target MetricTarget) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString("target:\n")
	writeScalar(b, indent+2, "type", target.Type)
	if target.Type == metricTargetTypeUtilization {
		writeIntScalar(b, indent+2, "averageUtilization", target.AverageUtilization)
	} else {
		writeScalar(b, indent+2, "averageValue", target.AverageValue)
	}
}

func writeIntScalar(b *strings.Builder, indent int, key string, value int) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(": ")
	b.WriteString(strconv.Itoa(value))
	b.WriteByte('\n')
}

func autoscalingMetricSortKey(metric AutoscalingMetric) string {
	if metric.Type == metricTypeResource {
		return "0/resource/" + metric.Resource.Name + "/" + metricTargetSortKey(metric.Resource.Target)
	}
	return "1/pods/" + metric.Pods.Name + "/" + metricTargetSortKey(metric.Pods.Target)
}

func metricTargetSortKey(target MetricTarget) string {
	return target.Type + "/" + strconv.Itoa(target.AverageUtilization) + "/" + target.AverageValue
}

func validateRequiredToken(field, value string) []error {
	if value == "" {
		return []error{invalidAutoscalingConfig(field, "value is required")}
	}
	if !safeAutoscalingToken(value) {
		return []error{invalidAutoscalingConfig(field, fmt.Sprintf("invalid value %q", value))}
	}
	return nil
}

func safeAutoscalingToken(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) || r == '"' || r == '\'' || r == '`' {
			return false
		}
	}
	return true
}

func invalidAutoscalingConfig(field, message string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidAutoscalingConfig, field, message)
}
