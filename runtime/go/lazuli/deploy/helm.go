package deploy

import (
	"errors"
	"fmt"
	"path"
	"sort"
	"strconv"
	"strings"
	"unicode"
)

const (
	// DefaultHelmChartAPIVersion is the apiVersion used for generated Chart.yaml
	// metadata when no apiVersion is set.
	DefaultHelmChartAPIVersion = "v2"
	// DefaultHelmChartType is the chart type used when no type is set.
	DefaultHelmChartType = "application"

	helmChartFilePath  = "Chart.yaml"
	helmValuesFilePath = "values.yaml"
)

// HelmValueType identifies the expected type for a Helm values entry.
type HelmValueType string

const (
	HelmValueString  HelmValueType = "string"
	HelmValueInteger HelmValueType = "integer"
	HelmValueNumber  HelmValueType = "number"
	HelmValueBoolean HelmValueType = "boolean"
	HelmValueObject  HelmValueType = "object"
	HelmValueArray   HelmValueType = "array"
)

// ErrInvalidHelmChart reports an invalid Helm chart manifest, values schema, or
// template file plan.
var ErrInvalidHelmChart = errors.New("lazuli/deploy: invalid helm chart")

// HelmChartMetadata describes the Chart.yaml fields Lazuli needs for chart
// planning without depending on Helm.
type HelmChartMetadata struct {
	APIVersion  string
	Name        string
	Version     string
	AppVersion  string
	Description string
	Type        string
	KubeVersion string
	Keywords    []string
	Maintainers []HelmChartMaintainer
	Annotations map[string]string
}

// HelmChartMaintainer describes one Chart.yaml maintainer.
type HelmChartMaintainer struct {
	Name  string
	Email string
	URL   string
}

// HelmValuesSchema is a small schema-like description of values.yaml entries.
// Paths are stored without the .Values prefix, for example image.repository.
type HelmValuesSchema struct {
	Fields []HelmValueField
}

// HelmValueField describes one values.yaml entry used by chart templates.
type HelmValueField struct {
	Path        string
	Type        HelmValueType
	Description string
	Default     string
	Required    bool
	Sensitive   bool
	Enum        []string
}

// HelmTemplateFilePlan describes a template file that should exist in a chart.
// It records the Kubernetes kind and values paths the template is expected to
// consume; it does not contain or render Helm Go template source.
type HelmTemplateFilePlan struct {
	Path        string
	APIVersion  string
	Kind        string
	Description string
	Values      []string
}

// HelmChartFilePlan describes a chart file to create or maintain.
type HelmChartFilePlan struct {
	Path        string
	Role        string
	Description string
}

// HelmChartManifest groups chart metadata, values schema, and template file
// plans for validation.
type HelmChartManifest struct {
	Metadata  HelmChartMetadata
	Values    HelmValuesSchema
	Templates []HelmTemplateFilePlan
}

// HelmChart is an alias for HelmChartManifest for callers that use shorter
// chart terminology.
type HelmChart = HelmChartManifest

// ChartMetadata returns Chart.yaml metadata with Helm's v2 application defaults.
func ChartMetadata(name, version string) HelmChartMetadata {
	return HelmChartMetadata{
		APIVersion: DefaultHelmChartAPIVersion,
		Name:       name,
		Version:    version,
		Type:       DefaultHelmChartType,
	}
}

// Maintainer returns one Chart.yaml maintainer.
func Maintainer(name, email, url string) HelmChartMaintainer {
	return HelmChartMaintainer{Name: name, Email: email, URL: url}
}

// ValuesSchema returns a schema-like Helm values descriptor.
func ValuesSchema(fields ...HelmValueField) HelmValuesSchema {
	return HelmValuesSchema{Fields: append([]HelmValueField(nil), fields...)}
}

// HelmValue returns one values.yaml field descriptor.
func HelmValue(path string, valueType HelmValueType) HelmValueField {
	return HelmValueField{Path: path, Type: valueType}
}

