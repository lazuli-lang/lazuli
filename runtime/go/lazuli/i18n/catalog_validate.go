package i18n

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

var (
	// ErrInvalidLocaleTag is wrapped when a locale tag fails Lazuli's
	// syntax-only BCP 47 shape checks.
	ErrInvalidLocaleTag = errors.New("lazuli/i18n: invalid locale tag")

	// ErrInvalidCatalogEntry is wrapped when a raw catalog entry has an
	// unusable locale, key, or plural arm shape.
	ErrInvalidCatalogEntry = errors.New("lazuli/i18n: invalid catalog entry")

	// ErrDuplicateCatalogKey is wrapped when the same locale/key pair appears
	// more than once in a raw catalog entry list.
	ErrDuplicateCatalogKey = errors.New("lazuli/i18n: duplicate catalog key")

	// ErrCatalogPlaceholderMismatch is wrapped when translations for the same
	// key do not use the same {name} placeholder set across locales.
	ErrCatalogPlaceholderMismatch = errors.New("lazuli/i18n: catalog placeholder mismatch")

	// ErrInvalidPluralArm is wrapped when a plural message uses an unknown or
	// repeated plural category.
	ErrInvalidPluralArm = errors.New("lazuli/i18n: invalid plural arm")
)

// CatalogMessage is one raw, non-plural translation entry before a catalog is
// folded into a locale-to-key map.
type CatalogMessage struct {
	Locale  string
	Key     string
	Message string
}

// CatalogPluralMessage is one raw plural translation entry before plural arms
// are folded into a category map.
type CatalogPluralMessage struct {
	Locale string
	Key    string
	Arms   []CatalogPluralArm
}

// CatalogPluralArm is one category-specific plural translation.
type CatalogPluralArm struct {
	Category PluralCategory
	Message  string
}

// ValidateLocaleTags checks every locale tag with ValidateLocaleTag.
func ValidateLocaleTags(tags []string) error {
	var errs []error
	for i, tag := range tags {
		if err := ValidateLocaleTag(tag); err != nil {
			errs = append(errs, fmt.Errorf("locale[%d]: %w", i, err))
		}
	}
	return errors.Join(errs...)
}

// ValidateLocaleTag performs syntax-only checks for the locale tags Lazuli
// accepts in generated catalogs. It intentionally does not consult CLDR or the
// IANA language subtag registry.
func ValidateLocaleTag(tag string) error {
	if tag == "" {
		return fmt.Errorf("%w: value is required", ErrInvalidLocaleTag)
	}
	if strings.TrimSpace(tag) != tag {
		return fmt.Errorf("%w: %q must be trimmed", ErrInvalidLocaleTag, tag)
	}
	if strings.Contains(tag, "_") {
		return fmt.Errorf("%w: %q must use hyphen separators", ErrInvalidLocaleTag, tag)
	}
	if !validLocaleTag(tag) {
		return fmt.Errorf("%w: %q must be a well-formed BCP 47-like tag", ErrInvalidLocaleTag, tag)
	}
	return nil
}

// ValidateCatalogMessages checks raw non-plural catalog entries for valid
// locale tags, usable keys, duplicate locale/key pairs, and placeholder-set
// consistency across locales for each key.
func ValidateCatalogMessages(messages []CatalogMessage) error {
	checked := make([]catalogMessageCheck, 0, len(messages))
	seen := make(map[string]int, len(messages))

	var errs []error
	for i, message := range messages {
		locale, key, shapeErr := validateCatalogEntryShape("message", i, message.Locale, message.Key)
		if shapeErr != nil {
			errs = append(errs, shapeErr)
		}
		if locale != "" && key != "" {
			duplicateKey := catalogDuplicateKey(locale, key)
			if first, ok := seen[duplicateKey]; ok {
				errs = append(errs, fmt.Errorf("%w: message[%d] locale %q key %q also appears at message[%d]", ErrDuplicateCatalogKey, i, locale, key, first))
			} else {
				seen[duplicateKey] = i
			}
		}

		placeholders, err := catalogMessagePlaceholders(key, message.Message)
		if err != nil {
			errs = append(errs, fmt.Errorf("message[%d]: %w", i, err))
			continue
		}
		if locale != "" && key != "" {
			checked = append(checked, catalogMessageCheck{
				Index:        i,
				Locale:       locale,
				Key:          key,
				Placeholders: placeholders,
			})
		}
	}

	errs = append(errs, validateCatalogPlaceholderConsistency(checked)...)
	return errors.Join(errs...)
}

