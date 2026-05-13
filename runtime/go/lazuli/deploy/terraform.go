package deploy

import (
	"errors"
	"fmt"
	"math"
	"sort"
	"strconv"
	"strings"
	"unicode"
)

const (
	// DefaultTerraformSensitiveMask is used in rendered previews and summaries
	// for sensitive Terraform variable values.
	DefaultTerraformSensitiveMask = "[REDACTED]"
)

var (
	// ErrInvalidTerraformPlan reports invalid Terraform module planning input.
	ErrInvalidTerraformPlan = errors.New("lazuli/deploy: invalid terraform plan")
)

// TerraformValueType describes a Terraform input value shape for validation and
// template metadata.
type TerraformValueType string

const (
	TerraformValueAny    TerraformValueType = "any"
	TerraformValueString TerraformValueType = "string"
	TerraformValueNumber TerraformValueType = "number"
	TerraformValueBool   TerraformValueType = "bool"
	TerraformValueList   TerraformValueType = "list"
	TerraformValueMap    TerraformValueType = "map"
)

// TerraformSourceDescriptor identifies a provider or module source address.
type TerraformSourceDescriptor struct {
	Address string
}

// TerraformVersionDescriptor identifies a provider or module version
// constraint. Empty is allowed for local modules.
type TerraformVersionDescriptor struct {
	Constraint string
}

// TerraformVariableDescriptor describes one provider or module input.
type TerraformVariableDescriptor struct {
	Name        string
	Type        TerraformValueType
	Value       any
	Required    bool
	Sensitive   bool
	Description string
}

// TerraformOutputDescriptor describes one module output reference to expose in
// a rendered plan preview.
type TerraformOutputDescriptor struct {
	Name        string
	Value       string
	Description string
	Sensitive   bool
}

// TerraformProviderDescriptor describes a required provider and optional
// provider block inputs.
type TerraformProviderDescriptor struct {
	Name    string
	Source  TerraformSourceDescriptor
	Version TerraformVersionDescriptor
	Alias   string
	Inputs  []TerraformVariableDescriptor
}

// TerraformModuleDescriptor describes a Terraform module block, its input
// variables, provider aliases, and outputs to expose.
type TerraformModuleDescriptor struct {
	Name      string
	Source    TerraformSourceDescriptor
	Version   TerraformVersionDescriptor
	Providers []string
	Inputs    []TerraformVariableDescriptor
	Outputs   []TerraformOutputDescriptor
}

// TerraformModulePlan groups provider and module descriptors for deterministic
// validation, rendering, and summaries. It does not execute Terraform.
type TerraformModulePlan struct {
	Providers []TerraformProviderDescriptor
	Modules   []TerraformModuleDescriptor
}

// TerraformPlanSummary is safe for logs and diagnostics.
type TerraformPlanSummary struct {
	Providers []TerraformProviderSummary
	Modules   []TerraformModuleSummary
}

// TerraformProviderSummary is a redacted provider summary.
type TerraformProviderSummary struct {
	Name    string
	Source  string
	Version string
	Alias   string
	Inputs  map[string]string
}

// TerraformModuleSummary is a redacted module summary.
type TerraformModuleSummary struct {
	Name        string
	Source      string
	Version     string
	Providers   []string
	Inputs      map[string]string
	Outputs     []string
	OutputCount int
}

// TerraformSource returns a source descriptor.
func TerraformSource(address string) TerraformSourceDescriptor {
	return TerraformSourceDescriptor{Address: address}
}

// TerraformVersion returns a version constraint descriptor.
func TerraformVersion(constraint string) TerraformVersionDescriptor {
	return TerraformVersionDescriptor{Constraint: constraint}
}

// TerraformInput returns a module or provider input descriptor.
func TerraformInput(name string, value any) TerraformVariableDescriptor {
	return TerraformVariableDescriptor{Name: name, Value: value}
}

