package views

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

const (
	// DefaultSyntaxLanguage is the fallback language for unlabelled code.
	DefaultSyntaxLanguage SyntaxLanguage = "text"

	// DefaultSyntaxTheme is the fallback theme for generated syntax views.
	DefaultSyntaxTheme SyntaxTheme = "lazuli-light"
)

const (
	SyntaxLanguageCSS        SyntaxLanguage = "css"
	SyntaxLanguageDiff       SyntaxLanguage = "diff"
	SyntaxLanguageDockerfile SyntaxLanguage = "dockerfile"
	SyntaxLanguageGo         SyntaxLanguage = "go"
	SyntaxLanguageHTML       SyntaxLanguage = "html"
	SyntaxLanguageJavaScript SyntaxLanguage = "javascript"
	SyntaxLanguageJSON       SyntaxLanguage = "json"
	SyntaxLanguageMarkdown   SyntaxLanguage = "markdown"
	SyntaxLanguageShell      SyntaxLanguage = "shell"
	SyntaxLanguageSQL        SyntaxLanguage = "sql"
	SyntaxLanguageText       SyntaxLanguage = "text"
	SyntaxLanguageTOML       SyntaxLanguage = "toml"
	SyntaxLanguageTypeScript SyntaxLanguage = "typescript"
	SyntaxLanguageXML        SyntaxLanguage = "xml"
	SyntaxLanguageYAML       SyntaxLanguage = "yaml"
)

const (
	SyntaxThemeLazuliLight   SyntaxTheme = "lazuli-light"
	SyntaxThemeLazuliDark    SyntaxTheme = "lazuli-dark"
	SyntaxThemeContrastDark  SyntaxTheme = "contrast-dark"
	SyntaxThemeContrastLight SyntaxTheme = "contrast-light"
)

const (
	SyntaxTokenAttribute   SyntaxTokenClass = "attribute"
	SyntaxTokenBoolean     SyntaxTokenClass = "boolean"
	SyntaxTokenBuiltin     SyntaxTokenClass = "builtin"
	SyntaxTokenChanged     SyntaxTokenClass = "changed"
	SyntaxTokenComment     SyntaxTokenClass = "comment"
	SyntaxTokenConstant    SyntaxTokenClass = "constant"
	SyntaxTokenDecorator   SyntaxTokenClass = "decorator"
	SyntaxTokenDeleted     SyntaxTokenClass = "deleted"
	SyntaxTokenEmphasis    SyntaxTokenClass = "emphasis"
	SyntaxTokenError       SyntaxTokenClass = "error"
	SyntaxTokenEscape      SyntaxTokenClass = "escape"
	SyntaxTokenFunction    SyntaxTokenClass = "function"
	SyntaxTokenHeading     SyntaxTokenClass = "heading"
	SyntaxTokenInserted    SyntaxTokenClass = "inserted"
	SyntaxTokenKeyword     SyntaxTokenClass = "keyword"
	SyntaxTokenLink        SyntaxTokenClass = "link"
	SyntaxTokenMeta        SyntaxTokenClass = "meta"
	SyntaxTokenNamespace   SyntaxTokenClass = "namespace"
	SyntaxTokenNumber      SyntaxTokenClass = "number"
	SyntaxTokenOperator    SyntaxTokenClass = "operator"
	SyntaxTokenProperty    SyntaxTokenClass = "property"
	SyntaxTokenPunctuation SyntaxTokenClass = "punctuation"
	SyntaxTokenRegex       SyntaxTokenClass = "regex"
	SyntaxTokenSelector    SyntaxTokenClass = "selector"
	SyntaxTokenString      SyntaxTokenClass = "string"
	SyntaxTokenStrong      SyntaxTokenClass = "strong"
	SyntaxTokenTag         SyntaxTokenClass = "tag"
	SyntaxTokenText        SyntaxTokenClass = "text"
	SyntaxTokenType        SyntaxTokenClass = "type"
	SyntaxTokenVariable    SyntaxTokenClass = "variable"
)