// HelmTemplate returns one Helm template file plan.
func HelmTemplate(filePath, apiVersion, kind string, values ...string) HelmTemplateFilePlan {
	return HelmTemplateFilePlan{
		Path:       filePath,
		APIVersion: apiVersion,
		Kind:       kind,
		Values:     append([]string(nil), values...),
	}
}

// NewHelmChartManifest returns a chart manifest plan with copied template
// entries.
func NewHelmChartManifest(metadata HelmChartMetadata, values HelmValuesSchema, templates ...HelmTemplateFilePlan) HelmChartManifest {
	return HelmChartManifest{
		Metadata:  metadata,
		Values:    values,
		Templates: append([]HelmTemplateFilePlan(nil), templates...),
	}
}

// NewHelmChart returns a chart manifest plan.
func NewHelmChart(metadata HelmChartMetadata, values HelmValuesSchema, templates ...HelmTemplateFilePlan) HelmChart {
	return NewHelmChartManifest(metadata, values, templates...)
}

// Validate checks the Helm chart metadata.
func (m HelmChartMetadata) Validate() error {
	return ValidateHelmChartMetadata(m)
}

// Render renders Helm Chart.yaml metadata as deterministic YAML.
func (m HelmChartMetadata) Render() (string, error) {
	return RenderHelmChartMetadata(m)
}

// Validate checks the Helm values schema.
func (s HelmValuesSchema) Validate() error {
	return ValidateHelmValuesSchema(s)
}

// Defaults returns the normalized scalar defaults keyed by values path.
func (s HelmValuesSchema) Defaults() (map[string]string, error) {
	normalized, err := normalizeHelmValuesSchema(s)
	if err != nil {
		return nil, err
	}
	defaults := make(map[string]string, len(normalized.Fields))
	for _, field := range normalized.Fields {
		if field.Default != "" {
			defaults[field.Path] = field.Default
		}
	}
	return defaults, nil
}

// Validate checks the Helm template file plan.
func (p HelmTemplateFilePlan) Validate() error {
	_, errs := normalizeHelmTemplateFilePlan(p, "template")
	if err := errors.Join(errs...); err != nil {
		return err
	}
	return nil
}

// Validate checks the Helm chart manifest plan.
func (m HelmChartManifest) Validate() error {
	return ValidateHelmChartManifest(m)
}

// FilePlan returns the normalized chart file plan.
func (m HelmChartManifest) FilePlan() ([]HelmChartFilePlan, error) {
	return PlanHelmChartFiles(m)
}

// ValidateHelmChartMetadata validates Chart.yaml metadata after defaults are
// applied.
func ValidateHelmChartMetadata(metadata HelmChartMetadata) error {
	_, err := normalizeHelmChartMetadata(metadata)
	return err
}

// RenderHelmChartMetadata renders Chart.yaml metadata as deterministic YAML.
func RenderHelmChartMetadata(metadata HelmChartMetadata) (string, error) {
	normalized, err := normalizeHelmChartMetadata(metadata)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	writeScalar(&b, 0, "apiVersion", normalized.APIVersion)
	writeScalar(&b, 0, "name", normalized.Name)
	if normalized.Description != "" {
		writeScalar(&b, 0, "description", normalized.Description)
	}
	writeScalar(&b, 0, "type", normalized.Type)
	writeScalar(&b, 0, "version", normalized.Version)
	if normalized.AppVersion != "" {
		writeScalar(&b, 0, "appVersion", normalized.AppVersion)
	}
	if normalized.KubeVersion != "" {
		writeScalar(&b, 0, "kubeVersion", normalized.KubeVersion)
	}
	if len(normalized.Keywords) > 0 {
		writeList(&b, 0, "keywords", normalized.Keywords)
	}
	if len(normalized.Maintainers) > 0 {
		b.WriteString("maintainers:\n")
		for _, maintainer := range normalized.Maintainers {
			b.WriteString("  - name: ")
			b.WriteString(quoteYAML(maintainer.Name))
			b.WriteByte('\n')
			if maintainer.Email != "" {
				writeScalar(&b, 4, "email", maintainer.Email)
			}
			if maintainer.URL != "" {
				writeScalar(&b, 4, "url", maintainer.URL)
			}
		}
	}
	if len(normalized.Annotations) > 0 {
		writeKubernetesStringMap(&b, 0, "annotations", normalized.Annotations)
	}
	return b.String(), nil
}

