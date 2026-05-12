package i18n

import (
	"errors"
	"fmt"
	"strings"
)

// ErrMessageFormatInvalid is wrapped when a message format string contains an
// unmatched brace or empty placeholder.
var ErrMessageFormatInvalid = errors.New("lazuli/i18n: invalid message format")

// InvalidMessageFormatError describes invalid brace syntax in a message.
type InvalidMessageFormatError struct {
	Key    string
	Offset int
	Reason string
}

func (e *InvalidMessageFormatError) Error() string {
	if e.Key == "" {
		return fmt.Sprintf("%s: byte %d: %s", ErrMessageFormatInvalid, e.Offset, e.Reason)
	}
	return fmt.Sprintf("%s: key %q byte %d: %s", ErrMessageFormatInvalid, e.Key, e.Offset, e.Reason)
}

// Unwrap returns the sentinel error for errors.Is checks.
func (e *InvalidMessageFormatError) Unwrap() error {
	return ErrMessageFormatInvalid
}

// MissingMessageVariablesError describes one or more placeholders that could
// not be replaced while formatting a message.
type MissingMessageVariablesError struct {
	Key   string
	Names []string
}

func (e *MissingMessageVariablesError) Error() string {
	if len(e.Names) == 1 {
		if e.Key == "" {
			return fmt.Sprintf("%s: variable %q", ErrMessageVariableMissing, e.Names[0])
		}
		return fmt.Sprintf("%s: key %q variable %q", ErrMessageVariableMissing, e.Key, e.Names[0])
	}

	names := strings.Join(quotedStrings(e.Names), ", ")
	if e.Key == "" {
		return fmt.Sprintf("%s: variables %s", ErrMessageVariableMissing, names)
	}
	return fmt.Sprintf("%s: key %q variables %s", ErrMessageVariableMissing, e.Key, names)
}

// Unwrap returns the sentinel error for errors.Is checks.
func (e *MissingMessageVariablesError) Unwrap() error {
	return ErrMessageVariableMissing
}

// FormatMessage replaces {name} placeholders with values from vars.
//
// Literal braces are escaped with doubled braces: "{{" renders "{" and "}}"
// renders "}". This is a deterministic, stdlib-only helper; it does not
// implement ICU MessageFormat, plural rules, or locale-aware formatting.
func FormatMessage(source string, vars map[string]string) (string, error) {
	return formatMessage("", source, vars)
}

// FormatMessage resolves key for locale using MessageCatalog fallbacks and
// renders {name} placeholders with vars.
func (c MessageCatalog) FormatMessage(locale, key string, vars map[string]string) (string, error) {
	locales := messageFallbackLocales(c.contract, locale)
	for _, candidate := range locales {
		entries, ok := c.messages[candidate]
		if !ok {
			continue
		}
		source, ok := entries[key]
		if !ok {
			continue
		}
		return formatMessage(key, source, vars)
	}

	return "", &MissingMessageError{
		Locale:   locale,
		Key:      key,
		Searched: append([]string(nil), locales...),
	}
}

func formatMessage(key, source string, vars map[string]string) (string, error) {
	var out strings.Builder
	missing := make([]string, 0)
	seenMissing := make(map[string]struct{})

	out.Grow(len(source))
	for i := 0; i < len(source); {
		switch source[i] {
		case '{':
			if i+1 < len(source) && source[i+1] == '{' {
				out.WriteByte('{')
				i += 2
				continue
			}

			closeOffset := strings.IndexByte(source[i+1:], '}')
			if closeOffset == -1 {
				return "", &InvalidMessageFormatError{
					Key:    key,
					Offset: i,
					Reason: "unclosed placeholder",
				}
			}

			end := i + 1 + closeOffset
			name := strings.TrimSpace(source[i+1 : end])
			if name == "" {
				return "", &InvalidMessageFormatError{
					Key:    key,
					Offset: i,
					Reason: "empty placeholder",
				}
			}
			if strings.Contains(name, "{") {
				return "", &InvalidMessageFormatError{
					Key:    key,
					Offset: i,
					Reason: "nested opening brace",
				}
			}

			value, ok := vars[name]
			if !ok {
				if _, seen := seenMissing[name]; !seen {
					seenMissing[name] = struct{}{}
					missing = append(missing, name)
				}
			} else {
				out.WriteString(value)
			}
			i = end + 1
		case '}':
			if i+1 < len(source) && source[i+1] == '}' {
				out.WriteByte('}')
				i += 2
				continue
			}

			return "", &InvalidMessageFormatError{
				Key:    key,
				Offset: i,
				Reason: "unescaped closing brace",
			}
		default:
			next := strings.IndexAny(source[i:], "{}")
			if next == -1 {
				out.WriteString(source[i:])
				i = len(source)
				continue
			}
			out.WriteString(source[i : i+next])
			i += next
		}
	}

	if len(missing) > 0 {
		return "", &MissingMessageVariablesError{
			Key:   key,
			Names: missing,
		}
	}

	return out.String(), nil
}

func quotedStrings(values []string) []string {
	quoted := make([]string, 0, len(values))
	for _, value := range values {
		quoted = append(quoted, fmt.Sprintf("%q", value))
	}
	return quoted
}