// ValidateCatalogPluralMessages checks raw plural catalog entries for valid
// locale tags, usable keys, duplicate locale/key pairs, known plural
// categories, repeated arms, required arms for Lazuli's fallback plural
// profiles, and placeholder consistency per key/category across locales.
func ValidateCatalogPluralMessages(messages []CatalogPluralMessage) error {
	checked := make([]catalogMessageCheck, 0)
	seen := make(map[string]int, len(messages))

	var errs []error
	for i, message := range messages {
		locale, key, shapeErr := validateCatalogEntryShape("plural", i, message.Locale, message.Key)
		if shapeErr != nil {
			errs = append(errs, shapeErr)
		}
		if locale != "" && key != "" {
			duplicateKey := catalogDuplicateKey(locale, key)
			if first, ok := seen[duplicateKey]; ok {
				errs = append(errs, fmt.Errorf("%w: plural[%d] locale %q key %q also appears at plural[%d]", ErrDuplicateCatalogKey, i, locale, key, first))
			} else {
				seen[duplicateKey] = i
			}
		}

		armMessages := make(map[PluralCategory]string, len(message.Arms))
		seenArms := make(map[PluralCategory]int, len(message.Arms))
		for armIndex, arm := range message.Arms {
			field := fmt.Sprintf("plural[%d].arms[%d]", i, armIndex)
			if !validPluralCategory(arm.Category) {
				errs = append(errs, fmt.Errorf("%w: %s.category %q is unknown", ErrInvalidPluralArm, field, arm.Category))
				continue
			}
			if first, ok := seenArms[arm.Category]; ok {
				errs = append(errs, fmt.Errorf("%w: %s.category %q also appears at plural[%d].arms[%d]", ErrInvalidPluralArm, field, arm.Category, i, first))
				continue
			}
			seenArms[arm.Category] = armIndex
			armMessages[arm.Category] = arm.Message

			placeholders, err := catalogMessagePlaceholders(key, arm.Message)
			if err != nil {
				errs = append(errs, fmt.Errorf("%s: %w", field, err))
				continue
			}
			if locale != "" && key != "" {
				checked = append(checked, catalogMessageCheck{
					Index:        i,
					ArmIndex:     armIndex,
					Locale:       locale,
					Key:          catalogPluralPlaceholderKey(key, arm.Category),
					Placeholders: placeholders,
				})
			}
		}

		if locale != "" {
			if err := ValidatePluralMessages(locale, armMessages); err != nil {
				errs = append(errs, fmt.Errorf("plural[%d]: %w", i, err))
			}
		}
	}

	errs = append(errs, validateCatalogPlaceholderConsistency(checked)...)
	return errors.Join(errs...)
}

type catalogMessageCheck struct {
	Index        int
	ArmIndex     int
	Locale       string
	Key          string
	Placeholders []string
}

func validateCatalogEntryShape(kind string, index int, locale, key string) (string, string, error) {
	cleanLocale := strings.TrimSpace(locale)
	cleanKey := strings.TrimSpace(key)

	var errs []error
	field := fmt.Sprintf("%s[%d]", kind, index)
	if err := ValidateLocaleTag(locale); err != nil {
		errs = append(errs, fmt.Errorf("%s.locale: %w", field, err))
	}
	if key == "" {
		errs = append(errs, fmt.Errorf("%w: %s.key is required", ErrInvalidCatalogEntry, field))
	} else if cleanKey != key {
		errs = append(errs, fmt.Errorf("%w: %s.key %q must be trimmed", ErrInvalidCatalogEntry, field, key))
	} else if catalogHasControl(key) {
		errs = append(errs, fmt.Errorf("%w: %s.key %q contains control characters", ErrInvalidCatalogEntry, field, key))
	}

	if err := errors.Join(errs...); err != nil {
		return cleanLocale, cleanKey, err
	}
	return cleanLocale, cleanKey, nil
}

func validateCatalogPlaceholderConsistency(messages []catalogMessageCheck) []error {
	byKey := make(map[string]catalogMessageCheck)
	errs := make([]error, 0)

	for _, message := range messages {
		reference, ok := byKey[message.Key]
		if !ok {
			byKey[message.Key] = message
			continue
		}
		if sameStrings(reference.Placeholders, message.Placeholders) {
			continue
		}

		errs = append(errs, fmt.Errorf(
			"%w: key %q locale %q placeholders %s differ from locale %q placeholders %s",
			ErrCatalogPlaceholderMismatch,
			message.Key,
			message.Locale,
			catalogPlaceholderList(message.Placeholders),
			reference.Locale,
			catalogPlaceholderList(reference.Placeholders),
		))
	}

	return errs
}