// ValidateHelmValuesSchema validates a Helm values schema-like descriptor.
func ValidateHelmValuesSchema(schema HelmValuesSchema) error {
	_, err := normalizeHelmValuesSchema(schema)
	return err
}

// ValidateHelmTemplateFilePlan validates a single Helm template file plan.
func ValidateHelmTemplateFilePlan(plan HelmTemplateFilePlan) error {
	return plan.Validate()
}

// ValidateHelmChartManifest validates a complete Helm chart manifest plan.
func ValidateHelmChartManifest(manifest HelmChartManifest) error {
	_, err := normalizeHelmChartManifest(manifest)
	return err
}

// ValidateHelmChart validates a complete Helm chart manifest plan.
func ValidateHelmChart(chart HelmChart) error {
	return ValidateHelmChartManifest(chart)
}

// PlanHelmTemplateFiles validates a chart manifest and returns its normalized
// template file plans sorted by path.
func PlanHelmTemplateFiles(manifest HelmChartManifest) ([]HelmTemplateFilePlan, error) {
	normalized, err := normalizeHelmChartManifest(manifest)
	if err != nil {
		return nil, err
	}
	return append([]HelmTemplateFilePlan(nil), normalized.Templates...), nil
}

// PlanHelmChartFiles validates a chart manifest and returns its normalized file
// plan. Template entries contain paths and descriptions only, not rendered
// template source.
func PlanHelmChartFiles(manifest HelmChartManifest) ([]HelmChartFilePlan, error) {
	normalized, err := normalizeHelmChartManifest(manifest)
	if err != nil {
		return nil, err
	}

	files := []HelmChartFilePlan{
		{Path: helmChartFilePath, Role: "metadata", Description: "Helm chart metadata."},
		{Path: helmValuesFilePath, Role: "values", Description: "Default values described by the chart values schema."},
	}
	for _, template := range normalized.Templates {
		description := template.Description
		if description == "" {
			description = template.Kind + " manifest template plan."
		}
		files = append(files, HelmChartFilePlan{
			Path:        template.Path,
			Role:        "template",
			Description: description,
		})
	}
	return files, nil
}

func normalizeHelmChartManifest(manifest HelmChartManifest) (HelmChartManifest, error) {
	var errs []error

	metadata, err := normalizeHelmChartMetadata(manifest.Metadata)
	if err != nil {
		errs = append(errs, err)
	}
	values, err := normalizeHelmValuesSchema(manifest.Values)
	if err != nil {
		errs = append(errs, err)
	}

	if len(manifest.Templates) == 0 {
		errs = append(errs, invalidHelmChart("templates", "at least one template file plan is required"))
	}

	templates := make([]HelmTemplateFilePlan, 0, len(manifest.Templates))
	seenTemplates := map[string]struct{}{}
	knownValues := helmValuesPathSet(values)
	for i, template := range manifest.Templates {
		field := fmt.Sprintf("templates[%d]", i)
		normalized, templateErrs := normalizeHelmTemplateFilePlan(template, field)
		errs = append(errs, templateErrs...)
		if normalized.Path != "" {
			if _, ok := seenTemplates[normalized.Path]; ok {
				errs = append(errs, invalidHelmChart(field+".path", fmt.Sprintf("duplicate template path %q", normalized.Path)))
			}
			seenTemplates[normalized.Path] = struct{}{}
		}
		if len(knownValues) > 0 {
			for _, valuePath := range normalized.Values {
				if _, ok := knownValues[valuePath]; !ok {
					errs = append(errs, invalidHelmChart(field+".values", fmt.Sprintf("unknown values path %q", valuePath)))
				}
			}
		}
		templates = append(templates, normalized)
	}

	if err := errors.Join(errs...); err != nil {
		return HelmChartManifest{}, err
	}
	sort.SliceStable(templates, func(i, j int) bool {
		return templates[i].Path < templates[j].Path
	})
	return HelmChartManifest{Metadata: metadata, Values: values, Templates: templates}, nil
}

