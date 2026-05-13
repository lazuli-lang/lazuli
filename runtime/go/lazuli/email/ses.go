package email

import (
	"errors"
	"fmt"
	netmail "net/mail"
	"regexp"
	"sort"
	"strings"
)

var (
	// ErrInvalidAmazonSESDescriptor is wrapped by malformed Amazon SES
	// provider descriptors.
	ErrInvalidAmazonSESDescriptor = errors.New("email: invalid amazon ses descriptor")
)

var amazonSESRegionPattern = regexp.MustCompile(`^[a-z]{2}(-gov)?-[a-z]+-\d+$`)

// AmazonSESDescriptor describes Amazon SES metadata without constructing an
// AWS client or making network calls.
type AmazonSESDescriptor struct {
	Region           string
	IdentityARN      string
	SourceARN        string
	Sender           string
	Sandbox          bool
	ConfigurationSet string
	Tags             []AmazonSESTag
}

// AmazonSESTag is provider metadata attached to a planned SES send.
type AmazonSESTag struct {
	Name  string
	Value string
}

// AmazonSESPlan is a normalized, validated dry-run plan for a future SES send.
type AmazonSESPlan struct {
	Region           string
	IdentityARN      string
	SourceARN        string
	Sender           string
	Sandbox          bool
	ConfigurationSet string
	Tags             []AmazonSESTag
	Summary          AmazonSESRedactedSummary
}

// AmazonSESRedactedSummary is safe to log or expose in diagnostics.
type AmazonSESRedactedSummary struct {
	Provider         string
	Region           string
	IdentityARN      string
	SourceARN        string
	Sender           string
	Sandbox          bool
	ConfigurationSet string
	TagCount         int
}

// Normalize returns a canonical descriptor copy.
func (d AmazonSESDescriptor) Normalize() AmazonSESDescriptor {
	d.Region = strings.ToLower(strings.TrimSpace(d.Region))
	d.IdentityARN = normalizeAmazonSESARNText(d.IdentityARN)
	d.SourceARN = normalizeAmazonSESARNText(d.SourceARN)
	d.Sender = strings.TrimSpace(d.Sender)
	d.ConfigurationSet = strings.TrimSpace(d.ConfigurationSet)
	if d.Tags != nil {
		tags := make([]AmazonSESTag, len(d.Tags))
		for i, tag := range d.Tags {
			tags[i] = tag.Normalize()
		}
		sort.SliceStable(tags, func(i, j int) bool {
			if tags[i].Name == tags[j].Name {
				return tags[i].Value < tags[j].Value
			}
			return tags[i].Name < tags[j].Name
		})
		d.Tags = tags
	}
	return d
}

// Validate checks descriptor shape without contacting Amazon SES.
func (d AmazonSESDescriptor) Validate() error {
	return ValidateAmazonSESDescriptor(d)
}

// Plan returns a normalized, validated dry-run plan.
func (d AmazonSESDescriptor) Plan() (AmazonSESPlan, error) {
	return PlanAmazonSESDescriptor(d)
}

// RedactedSummary returns a normalized diagnostic summary with ARN account
// identifiers and sender local parts redacted.
func (d AmazonSESDescriptor) RedactedSummary() AmazonSESRedactedSummary {
	d = d.Normalize()
	return AmazonSESRedactedSummary{
		Provider:         "amazon_ses",
		Region:           d.Region,
		IdentityARN:      redactAmazonSESARN(d.IdentityARN),
		SourceARN:        redactAmazonSESARN(d.SourceARN),
		Sender:           redactAmazonSESSender(d.Sender),
		Sandbox:          d.Sandbox,
		ConfigurationSet: d.ConfigurationSet,
		TagCount:         len(d.Tags),
	}
}

// Normalize returns a canonical tag copy.
func (t AmazonSESTag) Normalize() AmazonSESTag {
	t.Name = strings.TrimSpace(t.Name)
	t.Value = strings.TrimSpace(t.Value)
	return t
}