var (
	// ErrInvalidSyntaxLanguage reports an unknown syntax language name or alias.
	ErrInvalidSyntaxLanguage = errors.New("lazuli/views: invalid syntax language")

	// ErrInvalidSyntaxTheme reports an unknown syntax theme name or alias.
	ErrInvalidSyntaxTheme = errors.New("lazuli/views: invalid syntax theme")

	// ErrInvalidSyntaxTokenClass reports an unknown or unsafe syntax token class.
	ErrInvalidSyntaxTokenClass = errors.New("lazuli/views: invalid syntax token class")
)

// SyntaxLanguage is a canonical renderer-neutral syntax language name.
type SyntaxLanguage string

// SyntaxTheme is a canonical renderer-neutral syntax theme name.
type SyntaxTheme string

// SyntaxTokenClass is a canonical renderer-neutral syntax token class.
type SyntaxTokenClass string

// SyntaxLanguageDescriptor describes one supported syntax language and aliases
// accepted by NormalizeSyntaxLanguage.
type SyntaxLanguageDescriptor struct {
	Name    SyntaxLanguage
	Label   string
	Aliases []string
}

// SyntaxThemeDescriptor describes one supported syntax theme without coupling
// Lazuli to a renderer or stylesheet implementation.
type SyntaxThemeDescriptor struct {
	Name         SyntaxTheme
	Label        string
	Dark         bool
	HighContrast bool
	Aliases      []string
}

var syntaxLanguageCatalog = []SyntaxLanguageDescriptor{
	{Name: SyntaxLanguageCSS, Label: "CSS", Aliases: []string{"css3", "stylesheet"}},
	{Name: SyntaxLanguageDiff, Label: "Diff", Aliases: []string{"patch", "udiff"}},
	{Name: SyntaxLanguageDockerfile, Label: "Dockerfile", Aliases: []string{"containerfile", "docker"}},
	{Name: SyntaxLanguageGo, Label: "Go", Aliases: []string{"golang"}},
	{Name: SyntaxLanguageHTML, Label: "HTML", Aliases: []string{"htm"}},
	{Name: SyntaxLanguageJavaScript, Label: "JavaScript", Aliases: []string{"cjs", "js", "jsx", "mjs", "node"}},
	{Name: SyntaxLanguageJSON, Label: "JSON", Aliases: []string{"jsonc"}},
	{Name: SyntaxLanguageMarkdown, Label: "Markdown", Aliases: []string{"md", "mdown"}},
	{Name: SyntaxLanguageShell, Label: "Shell", Aliases: []string{"bash", "console", "sh", "terminal", "zsh"}},
	{Name: SyntaxLanguageSQL, Label: "SQL", Aliases: []string{"mysql", "postgres", "postgresql", "sqlite"}},
	{Name: SyntaxLanguageText, Label: "Plain text", Aliases: []string{"none", "plain", "plaintext", "txt"}},
	{Name: SyntaxLanguageTOML, Label: "TOML"},
	{Name: SyntaxLanguageTypeScript, Label: "TypeScript", Aliases: []string{"ts", "tsx"}},
	{Name: SyntaxLanguageXML, Label: "XML", Aliases: []string{"atom", "rss", "svg"}},
	{Name: SyntaxLanguageYAML, Label: "YAML", Aliases: []string{"yml"}},
}

var syntaxThemeCatalog = []SyntaxThemeDescriptor{
	{Name: SyntaxThemeContrastDark, Label: "Contrast Dark", Dark: true, HighContrast: true, Aliases: []string{"high-contrast-dark"}},
	{Name: SyntaxThemeContrastLight, Label: "Contrast Light", HighContrast: true, Aliases: []string{"contrast", "high-contrast", "high-contrast-light"}},
	{Name: SyntaxThemeLazuliDark, Label: "Lazuli Dark", Dark: true, Aliases: []string{"dark"}},
	{Name: SyntaxThemeLazuliLight, Label: "Lazuli Light", Aliases: []string{"default", "light"}},
}