func normalizeHelmChartMetadata(metadata HelmChartMetadata) (HelmChartMetadata, error) {
	var errs []error

	metadata.APIVersion = strings.TrimSpace(metadata.APIVersion)
	metadata.Name = strings.TrimSpace(metadata.Name)
	metadata.Version = strings.TrimSpace(metadata.Version)
	metadata.AppVersion = strings.TrimSpace(metadata.AppVersion)
	metadata.Description = strings.TrimSpace(metadata.Description)
	metadata.Type = strings.TrimSpace(metadata.Type)
	metadata.KubeVersion = strings.TrimSpace(metadata.KubeVersion)

	if metadata.APIVersion == "" {
		metadata.APIVersion = DefaultHelmChartAPIVersion
	}
	if metadata.Type == "" {
		metadata.Type = DefaultHelmChartType
	}

	if metadata.APIVersion != "v1" && metadata.APIVersion != "v2" {
		errs = append(errs, invalidHelmChart("metadata.api_version", "must be v1 or v2"))
	}
	if !validHelmChartName(metadata.Name) {
		errs = append(errs, invalidHelmChart("metadata.name", fmt.Sprintf("invalid chart name %q", metadata.Name)))
	}
	if !validHelmSemver(metadata.Version) {
		errs = append(errs, invalidHelmChart("metadata.version", "must be a SemVer version"))
	}
	if metadata.AppVersion != "" && hasControlRune(metadata.AppVersion) {
		errs = append(errs, invalidHelmChart("metadata.app_version", "cannot contain control characters"))
	}
	if metadata.Description != "" && hasControlRune(metadata.Description) {
		errs = append(errs, invalidHelmChart("metadata.description", "cannot contain control characters"))
	}
	if metadata.Type != "application" && metadata.Type != "library" {
		errs = append(errs, invalidHelmChart("metadata.type", "must be application or library"))
	}
	if metadata.KubeVersion != "" && hasControlRune(metadata.KubeVersion) {
		errs = append(errs, invalidHelmChart("metadata.kube_version", "cannot contain control characters"))
	}

	keywords, keywordErrs := normalizeHelmStringList(metadata.Keywords, "metadata.keywords")
	errs = append(errs, keywordErrs...)
	metadata.Keywords = keywords

	maintainers, maintainerErrs := normalizeHelmMaintainers(metadata.Maintainers)
	errs = append(errs, maintainerErrs...)
	metadata.Maintainers = maintainers

	annotations, annotationErrs := normalizeHelmAnnotations(metadata.Annotations, "metadata.annotations")
	errs = append(errs, annotationErrs...)
	metadata.Annotations = annotations

	if err := errors.Join(errs...); err != nil {
		return HelmChartMetadata{}, err
	}
	return metadata, nil
}

func normalizeHelmValuesSchema(schema HelmValuesSchema) (HelmValuesSchema, error) {
	fields := make([]HelmValueField, 0, len(schema.Fields))
	seen := make(map[string]struct{}, len(schema.Fields))
	var errs []error

	for i, field := range schema.Fields {
		itemField := fmt.Sprintf("values.fields[%d]", i)
		normalized, fieldErrs := normalizeHelmValueField(field, itemField)
		errs = append(errs, fieldErrs...)
		if normalized.Path != "" {
			if _, ok := seen[normalized.Path]; ok {
				errs = append(errs, invalidHelmChart(itemField+".path", fmt.Sprintf("duplicate values path %q", normalized.Path)))
			}
			seen[normalized.Path] = struct{}{}
		}
		fields = append(fields, normalized)
	}

	if err := errors.Join(errs...); err != nil {
		return HelmValuesSchema{}, err
	}
	sort.SliceStable(fields, func(i, j int) bool {
		return fields[i].Path < fields[j].Path
	})
	return HelmValuesSchema{Fields: fields}, nil
}