// TerraformSensitiveInput returns a sensitive module or provider input
// descriptor.
func TerraformSensitiveInput(name string, value any) TerraformVariableDescriptor {
	return TerraformVariableDescriptor{Name: name, Value: value, Sensitive: true}
}

// TerraformProvider returns a provider descriptor.
func TerraformProvider(name, source, version string, inputs ...TerraformVariableDescriptor) TerraformProviderDescriptor {
	return TerraformProviderDescriptor{
		Name:    name,
		Source:  TerraformSource(source),
		Version: TerraformVersion(version),
		Inputs:  append([]TerraformVariableDescriptor(nil), inputs...),
	}
}

// TerraformModule returns a module descriptor.
func TerraformModule(name, source, version string, inputs ...TerraformVariableDescriptor) TerraformModuleDescriptor {
	return TerraformModuleDescriptor{
		Name:    name,
		Source:  TerraformSource(source),
		Version: TerraformVersion(version),
		Inputs:  append([]TerraformVariableDescriptor(nil), inputs...),
	}
}

// ValidateTerraformModulePlan validates Terraform provider and module planning
// descriptors.
func ValidateTerraformModulePlan(plan TerraformModulePlan) error {
	_, err := normalizeTerraformModulePlan(plan)
	return err
}

// RenderTerraformModulePlan renders a deterministic HCL-like preview suitable
// for templates. Sensitive inputs are masked and no Terraform command is run.
func RenderTerraformModulePlan(plan TerraformModulePlan) (string, error) {
	normalized, err := normalizeTerraformModulePlan(plan)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	if len(normalized.Providers) > 0 {
		b.WriteString("terraform {\n")
		b.WriteString("  required_providers {\n")
		for _, provider := range normalized.Providers {
			b.WriteString("    ")
			b.WriteString(provider.Name)
			b.WriteString(" = {\n")
			b.WriteString("      source = ")
			b.WriteString(renderTerraformString(provider.Source.Address))
			b.WriteByte('\n')
			if provider.Version.Constraint != "" {
				b.WriteString("      version = ")
				b.WriteString(renderTerraformString(provider.Version.Constraint))
				b.WriteByte('\n')
			}
			b.WriteString("    }\n")
		}
		b.WriteString("  }\n")
		b.WriteString("}\n\n")
	}

	for _, provider := range normalized.Providers {
		b.WriteString("provider ")
		b.WriteString(renderTerraformString(provider.Name))
		b.WriteString(" {\n")
		if provider.Alias != "" {
			b.WriteString("  alias = ")
			b.WriteString(renderTerraformString(provider.Alias))
			b.WriteByte('\n')
		}
		renderTerraformInputs(&b, provider.Inputs, "  ")
		b.WriteString("}\n\n")
	}

	for i, module := range normalized.Modules {
		if i > 0 {
			b.WriteByte('\n')
		}
		b.WriteString("module ")
		b.WriteString(renderTerraformString(module.Name))
		b.WriteString(" {\n")
		b.WriteString("  source = ")
		b.WriteString(renderTerraformString(module.Source.Address))
		b.WriteByte('\n')
		if module.Version.Constraint != "" {
			b.WriteString("  version = ")
			b.WriteString(renderTerraformString(module.Version.Constraint))
			b.WriteByte('\n')
		}
		if len(module.Providers) > 0 {
			b.WriteString("  providers = {\n")
			for _, provider := range module.Providers {
				b.WriteString("    ")
				b.WriteString(provider)
				b.WriteString(" = ")
				b.WriteString(provider)
				b.WriteByte('\n')
			}
			b.WriteString("  }\n")
		}
		renderTerraformInputs(&b, module.Inputs, "  ")
		b.WriteString("}\n")

		for _, output := range module.Outputs {
			b.WriteByte('\n')
			b.WriteString("output ")
			b.WriteString(renderTerraformString(module.Name + "_" + output.Name))
			b.WriteString(" {\n")
			b.WriteString("  value = ")
			if output.Value == "" {
				b.WriteString("module.")
				b.WriteString(module.Name)
				b.WriteByte('.')
				b.WriteString(output.Name)
			} else {
				b.WriteString(output.Value)
			}
			b.WriteByte('\n')
			if output.Description != "" {
				b.WriteString("  description = ")
				b.WriteString(renderTerraformString(output.Description))
				b.WriteByte('\n')
			}
			if output.Sensitive {
				b.WriteString("  sensitive = true\n")
			}
			b.WriteString("}\n")
		}
	}

	return b.String(), nil
}

