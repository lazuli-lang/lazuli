package i18n

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"path"
	"strings"
)

var (
	// ErrMessageNotFound is wrapped when no message key resolves across the
	// requested locale, its fallback chain, and the contract default.
	ErrMessageNotFound = errors.New("lazuli/i18n: message not found")
	// ErrMessageVariableMissing is wrapped when a message contains a
	// {{name}} placeholder that is absent from the provided variables.
	ErrMessageVariableMissing = errors.New("lazuli/i18n: message variable missing")
)

// MessageCatalog is a small runtime fallback catalog for flat message maps.
// It supports LocaleContract fallback resolution and {{name}} interpolation
// only; it intentionally does not implement ICU MessageFormat or CLDR plural
// rules.
type MessageCatalog struct {
	contract LocaleContract
	messages map[string]map[string]string
}

// MissingMessageError describes a message key that could not be found in any
// locale searched during fallback resolution.
type MissingMessageError struct {
	Locale   string
	Key      string
	Searched []string
}

func (e *MissingMessageError) Error() string {
	if len(e.Searched) == 0 {
		return fmt.Sprintf("%s: key %q for locale %q", ErrMessageNotFound, e.Key, e.Locale)
	}
	return fmt.Sprintf("%s: key %q for locale %q (searched %s)", ErrMessageNotFound, e.Key, e.Locale, strings.Join(e.Searched, ", "))
}

// Unwrap returns the sentinel error for errors.Is checks.
func (e *MissingMessageError) Unwrap() error {
	return ErrMessageNotFound
}

// MissingMessageVariableError describes a placeholder that could not be
// replaced while rendering a found message.
type MissingMessageVariableError struct {
	Key  string
	Name string
}

func (e *MissingMessageVariableError) Error() string {
	return fmt.Sprintf("%s: key %q variable %q", ErrMessageVariableMissing, e.Key, e.Name)
}

// Unwrap returns the sentinel error for errors.Is checks.
func (e *MissingMessageVariableError) Unwrap() error {
	return ErrMessageVariableMissing
}

// NewMessageCatalog builds an in-memory message catalog from a
// locale-to-key-to-message map. The input map is copied so callers may mutate
// their source data after construction.
func NewMessageCatalog(contract LocaleContract, messages map[string]map[string]string) MessageCatalog {
	return MessageCatalog{
		contract: contract,
		messages: cloneMessageMap(messages),
	}
}

// LoadMessageCatalog loads flat JSON message files from Catalog.FS under
// Catalog.BasePath. Each file name without the .json suffix is treated as the
// locale tag, and each JSON file must be an object of string keys to string
// messages.
func LoadMessageCatalog(catalog Catalog, contract LocaleContract) (MessageCatalog, error) {
	loaded, err := LoadMessageCatalogFS(catalog.FS, catalog.BasePath, contract)
	if err != nil && catalog.Name != "" {
		return MessageCatalog{}, fmt.Errorf("lazuli/i18n: load message catalog %q: %w", catalog.Name, err)
	}
	return loaded, err
}

// LoadMessageCatalogFS loads flat JSON message files from fsys under
// basePath. It is useful for tests and non-embedded file systems; generated
// runtime code should usually call LoadMessageCatalog with the lowered Catalog
// value.
func LoadMessageCatalogFS(fsys fs.FS, basePath string, contract LocaleContract) (MessageCatalog, error) {
	base, err := cleanMessageBasePath(basePath)
	if err != nil {
		return MessageCatalog{}, err
	}

	entries, err := fs.ReadDir(fsys, base)
	if err != nil {
		return MessageCatalog{}, fmt.Errorf("lazuli/i18n: read message catalog %q: %w", base, err)
	}

	messages := make(map[string]map[string]string)
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		locale := strings.TrimSuffix(entry.Name(), ".json")
		if locale == "" {
			return MessageCatalog{}, fmt.Errorf("lazuli/i18n: invalid empty locale file %q", entry.Name())
		}

		filePath := path.Join(base, entry.Name())
		data, err := fs.ReadFile(fsys, filePath)
		if err != nil {
			return MessageCatalog{}, fmt.Errorf("lazuli/i18n: read message file %q: %w", filePath, err)
		}

		var fileMessages map[string]string
		if err := json.Unmarshal(data, &fileMessages); err != nil {
			return MessageCatalog{}, fmt.Errorf("lazuli/i18n: decode message file %q: %w", filePath, err)
		}
		if fileMessages == nil {
			fileMessages = map[string]string{}
		}
		messages[locale] = fileMessages
	}

	return NewMessageCatalog(contract, messages), nil
}

// Lookup resolves key for locale using LocaleContract fallbacks and renders
// simple {{name}} placeholders with vars. Missing placeholders return
// ErrMessageVariableMissing instead of falling through to another locale.
func (c MessageCatalog) Lookup(locale, key string, vars map[string]string) (string, error) {
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
		return interpolateMessage(key, source, vars)
	}

	return "", &MissingMessageError{
		Locale:   locale,
		Key:      key,
		Searched: append([]string(nil), locales...),
	}
}

func cloneMessageMap(messages map[string]map[string]string) map[string]map[string]string {
	out := make(map[string]map[string]string, len(messages))
	for locale, entries := range messages {
		copied := make(map[string]string, len(entries))
		for key, message := range entries {
			copied[key] = message
		}
		out[locale] = copied
	}
	return out
}

func cleanMessageBasePath(basePath string) (string, error) {
	if strings.Contains(basePath, "\\") {
		return "", fmt.Errorf("lazuli/i18n: invalid message catalog base path %q", basePath)
	}
	base := path.Clean(basePath)
	if !fs.ValidPath(base) {
		return "", fmt.Errorf("lazuli/i18n: invalid message catalog base path %q", basePath)
	}
	return base, nil
}

func messageFallbackLocales(contract LocaleContract, locale string) []string {
	seen := make(map[string]struct{})
	locales := make([]string, 0, 3)
	add := func(tag string) {
		if tag == "" {
			return
		}
		if _, ok := seen[tag]; ok {
			return
		}
		seen[tag] = struct{}{}
		locales = append(locales, tag)
	}

	cur := locale
	if cur == "" {
		cur = contract.Default
	}
	walked := make(map[string]struct{})
	for cur != "" {
		if _, ok := walked[cur]; ok {
			break
		}
		walked[cur] = struct{}{}
		if contract.IsSupported(cur) || cur == contract.Default {
			add(cur)
		}

		next := ""
		for _, fallback := range contract.Fallbacks {
			if fallback.From == cur {
				next = fallback.To
				break
			}
		}
		if next == "" {
			break
		}
		cur = next
	}
	add(contract.Default)
	return locales
}

func interpolateMessage(key, source string, vars map[string]string) (string, error) {
	var out strings.Builder
	remaining := source
	for {
		start := strings.Index(remaining, "{{")
		if start == -1 {
			out.WriteString(remaining)
			return out.String(), nil
		}
		out.WriteString(remaining[:start])
		afterStart := remaining[start+len("{{"):]
		end := strings.Index(afterStart, "}}")
		if end == -1 {
			out.WriteString(remaining[start:])
			return out.String(), nil
		}

		name := strings.TrimSpace(afterStart[:end])
		value, ok := vars[name]
		if !ok {
			return "", &MissingMessageVariableError{Key: key, Name: name}
		}
		out.WriteString(value)
		remaining = afterStart[end+len("}}"):]
	}
}