func normalizeHelmValueField(field HelmValueField, fieldName string) (HelmValueField, []error) {
	var errs []error

	field.Path = normalizeHelmValuePath(field.Path)
	field.Type = HelmValueType(strings.TrimSpace(string(field.Type)))
	field.Description = strings.TrimSpace(field.Description)
	field.Default = strings.TrimSpace(field.Default)

	if !validHelmValuePath(field.Path) {
		errs = append(errs, invalidHelmChart(fieldName+".path", fmt.Sprintf("invalid values path %q", field.Path)))
	}
	if !validHelmValueType(field.Type) {
		errs = append(errs, invalidHelmChart(fieldName+".type", "must be string, integer, number, boolean, object, or array"))
	}
	if field.Description != "" && hasControlRune(field.Description) {
		errs = append(errs, invalidHelmChart(fieldName+".description", "cannot contain control characters"))
	}
	if field.Default != "" {
		errs = append(errs, validateHelmValueDefault(fieldName+".default", field.Type, field.Default)...)
	}

	enum, enumErrs := normalizeHelmStringList(field.Enum, fieldName+".enum")
	errs = append(errs, enumErrs...)
	if len(enum) > 0 {
		if field.Type != HelmValueString && field.Type != HelmValueInteger && field.Type != HelmValueNumber && field.Type != HelmValueBoolean {
			errs = append(errs, invalidHelmChart(fieldName+".enum", "enum is only supported for scalar value types"))
		}
		for _, value := range enum {
			errs = append(errs, validateHelmValueDefault(fieldName+".enum", field.Type, value)...)
		}
		if field.Default != "" && !stringListContains(enum, field.Default) {
			errs = append(errs, invalidHelmChart(fieldName+".default", "must be one of enum values"))
		}
	}
	field.Enum = enum

	return field, errs
}

func normalizeHelmTemplateFilePlan(plan HelmTemplateFilePlan, field string) (HelmTemplateFilePlan, []error) {
	var errs []error

	plan.Path = normalizeHelmTemplatePath(plan.Path)
	plan.APIVersion = strings.TrimSpace(plan.APIVersion)
	plan.Kind = strings.TrimSpace(plan.Kind)
	plan.Description = strings.TrimSpace(plan.Description)

	if !validHelmTemplatePath(plan.Path) {
		errs = append(errs, invalidHelmChart(field+".path", fmt.Sprintf("invalid template path %q", plan.Path)))
	}
	if plan.Description != "" && hasControlRune(plan.Description) {
		errs = append(errs, invalidHelmChart(field+".description", "cannot contain control characters"))
	}

	if !helmTemplatePathIsPartial(plan.Path) {
		if plan.APIVersion == "" {
			errs = append(errs, invalidHelmChart(field+".api_version", "value is required for manifest templates"))
		} else if !safeHelmTemplateToken(plan.APIVersion) {
			errs = append(errs, invalidHelmChart(field+".api_version", fmt.Sprintf("invalid apiVersion %q", plan.APIVersion)))
		}
		if plan.Kind == "" {
			errs = append(errs, invalidHelmChart(field+".kind", "value is required for manifest templates"))
		} else if !safeHelmTemplateToken(plan.Kind) {
			errs = append(errs, invalidHelmChart(field+".kind", fmt.Sprintf("invalid kind %q", plan.Kind)))
		}
	}

	values, valueErrs := normalizeHelmValuePathList(plan.Values, field+".values")
	errs = append(errs, valueErrs...)
	plan.Values = values

	return plan, errs
}