// SafeSummary returns a deterministic redacted summary for logs and dry-run
// diagnostics.
func (p TerraformModulePlan) SafeSummary() (TerraformPlanSummary, error) {
	normalized, err := normalizeTerraformModulePlan(p)
	if err != nil {
		return TerraformPlanSummary{}, err
	}

	summary := TerraformPlanSummary{
		Providers: make([]TerraformProviderSummary, 0, len(normalized.Providers)),
		Modules:   make([]TerraformModuleSummary, 0, len(normalized.Modules)),
	}
	for _, provider := range normalized.Providers {
		summary.Providers = append(summary.Providers, TerraformProviderSummary{
			Name:    provider.Name,
			Source:  provider.Source.Address,
			Version: provider.Version.Constraint,
			Alias:   provider.Alias,
			Inputs:  summarizeTerraformInputs(provider.Inputs),
		})
	}
	for _, module := range normalized.Modules {
		outputs := make([]string, 0, len(module.Outputs))
		for _, output := range module.Outputs {
			outputs = append(outputs, output.Name)
		}
		summary.Modules = append(summary.Modules, TerraformModuleSummary{
			Name:        module.Name,
			Source:      module.Source.Address,
			Version:     module.Version.Constraint,
			Providers:   append([]string(nil), module.Providers...),
			Inputs:      summarizeTerraformInputs(module.Inputs),
			Outputs:     outputs,
			OutputCount: len(outputs),
		})
	}
	return summary, nil
}

// RedactedValue returns a deterministic redacted representation of a variable
// descriptor value.
func (v TerraformVariableDescriptor) RedactedValue() (string, error) {
	normalized, errs := normalizeTerraformVariable(v, "variable")
	if err := errors.Join(errs...); err != nil {
		return "", fmt.Errorf("%w: %v", ErrInvalidTerraformPlan, err)
	}
	return renderTerraformRedactedValue(normalized), nil
}

func renderTerraformInputs(b *strings.Builder, inputs []TerraformVariableDescriptor, indent string) {
	for _, input := range inputs {
		b.WriteString(indent)
		b.WriteString(input.Name)
		b.WriteString(" = ")
		if input.Sensitive {
			b.WriteString("sensitive(")
			b.WriteString(renderTerraformString(DefaultTerraformSensitiveMask))
			b.WriteByte(')')
		} else {
			b.WriteString(renderTerraformValue(input.Value))
		}
		b.WriteByte('\n')
	}
}

func summarizeTerraformInputs(inputs []TerraformVariableDescriptor) map[string]string {
	summary := make(map[string]string, len(inputs))
	for _, input := range inputs {
		summary[input.Name] = renderTerraformRedactedValue(input)
	}
	return summary
}

func renderTerraformRedactedValue(input TerraformVariableDescriptor) string {
	if input.Sensitive {
		return DefaultTerraformSensitiveMask
	}
	return renderTerraformValue(input.Value)
}

