package observability

import (
	"strings"
	"testing"
)

func TestRegexRedactorCPF(t *testing.T) {
	in := "user CPF 123.456.789-10 should redact"
	out := RegexRedactor{}.Redact(in)
	if !strings.Contains(out, "***.***.***-**") || strings.Contains(out, "123.456.789-10") {
		t.Fatalf("CPF leak: %q", out)
	}
}

func TestRegexRedactorEmail(t *testing.T) {
	in := "user@example.com signed up"
	out := RegexRedactor{}.Redact(in)
	if !strings.Contains(out, "***@***.***") || strings.Contains(out, "user@example.com") {
		t.Fatalf("email leak: %q", out)
	}
}

func TestRegexRedactorPhonePreservesShape(t *testing.T) {
	in := "ligar (11) 99999-9999"
	out := RegexRedactor{}.Redact(in)
	if strings.Contains(out, "99999-9999") {
		t.Fatalf("phone leak: %q", out)
	}
}