// ValidateAmazonSESDescriptor checks Amazon SES descriptor metadata without AWS
// SDK dependencies or network calls.
func ValidateAmazonSESDescriptor(descriptor AmazonSESDescriptor) error {
	descriptor = descriptor.Normalize()
	var errs []error
	if !amazonSESRegionPattern.MatchString(descriptor.Region) {
		errs = append(errs, amazonSESInvalidf("region %q is invalid", descriptor.Region))
	}
	if err := validateAmazonSESARN("identity_arn", descriptor.IdentityARN, descriptor.Region); err != nil {
		errs = append(errs, err)
	}
	if descriptor.SourceARN != "" {
		if err := validateAmazonSESARN("source_arn", descriptor.SourceARN, descriptor.Region); err != nil {
			errs = append(errs, err)
		}
	}
	if err := validateAmazonSESSender(descriptor.Sender); err != nil {
		errs = append(errs, fieldError("sender", err))
	}
	if err := validateAmazonSESConfigurationSet(descriptor.ConfigurationSet); err != nil {
		errs = append(errs, err)
	}
	if err := validateAmazonSESTags(descriptor.Tags); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

// PlanAmazonSESDescriptor normalizes and validates a dry-run SES send plan.
func PlanAmazonSESDescriptor(descriptor AmazonSESDescriptor) (AmazonSESPlan, error) {
	descriptor = descriptor.Normalize()
	if err := ValidateAmazonSESDescriptor(descriptor); err != nil {
		return AmazonSESPlan{}, err
	}
	return AmazonSESPlan{
		Region:           descriptor.Region,
		IdentityARN:      descriptor.IdentityARN,
		SourceARN:        descriptor.SourceARN,
		Sender:           descriptor.Sender,
		Sandbox:          descriptor.Sandbox,
		ConfigurationSet: descriptor.ConfigurationSet,
		Tags:             append([]AmazonSESTag(nil), descriptor.Tags...),
		Summary:          descriptor.RedactedSummary(),
	}, nil
}

func validateAmazonSESSender(sender string) error {
	if strings.TrimSpace(sender) == "" {
		return amazonSESInvalidf("sender is required")
	}
	if strings.TrimSpace(sender) != sender {
		return amazonSESInvalidf("sender has surrounding whitespace")
	}
	if containsControl(sender) {
		return amazonSESInvalidf("sender contains control characters")
	}
	parsed, err := netmail.ParseAddress(sender)
	if err != nil {
		return amazonSESInvalidf("invalid sender %q", sender)
	}
	if parsed.Address == "" {
		return amazonSESInvalidf("sender address is required")
	}
	return nil
}

func validateAmazonSESARN(field, arn, descriptorRegion string) error {
	parts := strings.SplitN(arn, ":", 6)
	if len(parts) != 6 || parts[0] != "arn" {
		return amazonSESInvalidf("%s is invalid", field)
	}
	if parts[1] == "" {
		return amazonSESInvalidf("%s partition is required", field)
	}
	if parts[2] != "ses" {
		return amazonSESInvalidf("%s service must be ses", field)
	}
	if parts[3] == "" {
		return amazonSESInvalidf("%s region is required", field)
	}
	if descriptorRegion != "" && parts[3] != descriptorRegion {
		return amazonSESInvalidf("%s region %q does not match descriptor region %q", field, parts[3], descriptorRegion)
	}
	if parts[4] == "" {
		return amazonSESInvalidf("%s account id is required", field)
	}
	if parts[5] == "" || !strings.HasPrefix(parts[5], "identity/") || strings.TrimPrefix(parts[5], "identity/") == "" {
		return amazonSESInvalidf("%s resource must be identity/<value>", field)
	}
	if containsControl(arn) || containsWhitespace(arn) {
		return amazonSESInvalidf("%s contains control characters or whitespace", field)
	}
	return nil
}

func validateAmazonSESConfigurationSet(configurationSet string) error {
	if configurationSet == "" {
		return nil
	}
	if containsControl(configurationSet) || containsWhitespace(configurationSet) {
		return amazonSESInvalidf("configuration_set contains control characters or whitespace")
	}
	return nil
}

func validateAmazonSESTags(tags []AmazonSESTag) error {
	seen := make(map[string]int, len(tags))
	for i, tag := range tags {
		if tag.Name == "" {
			return amazonSESInvalidf("tags[%d].name is required", i)
		}
		if containsControl(tag.Name) || containsWhitespace(tag.Name) {
			return amazonSESInvalidf("tags[%d].name contains control characters or whitespace", i)
		}
		if containsControl(tag.Value) {
			return amazonSESInvalidf("tags[%d].value contains control characters", i)
		}
		if previous, ok := seen[tag.Name]; ok {
			return amazonSESInvalidf("tags[%d].name duplicates tags[%d].name %q", i, previous, tag.Name)
		}
		seen[tag.Name] = i
	}
	return nil
}

func normalizeAmazonSESARNText(arn string) string {
	arn = strings.TrimSpace(arn)
	parts := strings.SplitN(arn, ":", 6)
	if len(parts) != 6 {
		return arn
	}
	parts[0] = strings.ToLower(parts[0])
	parts[1] = strings.ToLower(parts[1])
	parts[2] = strings.ToLower(parts[2])
	parts[3] = strings.ToLower(parts[3])
	return strings.Join(parts, ":")
}

func redactAmazonSESARN(arn string) string {
	parts := strings.SplitN(arn, ":", 6)
	if len(parts) != 6 {
		return arn
	}
	parts[4] = "REDACTED"
	return strings.Join(parts, ":")
}

func redactAmazonSESSender(sender string) string {
	parsed, err := netmail.ParseAddress(sender)
	if err != nil {
		return sender
	}
	at := strings.LastIndex(parsed.Address, "@")
	if at <= 0 {
		return sender
	}
	redactedAddress := "***@" + strings.ToLower(parsed.Address[at+1:])
	if parsed.Name == "" {
		return redactedAddress
	}
	return (&netmail.Address{Name: parsed.Name, Address: redactedAddress}).String()
}

func amazonSESInvalidf(format string, args ...any) error {
	return fmt.Errorf("%w: "+format, append([]any{ErrInvalidAmazonSESDescriptor}, args...)...)
}