func normalizeTerraformModulePlan(plan TerraformModulePlan) (TerraformModulePlan, error) {
	normalized := TerraformModulePlan{
		Providers: make([]TerraformProviderDescriptor, 0, len(plan.Providers)),
		Modules:   make([]TerraformModuleDescriptor, 0, len(plan.Modules)),
	}
	var errs []error

	providerNames := make(map[string]struct{}, len(plan.Providers))
	for i, provider := range plan.Providers {
		item, itemErrs := normalizeTerraformProvider(provider, fmt.Sprintf("providers[%d]", i))
		errs = append(errs, itemErrs...)
		if item.Name != "" {
			key := item.Name
			if item.Alias != "" {
				key += "." + item.Alias
			}
			if _, ok := providerNames[key]; ok {
				errs = append(errs, fmt.Errorf("providers[%d].name duplicate %q", i, key))
			} else {
				providerNames[key] = struct{}{}
			}
		}
		normalized.Providers = append(normalized.Providers, item)
	}

	moduleNames := make(map[string]struct{}, len(plan.Modules))
	for i, module := range plan.Modules {
		item, itemErrs := normalizeTerraformModule(module, fmt.Sprintf("modules[%d]", i))
		errs = append(errs, itemErrs...)
		if item.Name != "" {
			if _, ok := moduleNames[item.Name]; ok {
				errs = append(errs, fmt.Errorf("modules[%d].name duplicate %q", i, item.Name))
			} else {
				moduleNames[item.Name] = struct{}{}
			}
		}
		normalized.Modules = append(normalized.Modules, item)
	}

	if len(normalized.Providers) == 0 && len(normalized.Modules) == 0 {
		errs = append(errs, errors.New("at least one provider or module is required"))
	}

	sort.SliceStable(normalized.Providers, func(i, j int) bool {
		left := normalized.Providers[i].Name + "." + normalized.Providers[i].Alias
		right := normalized.Providers[j].Name + "." + normalized.Providers[j].Alias
		return left < right
	})
	sort.SliceStable(normalized.Modules, func(i, j int) bool {
		return normalized.Modules[i].Name < normalized.Modules[j].Name
	})

	if err := errors.Join(errs...); err != nil {
		return TerraformModulePlan{}, fmt.Errorf("%w: %v", ErrInvalidTerraformPlan, err)
	}
	return normalized, nil
}

func normalizeTerraformProvider(provider TerraformProviderDescriptor, field string) (TerraformProviderDescriptor, []error) {
	var errs []error
	out := TerraformProviderDescriptor{
		Name:    strings.TrimSpace(provider.Name),
		Source:  normalizeTerraformSource(provider.Source),
		Version: normalizeTerraformVersion(provider.Version),
		Alias:   strings.TrimSpace(provider.Alias),
	}
	if err := validateTerraformIdentifier(out.Name, field+".name"); err != nil {
		errs = append(errs, err)
	}
	if out.Source.Address == "" {
		errs = append(errs, fmt.Errorf("%s.source is required", field))
	} else if hasTerraformUnsafeRune(out.Source.Address) {
		errs = append(errs, fmt.Errorf("%s.source contains control characters", field))
	}
	if hasTerraformUnsafeRune(out.Version.Constraint) {
		errs = append(errs, fmt.Errorf("%s.version contains control characters", field))
	}
	if out.Alias != "" {
		if err := validateTerraformIdentifier(out.Alias, field+".alias"); err != nil {
			errs = append(errs, err)
		}
	}
	out.Inputs, errs = normalizeTerraformVariables(provider.Inputs, field+".inputs", errs)
	return out, errs
}