func normalizeHelmMaintainers(values []HelmChartMaintainer) ([]HelmChartMaintainer, []error) {
	var errs []error
	out := make([]HelmChartMaintainer, 0, len(values))
	seen := map[string]struct{}{}
	for i, maintainer := range values {
		field := fmt.Sprintf("metadata.maintainers[%d]", i)
		maintainer.Name = strings.TrimSpace(maintainer.Name)
		maintainer.Email = strings.TrimSpace(maintainer.Email)
		maintainer.URL = strings.TrimSpace(maintainer.URL)

		if maintainer.Name == "" {
			errs = append(errs, invalidHelmChart(field+".name", "value is required"))
			continue
		}
		if hasControlRune(maintainer.Name) {
			errs = append(errs, invalidHelmChart(field+".name", "cannot contain control characters"))
			continue
		}
		if maintainer.Email != "" && (!strings.Contains(maintainer.Email, "@") || hasControlRune(maintainer.Email)) {
			errs = append(errs, invalidHelmChart(field+".email", fmt.Sprintf("invalid email %q", maintainer.Email)))
			continue
		}
		if maintainer.URL != "" && hasControlRune(maintainer.URL) {
			errs = append(errs, invalidHelmChart(field+".url", "cannot contain control characters"))
			continue
		}
		key := strings.ToLower(maintainer.Name) + "\x00" + strings.ToLower(maintainer.Email)
		if _, ok := seen[key]; ok {
			errs = append(errs, invalidHelmChart(field+".name", fmt.Sprintf("duplicate maintainer %q", maintainer.Name)))
			continue
		}
		seen[key] = struct{}{}
		out = append(out, maintainer)
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Name != out[j].Name {
			return out[i].Name < out[j].Name
		}
		return out[i].Email < out[j].Email
	})
	return out, errs
}

func normalizeHelmAnnotations(values map[string]string, field string) (map[string]string, []error) {
	var errs []error
	out := make(map[string]string, len(values))
	for _, rawKey := range sortedMapKeys(values) {
		key := strings.TrimSpace(rawKey)
		value := values[rawKey]
		itemField := field + "." + rawKey
		if !safeHelmMapKey(key) {
			errs = append(errs, invalidHelmChart(itemField, "annotation key is invalid"))
			continue
		}
		if hasControlRune(value) {
			errs = append(errs, invalidHelmChart(itemField, "annotation value cannot contain control characters"))
			continue
		}
		if _, ok := out[key]; ok {
			errs = append(errs, invalidHelmChart(itemField, fmt.Sprintf("duplicate annotation key %q", key)))
			continue
		}
		out[key] = value
	}
	return out, errs
}