var syntaxTokenClassCatalog = []SyntaxTokenClass{
	SyntaxTokenAttribute,
	SyntaxTokenBoolean,
	SyntaxTokenBuiltin,
	SyntaxTokenChanged,
	SyntaxTokenComment,
	SyntaxTokenConstant,
	SyntaxTokenDecorator,
	SyntaxTokenDeleted,
	SyntaxTokenEmphasis,
	SyntaxTokenError,
	SyntaxTokenEscape,
	SyntaxTokenFunction,
	SyntaxTokenHeading,
	SyntaxTokenInserted,
	SyntaxTokenKeyword,
	SyntaxTokenLink,
	SyntaxTokenMeta,
	SyntaxTokenNamespace,
	SyntaxTokenNumber,
	SyntaxTokenOperator,
	SyntaxTokenProperty,
	SyntaxTokenPunctuation,
	SyntaxTokenRegex,
	SyntaxTokenSelector,
	SyntaxTokenString,
	SyntaxTokenStrong,
	SyntaxTokenTag,
	SyntaxTokenText,
	SyntaxTokenType,
	SyntaxTokenVariable,
}

var syntaxTokenClassAliases = map[string]SyntaxTokenClass{
	"bool":       SyntaxTokenBoolean,
	"class-name": SyntaxTokenType,
	"func":       SyntaxTokenFunction,
	"invalid":    SyntaxTokenError,
	"literal":    SyntaxTokenString,
	"name":       SyntaxTokenVariable,
	"plain":      SyntaxTokenText,
	"regexp":     SyntaxTokenRegex,
}

var (
	syntaxLanguageIndex = buildSyntaxLanguageIndex(syntaxLanguageCatalog)
	syntaxThemeIndex    = buildSyntaxThemeIndex(syntaxThemeCatalog)
	syntaxTokenClassSet = buildSyntaxTokenClassSet(syntaxTokenClassCatalog)
)

// SyntaxLanguageDescriptors returns supported language descriptors sorted by
// canonical language name.
func SyntaxLanguageDescriptors() []SyntaxLanguageDescriptor {
	return sortedSyntaxLanguageDescriptors(syntaxLanguageCatalog)
}

// LookupSyntaxLanguage returns the descriptor for language or alias.
func LookupSyntaxLanguage(language string) (SyntaxLanguageDescriptor, bool) {
	name, ok := NormalizeSyntaxLanguage(language)
	if !ok {
		return SyntaxLanguageDescriptor{}, false
	}
	return syntaxLanguageDescriptor(name)
}

// NormalizeSyntaxLanguage returns the canonical language for a name, alias, or
// file extension such as ".go".
func NormalizeSyntaxLanguage(language string) (SyntaxLanguage, bool) {
	name, ok := syntaxLanguageIndex[syntaxLookupKey(language)]
	return name, ok
}

// ValidateSyntaxLanguage reports whether language is known by name or alias.
func ValidateSyntaxLanguage(language string) error {
	if _, ok := NormalizeSyntaxLanguage(language); ok {
		return nil
	}
	return fmt.Errorf("%w: %q", ErrInvalidSyntaxLanguage, strings.TrimSpace(language))
}

// ResolveSyntaxLanguage returns the first known language among language and
// fallbacks. When none match, it returns DefaultSyntaxLanguage.
func ResolveSyntaxLanguage(language string, fallbacks ...string) SyntaxLanguageDescriptor {
	if descriptor, ok := LookupSyntaxLanguage(language); ok {
		return descriptor
	}
	for _, fallback := range fallbacks {
		if descriptor, ok := LookupSyntaxLanguage(fallback); ok {
			return descriptor
		}
	}
	descriptor, _ := syntaxLanguageDescriptor(DefaultSyntaxLanguage)
	return descriptor
}

// SyntaxThemeDescriptors returns supported theme descriptors sorted by
// canonical theme name.
func SyntaxThemeDescriptors() []SyntaxThemeDescriptor {
	return sortedSyntaxThemeDescriptors(syntaxThemeCatalog)
}

// LookupSyntaxTheme returns the descriptor for theme or alias.
func LookupSyntaxTheme(theme string) (SyntaxThemeDescriptor, bool) {
	name, ok := NormalizeSyntaxTheme(theme)
	if !ok {
		return SyntaxThemeDescriptor{}, false
	}
	return syntaxThemeDescriptor(name)
}

// NormalizeSyntaxTheme returns the canonical theme for a name or alias.
func NormalizeSyntaxTheme(theme string) (SyntaxTheme, bool) {
	name, ok := syntaxThemeIndex[syntaxLookupKey(theme)]
	return name, ok
}