func normalizeTerraformModule(module TerraformModuleDescriptor, field string) (TerraformModuleDescriptor, []error) {
	var errs []error
	out := TerraformModuleDescriptor{
		Name:    strings.TrimSpace(module.Name),
		Source:  normalizeTerraformSource(module.Source),
		Version: normalizeTerraformVersion(module.Version),
	}
	if err := validateTerraformIdentifier(out.Name, field+".name"); err != nil {
		errs = append(errs, err)
	}
	if out.Source.Address == "" {
		errs = append(errs, fmt.Errorf("%s.source is required", field))
	} else if hasTerraformUnsafeRune(out.Source.Address) {
		errs = append(errs, fmt.Errorf("%s.source contains control characters", field))
	}
	if hasTerraformUnsafeRune(out.Version.Constraint) {
		errs = append(errs, fmt.Errorf("%s.version contains control characters", field))
	}

	seenProviders := make(map[string]struct{}, len(module.Providers))
	for i, provider := range module.Providers {
		provider = strings.TrimSpace(provider)
		if provider == "" {
			errs = append(errs, fmt.Errorf("%s.providers[%d] is required", field, i))
			continue
		}
		if !validTerraformProviderReference(provider) {
			errs = append(errs, fmt.Errorf("%s.providers[%d] %q is invalid", field, i, provider))
			continue
		}
		if _, ok := seenProviders[provider]; ok {
			errs = append(errs, fmt.Errorf("%s.providers[%d] duplicate %q", field, i, provider))
			continue
		}
		seenProviders[provider] = struct{}{}
		out.Providers = append(out.Providers, provider)
	}
	sort.Strings(out.Providers)

	out.Inputs, errs = normalizeTerraformVariables(module.Inputs, field+".inputs", errs)
	out.Outputs, errs = normalizeTerraformOutputs(module.Outputs, field+".outputs", errs)
	return out, errs
}

func normalizeTerraformVariables(inputs []TerraformVariableDescriptor, field string, errs []error) ([]TerraformVariableDescriptor, []error) {
	out := make([]TerraformVariableDescriptor, 0, len(inputs))
	seen := make(map[string]struct{}, len(inputs))
	for i, input := range inputs {
		item, itemErrs := normalizeTerraformVariable(input, fmt.Sprintf("%s[%d]", field, i))
		errs = append(errs, itemErrs...)
		if item.Name != "" {
			if _, ok := seen[item.Name]; ok {
				errs = append(errs, fmt.Errorf("%s[%d].name duplicate %q", field, i, item.Name))
			} else {
				seen[item.Name] = struct{}{}
			}
		}
		out = append(out, item)
	}
	sort.SliceStable(out, func(i, j int) bool {
		return out[i].Name < out[j].Name
	})
	return out, errs
}

func normalizeTerraformVariable(input TerraformVariableDescriptor, field string) (TerraformVariableDescriptor, []error) {
	var errs []error
	out := TerraformVariableDescriptor{
		Name:        strings.TrimSpace(input.Name),
		Type:        input.Type,
		Value:       normalizeTerraformValue(input.Value),
		Required:    input.Required,
		Sensitive:   input.Sensitive,
		Description: strings.TrimSpace(input.Description),
	}
	if out.Type == "" {
		out.Type = TerraformValueAny
	}
	if err := validateTerraformIdentifier(out.Name, field+".name"); err != nil {
		errs = append(errs, err)
	}
	if !validTerraformValueType(out.Type) {
		errs = append(errs, fmt.Errorf("%s.type %q is invalid", field, out.Type))
	}
	if out.Required && out.Value == nil {
		errs = append(errs, fmt.Errorf("%s.value is required", field))
	}
	if out.Description != "" && hasTerraformUnsafeRune(out.Description) {
		errs = append(errs, fmt.Errorf("%s.description contains control characters", field))
	}
	if err := validateTerraformValueShape(out.Type, out.Value, field+".value"); err != nil {
		errs = append(errs, err)
	}
	return out, errs
}