func catalogMessagePlaceholders(key, source string) ([]string, error) {
	names := make([]string, 0)
	seen := make(map[string]struct{})

	for i := 0; i < len(source); {
		switch source[i] {
		case '{':
			if i+1 < len(source) && source[i+1] == '{' {
				i += 2
				continue
			}

			closeOffset := strings.IndexByte(source[i+1:], '}')
			if closeOffset == -1 {
				return nil, &InvalidMessageFormatError{
					Key:    key,
					Offset: i,
					Reason: "unclosed placeholder",
				}
			}

			end := i + 1 + closeOffset
			name := strings.TrimSpace(source[i+1 : end])
			if name == "" {
				return nil, &InvalidMessageFormatError{
					Key:    key,
					Offset: i,
					Reason: "empty placeholder",
				}
			}
			if strings.Contains(name, "{") {
				return nil, &InvalidMessageFormatError{
					Key:    key,
					Offset: i,
					Reason: "nested opening brace",
				}
			}
			if _, ok := seen[name]; !ok {
				seen[name] = struct{}{}
				names = append(names, name)
			}
			i = end + 1
		case '}':
			if i+1 < len(source) && source[i+1] == '}' {
				i += 2
				continue
			}

			return nil, &InvalidMessageFormatError{
				Key:    key,
				Offset: i,
				Reason: "unescaped closing brace",
			}
		default:
			next := strings.IndexAny(source[i:], "{}")
			if next == -1 {
				i = len(source)
				continue
			}
			i += next
		}
	}

	sort.Strings(names)
	return names, nil
}

func validLocaleTag(tag string) bool {
	subtags := strings.Split(tag, "-")
	if len(subtags) == 0 || !isAlphaSubtag(subtags[0], 2, 8) {
		return false
	}

	i := 1
	for extlangCount := 0; i < len(subtags) && extlangCount < 3 && isAlphaSubtag(subtags[i], 3, 3); extlangCount++ {
		i++
	}
	if i < len(subtags) && isAlphaSubtag(subtags[i], 4, 4) {
		i++
	}
	if i < len(subtags) && (isAlphaSubtag(subtags[i], 2, 2) || isDigitSubtag(subtags[i], 3, 3)) {
		i++
	}

	for i < len(subtags) {
		subtag := subtags[i]
		if strings.EqualFold(subtag, "x") {
			i++
			if i == len(subtags) {
				return false
			}
			for ; i < len(subtags); i++ {
				if !isAlnumSubtag(subtags[i], 1, 8) {
					return false
				}
			}
			return true
		}
		if isAlnumSubtag(subtag, 1, 1) {
			i++
			if i == len(subtags) {
				return false
			}
			for i < len(subtags) && isAlnumSubtag(subtags[i], 2, 8) {
				i++
			}
			continue
		}
		if isVariantSubtag(subtag) {
			i++
			continue
		}
		return false
	}

	return true
}

func isVariantSubtag(value string) bool {
	if isAlnumSubtag(value, 5, 8) {
		return true
	}
	return len(value) == 4 && isASCIIDigit(value[0]) && isAlnumSubtag(value, 4, 4)
}

func isAlphaSubtag(value string, minLen, maxLen int) bool {
	if len(value) < minLen || len(value) > maxLen {
		return false
	}
	for i := 0; i < len(value); i++ {
		if !isASCIIAlpha(value[i]) {
			return false
		}
	}
	return true
}

func isDigitSubtag(value string, minLen, maxLen int) bool {
	if len(value) < minLen || len(value) > maxLen {
		return false
	}
	for i := 0; i < len(value); i++ {
		if !isASCIIDigit(value[i]) {
			return false
		}
	}
	return true
}

func isAlnumSubtag(value string, minLen, maxLen int) bool {
	if len(value) < minLen || len(value) > maxLen {
		return false
	}
	for i := 0; i < len(value); i++ {
		if !isASCIIAlpha(value[i]) && !isASCIIDigit(value[i]) {
			return false
		}
	}
	return true
}

func isASCIIAlpha(b byte) bool {
	return (b >= 'a' && b <= 'z') || (b >= 'A' && b <= 'Z')
}

func isASCIIDigit(b byte) bool {
	return b >= '0' && b <= '9'
}

func validPluralCategory(category PluralCategory) bool {
	switch category {
	case PluralZero, PluralOne, PluralTwo, PluralFew, PluralMany, PluralOther:
		return true
	default:
		return false
	}
}

func catalogDuplicateKey(locale, key string) string {
	return strings.ToLower(locale) + "\x00" + key
}

func catalogPluralPlaceholderKey(key string, category PluralCategory) string {
	return key + "#" + string(category)
}

func sameStrings(left, right []string) bool {
	if len(left) != len(right) {
		return false
	}
	for i := range left {
		if left[i] != right[i] {
			return false
		}
	}
	return true
}

func catalogPlaceholderList(names []string) string {
	if len(names) == 0 {
		return "none"
	}
	return strings.Join(quotedStrings(names), ", ")
}

func catalogHasControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}