// ValidateSyntaxTheme reports whether theme is known by name or alias.
func ValidateSyntaxTheme(theme string) error {
	if _, ok := NormalizeSyntaxTheme(theme); ok {
		return nil
	}
	return fmt.Errorf("%w: %q", ErrInvalidSyntaxTheme, strings.TrimSpace(theme))
}

// ResolveSyntaxTheme returns the first known theme among theme and fallbacks.
// When none match, it returns DefaultSyntaxTheme.
func ResolveSyntaxTheme(theme string, fallbacks ...string) SyntaxThemeDescriptor {
	if descriptor, ok := LookupSyntaxTheme(theme); ok {
		return descriptor
	}
	for _, fallback := range fallbacks {
		if descriptor, ok := LookupSyntaxTheme(fallback); ok {
			return descriptor
		}
	}
	descriptor, _ := syntaxThemeDescriptor(DefaultSyntaxTheme)
	return descriptor
}

// SyntaxTokenClasses returns supported token classes sorted by class name.
func SyntaxTokenClasses() []SyntaxTokenClass {
	out := append([]SyntaxTokenClass(nil), syntaxTokenClassCatalog...)
	sort.Slice(out, func(i, j int) bool {
		return out[i] < out[j]
	})
	return out
}

// NormalizeSyntaxTokenClass returns the canonical renderer-neutral token class.
// Common CSS-style "syntax-" and "token-" prefixes are accepted and removed.
func NormalizeSyntaxTokenClass(class string) (SyntaxTokenClass, error) {
	key := syntaxTokenClassKey(class)
	if key == "" {
		return "", fmt.Errorf("%w: value is required", ErrInvalidSyntaxTokenClass)
	}
	if containsSyntaxTokenClassSeparator(key) {
		return "", fmt.Errorf("%w: %q contains whitespace, control characters, or separators", ErrInvalidSyntaxTokenClass, strings.TrimSpace(class))
	}
	if alias, ok := syntaxTokenClassAliases[key]; ok {
		return alias, nil
	}
	tokenClass := SyntaxTokenClass(key)
	if _, ok := syntaxTokenClassSet[tokenClass]; !ok {
		return "", fmt.Errorf("%w: %q", ErrInvalidSyntaxTokenClass, strings.TrimSpace(class))
	}
	return tokenClass, nil
}

// ValidateSyntaxTokenClass reports whether class is a known safe token class.
func ValidateSyntaxTokenClass(class string) error {
	_, err := NormalizeSyntaxTokenClass(class)
	return err
}

// NormalizeSyntaxTokenClasses returns validated, deduplicated token classes in
// deterministic order.
func NormalizeSyntaxTokenClasses(classes []string) ([]SyntaxTokenClass, error) {
	seen := make(map[SyntaxTokenClass]struct{}, len(classes))
	var errs []error
	for i, class := range classes {
		normalized, err := NormalizeSyntaxTokenClass(class)
		if err != nil {
			errs = append(errs, fmt.Errorf("token_class[%d]: %w", i, err))
			continue
		}
		seen[normalized] = struct{}{}
	}
	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	out := make([]SyntaxTokenClass, 0, len(seen))
	for class := range seen {
		out = append(out, class)
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i] < out[j]
	})
	return out, nil
}

func syntaxLanguageDescriptor(name SyntaxLanguage) (SyntaxLanguageDescriptor, bool) {
	for _, descriptor := range syntaxLanguageCatalog {
		if descriptor.Name == name {
			return cloneSyntaxLanguageDescriptor(descriptor), true
		}
	}
	return SyntaxLanguageDescriptor{}, false
}

func syntaxThemeDescriptor(name SyntaxTheme) (SyntaxThemeDescriptor, bool) {
	for _, descriptor := range syntaxThemeCatalog {
		if descriptor.Name == name {
			return cloneSyntaxThemeDescriptor(descriptor), true
		}
	}
	return SyntaxThemeDescriptor{}, false
}