func normalizeHelmStringList(values []string, field string) ([]string, []error) {
	var errs []error
	out := make([]string, 0, len(values))
	seen := map[string]struct{}{}
	for i, value := range values {
		value = strings.TrimSpace(value)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if value == "" {
			errs = append(errs, invalidHelmChart(itemField, "value is required"))
			continue
		}
		if hasControlRune(value) {
			errs = append(errs, invalidHelmChart(itemField, "cannot contain control characters"))
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out, errs
}

func normalizeHelmValuePathList(values []string, field string) ([]string, []error) {
	var errs []error
	out := make([]string, 0, len(values))
	seen := map[string]struct{}{}
	for i, value := range values {
		value = normalizeHelmValuePath(value)
		itemField := fmt.Sprintf("%s[%d]", field, i)
		if !validHelmValuePath(value) {
			errs = append(errs, invalidHelmChart(itemField, fmt.Sprintf("invalid values path %q", value)))
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out, errs
}

func helmValuesPathSet(schema HelmValuesSchema) map[string]struct{} {
	out := make(map[string]struct{}, len(schema.Fields))
	for _, field := range schema.Fields {
		out[field.Path] = struct{}{}
	}
	return out
}

func normalizeHelmValuePath(value string) string {
	value = strings.TrimSpace(value)
	value = strings.TrimPrefix(value, ".Values.")
	value = strings.TrimPrefix(value, "Values.")
	value = strings.TrimPrefix(value, ".")
	return value
}

func normalizeHelmTemplatePath(value string) string {
	value = strings.TrimSpace(strings.ReplaceAll(value, "\\", "/"))
	if value == "" {
		return ""
	}
	return path.Clean(value)
}

func validateHelmValueDefault(field string, valueType HelmValueType, value string) []error {
	if hasControlRune(value) {
		return []error{invalidHelmChart(field, "cannot contain control characters")}
	}
	switch valueType {
	case HelmValueString:
		return nil
	case HelmValueInteger:
		if _, err := strconv.Atoi(value); err != nil {
			return []error{invalidHelmChart(field, "must be an integer")}
		}
	case HelmValueNumber:
		if _, err := strconv.ParseFloat(value, 64); err != nil {
			return []error{invalidHelmChart(field, "must be a number")}
		}
	case HelmValueBoolean:
		if _, err := strconv.ParseBool(value); err != nil {
			return []error{invalidHelmChart(field, "must be a boolean")}
		}
	case HelmValueObject, HelmValueArray:
		return nil
	}
	return nil
}

func validHelmChartName(value string) bool {
	if value == "" || len(value) > 63 {
		return false
	}
	for i, r := range value {
		switch {
		case r >= 'a' && r <= 'z':
		case r >= '0' && r <= '9':
		case r == '-':
		default:
			return false
		}
		if (i == 0 || i == len(value)-1) && r == '-' {
			return false
		}
	}
	return true
}

func validHelmSemver(value string) bool {
	if value == "" || strings.TrimSpace(value) != value || hasControlRune(value) {
		return false
	}
	buildSplit := strings.Split(value, "+")
	if len(buildSplit) > 2 {
		return false
	}
	if len(buildSplit) == 2 && !validHelmSemverSuffix(buildSplit[1]) {
		return false
	}
	preSplit := strings.Split(buildSplit[0], "-")
	if len(preSplit) > 2 {
		return false
	}
	if len(preSplit) == 2 && !validHelmSemverSuffix(preSplit[1]) {
		return false
	}
	core := strings.Split(preSplit[0], ".")
	if len(core) != 3 {
		return false
	}
	for _, part := range core {
		if !validHelmSemverNumber(part) {
			return false
		}
	}
	return true
}

func validHelmSemverNumber(value string) bool {
	if value == "" {
		return false
	}
	for _, r := range value {
		if r < '0' || r > '9' {
			return false
		}
	}
	return len(value) == 1 || value[0] != '0'
}

func validHelmSemverSuffix(value string) bool {
	if value == "" {
		return false
	}
	for _, part := range strings.Split(value, ".") {
		if part == "" {
			return false
		}
		for _, r := range part {
			ok := (r >= 'a' && r <= 'z') ||
				(r >= 'A' && r <= 'Z') ||
				(r >= '0' && r <= '9') ||
				r == '-'
			if !ok {
				return false
			}
		}
	}
	return true
}

func validHelmValueType(valueType HelmValueType) bool {
	switch valueType {
	case HelmValueString, HelmValueInteger, HelmValueNumber, HelmValueBoolean, HelmValueObject, HelmValueArray:
		return true
	default:
		return false
	}
}

func validHelmValuePath(value string) bool {
	if value == "" || strings.Contains(value, "..") {
		return false
	}
	parts := strings.Split(value, ".")
	for _, part := range parts {
		if !validHelmValuePathPart(part) {
			return false
		}
	}
	return true
}

func validHelmValuePathPart(value string) bool {
	if value == "" {
		return false
	}
	for i, r := range value {
		ok := r == '_' ||
			(r >= 'a' && r <= 'z') ||
			(r >= 'A' && r <= 'Z') ||
			(i > 0 && r >= '0' && r <= '9')
		if !ok {
			return false
		}
	}
	return true
}

func validHelmTemplatePath(value string) bool {
	if value == "" || path.IsAbs(value) || strings.Contains(value, "..") {
		return false
	}
	if !strings.HasPrefix(value, "templates/") {
		return false
	}
	ext := path.Ext(value)
	if ext != ".yaml" && ext != ".yml" && ext != ".tpl" && ext != ".txt" {
		return false
	}
	for _, part := range strings.Split(value, "/") {
		if part == "" || part == "." || part == ".." || strings.ContainsAny(part, "\"'`$;&|<>") {
			return false
		}
	}
	return true
}

func helmTemplatePathIsPartial(value string) bool {
	base := path.Base(value)
	return strings.HasPrefix(base, "_") || strings.HasSuffix(base, ".tpl") || strings.EqualFold(base, "NOTES.txt")
}

func safeHelmTemplateToken(value string) bool {
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

func safeHelmMapKey(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) || r == '=' || r == '"' || r == '\'' {
			return false
		}
	}
	return true
}

func stringListContains(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

func invalidHelmChart(field, message string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidHelmChart, field, message)
}