func normalizeTerraformOutputs(outputs []TerraformOutputDescriptor, field string, errs []error) ([]TerraformOutputDescriptor, []error) {
	out := make([]TerraformOutputDescriptor, 0, len(outputs))
	seen := make(map[string]struct{}, len(outputs))
	for i, output := range outputs {
		itemField := fmt.Sprintf("%s[%d]", field, i)
		item := TerraformOutputDescriptor{
			Name:        strings.TrimSpace(output.Name),
			Value:       strings.TrimSpace(output.Value),
			Description: strings.TrimSpace(output.Description),
			Sensitive:   output.Sensitive,
		}
		if err := validateTerraformIdentifier(item.Name, itemField+".name"); err != nil {
			errs = append(errs, err)
		}
		if item.Value != "" && hasTerraformUnsafeRune(item.Value) {
			errs = append(errs, fmt.Errorf("%s.value contains control characters", itemField))
		}
		if item.Description != "" && hasTerraformUnsafeRune(item.Description) {
			errs = append(errs, fmt.Errorf("%s.description contains control characters", itemField))
		}
		if item.Name != "" {
			if _, ok := seen[item.Name]; ok {
				errs = append(errs, fmt.Errorf("%s.name duplicate %q", itemField, item.Name))
			} else {
				seen[item.Name] = struct{}{}
			}
		}
		out = append(out, item)
	}
	sort.SliceStable(out, func(i, j int) bool {
		return out[i].Name < out[j].Name
	})
	return out, errs
}

func normalizeTerraformSource(source TerraformSourceDescriptor) TerraformSourceDescriptor {
	return TerraformSourceDescriptor{Address: strings.TrimSpace(source.Address)}
}

func normalizeTerraformVersion(version TerraformVersionDescriptor) TerraformVersionDescriptor {
	return TerraformVersionDescriptor{Constraint: strings.TrimSpace(version.Constraint)}
}

func validateTerraformIdentifier(value, field string) error {
	if value == "" {
		return fmt.Errorf("%s is required", field)
	}
	for i, r := range value {
		if i == 0 {
			if !(r == '_' || unicode.IsLetter(r)) {
				return fmt.Errorf("%s %q is invalid", field, value)
			}
			continue
		}
		if !(r == '_' || r == '-' || unicode.IsLetter(r) || unicode.IsDigit(r)) {
			return fmt.Errorf("%s %q is invalid", field, value)
		}
	}
	return nil
}

func validTerraformProviderReference(value string) bool {
	parts := strings.Split(value, ".")
	if len(parts) > 2 {
		return false
	}
	for _, part := range parts {
		if err := validateTerraformIdentifier(part, "provider"); err != nil {
			return false
		}
	}
	return true
}

func validTerraformValueType(valueType TerraformValueType) bool {
	switch valueType {
	case TerraformValueAny, TerraformValueString, TerraformValueNumber, TerraformValueBool, TerraformValueList, TerraformValueMap:
		return true
	default:
		return false
	}
}

func validateTerraformValueShape(valueType TerraformValueType, value any, field string) error {
	if value == nil {
		return nil
	}
	switch valueType {
	case TerraformValueString:
		if _, ok := value.(string); !ok {
			return fmt.Errorf("%s must be a string", field)
		}
	case TerraformValueNumber:
		switch value.(type) {
		case int, int8, int16, int32, int64, uint, uint8, uint16, uint32, uint64, float32, float64:
			return nil
		default:
			return fmt.Errorf("%s must be a number", field)
		}
	case TerraformValueBool:
		if _, ok := value.(bool); !ok {
			return fmt.Errorf("%s must be a bool", field)
		}
	case TerraformValueList:
		if _, ok := value.([]any); !ok {
			return fmt.Errorf("%s must be a list", field)
		}
	case TerraformValueMap:
		if _, ok := value.(map[string]any); !ok {
			return fmt.Errorf("%s must be a map", field)
		}
	}
	return validateTerraformValue(value, field)
}