func sortedSyntaxLanguageDescriptors(descriptors []SyntaxLanguageDescriptor) []SyntaxLanguageDescriptor {
	out := make([]SyntaxLanguageDescriptor, 0, len(descriptors))
	for _, descriptor := range descriptors {
		out = append(out, cloneSyntaxLanguageDescriptor(descriptor))
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i].Name < out[j].Name
	})
	return out
}

func sortedSyntaxThemeDescriptors(descriptors []SyntaxThemeDescriptor) []SyntaxThemeDescriptor {
	out := make([]SyntaxThemeDescriptor, 0, len(descriptors))
	for _, descriptor := range descriptors {
		out = append(out, cloneSyntaxThemeDescriptor(descriptor))
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i].Name < out[j].Name
	})
	return out
}

func cloneSyntaxLanguageDescriptor(descriptor SyntaxLanguageDescriptor) SyntaxLanguageDescriptor {
	descriptor.Aliases = append([]string(nil), descriptor.Aliases...)
	sort.Strings(descriptor.Aliases)
	return descriptor
}

func cloneSyntaxThemeDescriptor(descriptor SyntaxThemeDescriptor) SyntaxThemeDescriptor {
	descriptor.Aliases = append([]string(nil), descriptor.Aliases...)
	sort.Strings(descriptor.Aliases)
	return descriptor
}

func buildSyntaxLanguageIndex(descriptors []SyntaxLanguageDescriptor) map[string]SyntaxLanguage {
	index := make(map[string]SyntaxLanguage, len(descriptors))
	for _, descriptor := range descriptors {
		registerSyntaxLanguageAlias(index, descriptor.Name, string(descriptor.Name))
		for _, alias := range descriptor.Aliases {
			registerSyntaxLanguageAlias(index, descriptor.Name, alias)
		}
	}
	return index
}

func registerSyntaxLanguageAlias(index map[string]SyntaxLanguage, language SyntaxLanguage, alias string) {
	key := syntaxLookupKey(alias)
	if key == "" {
		panic("lazuli/views: empty syntax language alias")
	}
	if existing, ok := index[key]; ok && existing != language {
		panic(fmt.Sprintf("lazuli/views: syntax language alias %q is used by %q and %q", alias, existing, language))
	}
	index[key] = language
}

func buildSyntaxThemeIndex(descriptors []SyntaxThemeDescriptor) map[string]SyntaxTheme {
	index := make(map[string]SyntaxTheme, len(descriptors))
	for _, descriptor := range descriptors {
		registerSyntaxThemeAlias(index, descriptor.Name, string(descriptor.Name))
		for _, alias := range descriptor.Aliases {
			registerSyntaxThemeAlias(index, descriptor.Name, alias)
		}
	}
	return index
}

func registerSyntaxThemeAlias(index map[string]SyntaxTheme, theme SyntaxTheme, alias string) {
	key := syntaxLookupKey(alias)
	if key == "" {
		panic("lazuli/views: empty syntax theme alias")
	}
	if existing, ok := index[key]; ok && existing != theme {
		panic(fmt.Sprintf("lazuli/views: syntax theme alias %q is used by %q and %q", alias, existing, theme))
	}
	index[key] = theme
}

func buildSyntaxTokenClassSet(classes []SyntaxTokenClass) map[SyntaxTokenClass]struct{} {
	set := make(map[SyntaxTokenClass]struct{}, len(classes))
	for _, class := range classes {
		if class == "" {
			panic("lazuli/views: empty syntax token class")
		}
		if _, ok := set[class]; ok {
			panic(fmt.Sprintf("lazuli/views: duplicate syntax token class %q", class))
		}
		set[class] = struct{}{}
	}
	return set
}

func syntaxLookupKey(value string) string {
	value = strings.ToLower(strings.TrimSpace(value))
	value = strings.TrimPrefix(value, ".")
	value = strings.ReplaceAll(value, "_", "-")
	return value
}

func syntaxTokenClassKey(class string) string {
	key := syntaxLookupKey(class)
	key = strings.TrimPrefix(key, "syntax-")
	key = strings.TrimPrefix(key, "token-")
	return key
}

func containsSyntaxTokenClassSeparator(value string) bool {
	for _, r := range value {
		if r == '.' || r == '#' || r == ';' || r == ',' || unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}
