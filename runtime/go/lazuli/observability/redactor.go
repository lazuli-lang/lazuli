// Package observability extension for log redaction.
//
// PII scanning is a pre-emission filter: before any log line, error
// envelope, or diagnostic output leaves the process, the active
// Redactor scans for known patterns (CPF, CNPJ, email, phone, credit
// card) and masks them. The default uses Go regex; @plugin/pii-scan
// can swap in a smarter matcher (NER models, custom dictionaries).
package observability

import (
	"regexp"
	"sync/atomic"
)

// Redactor masks sensitive substrings in arbitrary text. Implementations
// MUST be safe for concurrent use AND deterministic: the same input
// must produce the same masked output.
type Redactor interface {
	Redact(text string) string
}

var activeRedactor atomic.Value

// SetRedactor installs the active redactor. Called from @plugin/pii-scan's
// init block; nil resets to RegexRedactor.
func SetRedactor(r Redactor) {
	if r == nil {
		r = RegexRedactor{}
	}
	activeRedactor.Store(redactorHolder{redactor: r})
}

// Active returns the currently-installed redactor.
func Active() Redactor {
	if h, ok := activeRedactor.Load().(redactorHolder); ok && h.redactor != nil {
		return h.redactor
	}
	return RegexRedactor{}
}

type redactorHolder struct {
	redactor Redactor
}

// RegexRedactor is the default v0 redactor: regex match-and-replace for a
// closed catalog of patterns. @plugin/pii-scan replaces this with a richer
// matcher.
type RegexRedactor struct{}

func (RegexRedactor) Redact(text string) string {
	out := text
	for _, p := range piiPatterns {
		out = p.regex.ReplaceAllString(out, p.mask)
	}
	return out
}

var piiPatterns = []piiPattern{
	{regex: regexp.MustCompile(`\d{3}\.\d{3}\.\d{3}-\d{2}`), mask: "***.***.***-**"},
	{regex: regexp.MustCompile(`\d{2}\.\d{3}\.\d{3}/\d{4}-\d{2}`), mask: "**.***.***/****-**"},
	{regex: regexp.MustCompile(`\(\d{2}\) \d{4,5}-\d{4}`), mask: "(**) *****-****"},
	{regex: regexp.MustCompile(`[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}`), mask: "***@***.***"},
	{regex: regexp.MustCompile(`\d{4} ?\d{4} ?\d{4} ?\d{4}`), mask: "**** **** **** ****"},
}

type piiPattern struct {
	regex *regexp.Regexp
	mask  string
}