func validateTerraformValue(value any, field string) error {
	switch typed := value.(type) {
	case nil, string, bool:
		return nil
	case int, int8, int16, int32, int64, uint, uint8, uint16, uint32, uint64:
		return nil
	case float32:
		if math.IsNaN(float64(typed)) || math.IsInf(float64(typed), 0) {
			return fmt.Errorf("%s must be a finite number", field)
		}
		return nil
	case float64:
		if math.IsNaN(typed) || math.IsInf(typed, 0) {
			return fmt.Errorf("%s must be a finite number", field)
		}
		return nil
	case []any:
		for i, item := range typed {
			if err := validateTerraformValue(item, fmt.Sprintf("%s[%d]", field, i)); err != nil {
				return err
			}
		}
		return nil
	case map[string]any:
		for key, item := range typed {
			if strings.TrimSpace(key) == "" {
				return fmt.Errorf("%s contains an empty map key", field)
			}
			if hasTerraformUnsafeRune(key) {
				return fmt.Errorf("%s map key %q contains control characters", field, key)
			}
			if err := validateTerraformValue(item, field+"."+key); err != nil {
				return err
			}
		}
		return nil
	default:
		return fmt.Errorf("%s has unsupported value type %T", field, value)
	}
}

func normalizeTerraformValue(value any) any {
	switch typed := value.(type) {
	case []string:
		out := make([]any, 0, len(typed))
		for _, item := range typed {
			out = append(out, item)
		}
		return out
	case []int:
		out := make([]any, 0, len(typed))
		for _, item := range typed {
			out = append(out, item)
		}
		return out
	case []bool:
		out := make([]any, 0, len(typed))
		for _, item := range typed {
			out = append(out, item)
		}
		return out
	case map[string]string:
		out := make(map[string]any, len(typed))
		for key, item := range typed {
			out[key] = item
		}
		return out
	case map[string]int:
		out := make(map[string]any, len(typed))
		for key, item := range typed {
			out[key] = item
		}
		return out
	case map[string]bool:
		out := make(map[string]any, len(typed))
		for key, item := range typed {
			out[key] = item
		}
		return out
	default:
		return value
	}
}

func renderTerraformValue(value any) string {
	switch typed := value.(type) {
	case nil:
		return "null"
	case string:
		return renderTerraformString(typed)
	case bool:
		if typed {
			return "true"
		}
		return "false"
	case int:
		return strconv.Itoa(typed)
	case int8:
		return strconv.FormatInt(int64(typed), 10)
	case int16:
		return strconv.FormatInt(int64(typed), 10)
	case int32:
		return strconv.FormatInt(int64(typed), 10)
	case int64:
		return strconv.FormatInt(typed, 10)
	case uint:
		return strconv.FormatUint(uint64(typed), 10)
	case uint8:
		return strconv.FormatUint(uint64(typed), 10)
	case uint16:
		return strconv.FormatUint(uint64(typed), 10)
	case uint32:
		return strconv.FormatUint(uint64(typed), 10)
	case uint64:
		return strconv.FormatUint(typed, 10)
	case float32:
		return strconv.FormatFloat(float64(typed), 'f', -1, 32)
	case float64:
		return strconv.FormatFloat(typed, 'f', -1, 64)
	case []any:
		parts := make([]string, 0, len(typed))
		for _, item := range typed {
			parts = append(parts, renderTerraformValue(item))
		}
		return "[" + strings.Join(parts, ", ") + "]"
	case map[string]any:
		keys := make([]string, 0, len(typed))
		for key := range typed {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		parts := make([]string, 0, len(keys))
		for _, key := range keys {
			parts = append(parts, renderTerraformMapKey(key)+" = "+renderTerraformValue(typed[key]))
		}
		return "{ " + strings.Join(parts, ", ") + " }"
	default:
		return renderTerraformString(fmt.Sprint(typed))
	}
}

func renderTerraformMapKey(key string) string {
	if validateTerraformIdentifier(key, "key") == nil {
		return key
	}
	return renderTerraformString(key)
}

func renderTerraformString(value string) string {
	return strconv.Quote(value)
}

func hasTerraformUnsafeRune(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) && r != '\n' && r != '\t' {
			return true
		}
	}
	return false
}
