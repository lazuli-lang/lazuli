package email

import (
	"errors"
	"fmt"
	"net"
	"strconv"
	"strings"
)

// SMTPServerTLSMode describes how an SMTP server descriptor expects transport
// security to be offered. It does not start listeners or load certificates.
type SMTPServerTLSMode string

const (
	// SMTPServerTLSNone disables TLS for the descriptor.
	SMTPServerTLSNone SMTPServerTLSMode = "none"
	// SMTPServerTLSStartTLS advertises STARTTLS on a plain SMTP connection.
	SMTPServerTLSStartTLS SMTPServerTLSMode = "starttls"
	// SMTPServerTLSImplicit expects TLS from connection start.
	SMTPServerTLSImplicit SMTPServerTLSMode = "implicit_tls"
)

// SMTPServerAuthMode describes whether SMTP AUTH is offered or required.
type SMTPServerAuthMode string

const (
	// SMTPServerAuthNone disables SMTP AUTH.
	SMTPServerAuthNone SMTPServerAuthMode = "none"
	// SMTPServerAuthOptional allows clients to send mail with or without AUTH.
	SMTPServerAuthOptional SMTPServerAuthMode = "optional"
	// SMTPServerAuthRequired rejects unauthenticated mail.
	SMTPServerAuthRequired SMTPServerAuthMode = "required"
)

var (
	// ErrInvalidSMTPServerDescriptor is wrapped by malformed inbound SMTP
	// server descriptors.
	ErrInvalidSMTPServerDescriptor = errors.New("email: invalid smtp server descriptor")
)

// SMTPServerBindAddress is the host and port a future SMTP server may bind.
//
// Host may be empty to describe all interfaces. Port must be explicit; no
// socket is opened by these helpers.
type SMTPServerBindAddress struct {
	Host string
	Port int
}

// SMTPMailboxRoute maps accepted SMTP recipients to an application mailbox.
//
// Set Recipient for one exact addr-spec, Domain for every recipient under a
// domain, or neither for a catch-all route. Mailbox is an application-defined
// mailbox key.
type SMTPMailboxRoute struct {
	Mailbox   string
	Recipient string
	Domain    string
}

// SMTPServerDescriptor describes future inbound/dev SMTP behavior without
// opening sockets or depending on a concrete server implementation.
type SMTPServerDescriptor struct {
	BindAddress SMTPServerBindAddress
	TLSMode     SMTPServerTLSMode
	AuthMode    SMTPServerAuthMode
	Routes      []SMTPMailboxRoute
}

// Addr returns the net.Listen-compatible host:port representation.
func (b SMTPServerBindAddress) Addr() string {
	return net.JoinHostPort(strings.TrimSpace(b.Host), strconv.Itoa(b.Port))
}

// String returns Addr.
func (b SMTPServerBindAddress) String() string {
	return b.Addr()
}

// Normalize returns a copy with the host trimmed.
func (b SMTPServerBindAddress) Normalize() SMTPServerBindAddress {
	b.Host = strings.TrimSpace(b.Host)
	return b
}

// Validate checks that the bind descriptor is structurally usable.
func (b SMTPServerBindAddress) Validate() error {
	if err := b.validate(); err != nil {
		return smtpServerInvalidf("bind_address: %v", err)
	}
	return nil
}

// Normalize returns the canonical TLS mode. Unknown values are lower-cased and
// trimmed so callers can report or persist the normalized attempt.
func (m SMTPServerTLSMode) Normalize() SMTPServerTLSMode {
	normalized, _ := normalizeSMTPServerTLSMode(m)
	return normalized
}

// Valid reports whether m is a known TLS mode. Empty means none.
func (m SMTPServerTLSMode) Valid() bool {
	_, ok := normalizeSMTPServerTLSMode(m)
	return ok
}

// Normalize returns the canonical auth mode. Unknown values are lower-cased and
// trimmed so callers can report or persist the normalized attempt.
func (m SMTPServerAuthMode) Normalize() SMTPServerAuthMode {
	normalized, _ := normalizeSMTPServerAuthMode(m)
	return normalized
}

// Valid reports whether m is a known auth mode. Empty means none.
func (m SMTPServerAuthMode) Valid() bool {
	_, ok := normalizeSMTPServerAuthMode(m)
	return ok
}

// Normalize returns a canonical copy of the mailbox route.
func (r SMTPMailboxRoute) Normalize() SMTPMailboxRoute {
	normalized, err := normalizeSMTPMailboxRoute(r)
	if err == nil {
		return normalized
	}

	r.Mailbox = strings.TrimSpace(r.Mailbox)
	r.Recipient = strings.ToLower(strings.TrimSpace(r.Recipient))
	r.Domain = strings.ToLower(strings.TrimSpace(r.Domain))
	return r
}

// Validate checks that the route has a mailbox and one unambiguous selector.
func (r SMTPMailboxRoute) Validate() error {
	if _, err := normalizeSMTPMailboxRoute(r); err != nil {
		return smtpServerInvalidf("%v", err)
	}
	return nil
}

// CatchAll reports whether the route matches recipients not claimed by an
// exact recipient or domain route.
func (r SMTPMailboxRoute) CatchAll() bool {
	return strings.TrimSpace(r.Recipient) == "" && strings.TrimSpace(r.Domain) == ""
}

// Normalize returns a canonical descriptor copy.
func (d SMTPServerDescriptor) Normalize() SMTPServerDescriptor {
	d.BindAddress = d.BindAddress.Normalize()
	d.TLSMode = d.TLSMode.Normalize()
	d.AuthMode = d.AuthMode.Normalize()
	if d.Routes != nil {
		routes := make([]SMTPMailboxRoute, len(d.Routes))
		for i, route := range d.Routes {
			routes[i] = route.Normalize()
		}
		d.Routes = routes
	}
	return d
}

// BindAddr returns the descriptor's net.Listen-compatible bind address.
func (d SMTPServerDescriptor) BindAddr() string {
	return d.BindAddress.Addr()
}

// Validate checks descriptor shape, modes, bind address, and route ambiguity.
func (d SMTPServerDescriptor) Validate() error {
	return ValidateSMTPServerDescriptor(d)
}

// RouteForRecipient resolves recipient to the route selected by this
// descriptor. Exact recipient routes win over domain routes, which win over a
// catch-all route.
func (d SMTPServerDescriptor) RouteForRecipient(recipient string) (SMTPMailboxRoute, bool, error) {
	return ResolveSMTPMailboxRoute(d.Routes, recipient)
}

// ValidateSMTPServerDescriptor checks an inbound SMTP server descriptor without
// opening sockets.
func ValidateSMTPServerDescriptor(descriptor SMTPServerDescriptor) error {
	var errs []error
	if err := descriptor.BindAddress.validate(); err != nil {
		errs = append(errs, smtpServerInvalidf("bind_address: %v", err))
	}
	if _, ok := normalizeSMTPServerTLSMode(descriptor.TLSMode); !ok {
		errs = append(errs, smtpServerInvalidf("tls_mode %q is invalid", descriptor.TLSMode))
	}
	if _, ok := normalizeSMTPServerAuthMode(descriptor.AuthMode); !ok {
		errs = append(errs, smtpServerInvalidf("auth_mode %q is invalid", descriptor.AuthMode))
	}
	if len(descriptor.Routes) == 0 {
		errs = append(errs, smtpServerInvalidf("at least one mailbox route is required"))
	} else if err := validateSMTPMailboxRoutes(descriptor.Routes); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

// ResolveSMTPMailboxRoute selects the mailbox route for recipient. It accepts
// raw addr-spec recipients and does not require the surrounding descriptor.
func ResolveSMTPMailboxRoute(routes []SMTPMailboxRoute, recipient string) (SMTPMailboxRoute, bool, error) {
	normalizedRecipient, recipientDomain, err := normalizeSMTPMailboxAddress(recipient)
	if err != nil {
		return SMTPMailboxRoute{}, false, smtpServerInvalidf("recipient: %v", err)
	}

	var domainRoute SMTPMailboxRoute
	var catchAllRoute SMTPMailboxRoute
	var hasDomainRoute bool
	var hasCatchAllRoute bool

	for i, route := range routes {
		normalizedRoute, err := normalizeSMTPMailboxRoute(route)
		if err != nil {
			return SMTPMailboxRoute{}, false, smtpServerInvalidf("route %d: %v", i, err)
		}

		switch {
		case normalizedRoute.Recipient != "":
			if normalizedRoute.Recipient == normalizedRecipient {
				return normalizedRoute, true, nil
			}
		case normalizedRoute.Domain != "":
			if normalizedRoute.Domain == recipientDomain && !hasDomainRoute {
				domainRoute = normalizedRoute
				hasDomainRoute = true
			}
		default:
			if !hasCatchAllRoute {
				catchAllRoute = normalizedRoute
				hasCatchAllRoute = true
			}
		}
	}

	if hasDomainRoute {
		return domainRoute, true, nil
	}
	if hasCatchAllRoute {
		return catchAllRoute, true, nil
	}
	return SMTPMailboxRoute{}, false, nil
}

func (b SMTPServerBindAddress) validate() error {
	b = b.Normalize()
	if b.Port <= 0 || b.Port > 65535 {
		return fmt.Errorf("port must be between 1 and 65535")
	}
	if b.Host == "" {
		return nil
	}
	if containsControl(b.Host) || containsWhitespace(b.Host) {
		return fmt.Errorf("host contains control characters or whitespace")
	}
	if strings.HasPrefix(b.Host, "[") || strings.HasSuffix(b.Host, "]") {
		return fmt.Errorf("host must omit IPv6 brackets")
	}
	if host, port, err := net.SplitHostPort(b.Host); err == nil && host != "" && port != "" {
		return fmt.Errorf("host must not include a port")
	}
	if strings.Contains(b.Host, ":") && net.ParseIP(b.Host) == nil {
		return fmt.Errorf("host contains an invalid colon")
	}
	return nil
}

func validateSMTPMailboxRoutes(routes []SMTPMailboxRoute) error {
	seen := make(map[string]int, len(routes))
	for i, route := range routes {
		normalizedRoute, err := normalizeSMTPMailboxRoute(route)
		if err != nil {
			return smtpServerInvalidf("route %d: %v", i, err)
		}

		key := smtpMailboxRouteKey(normalizedRoute)
		if previous, ok := seen[key]; ok {
			return smtpServerInvalidf("route %d duplicates route %d selector %q", i, previous, key)
		}
		seen[key] = i
	}
	return nil
}

func normalizeSMTPMailboxRoute(route SMTPMailboxRoute) (SMTPMailboxRoute, error) {
	route.Mailbox = strings.TrimSpace(route.Mailbox)
	if route.Mailbox == "" {
		return SMTPMailboxRoute{}, fmt.Errorf("mailbox is required")
	}
	if containsControl(route.Mailbox) {
		return SMTPMailboxRoute{}, fmt.Errorf("mailbox contains control characters")
	}

	route.Recipient = strings.TrimSpace(route.Recipient)
	route.Domain = strings.TrimSpace(route.Domain)
	if route.Recipient != "" && route.Domain != "" {
		return SMTPMailboxRoute{}, fmt.Errorf("recipient and domain are mutually exclusive")
	}
	if route.Recipient != "" {
		recipient, _, err := normalizeSMTPMailboxAddress(route.Recipient)
		if err != nil {
			return SMTPMailboxRoute{}, fmt.Errorf("recipient is invalid: %v", err)
		}
		route.Recipient = recipient
	}
	if route.Domain != "" {
		domain, err := normalizeSMTPMailboxDomain(route.Domain)
		if err != nil {
			return SMTPMailboxRoute{}, err
		}
		route.Domain = domain
	}
	return route, nil
}

func normalizeSMTPMailboxAddress(address string) (string, string, error) {
	address = strings.TrimSpace(address)
	if err := ValidateAddress(Address{Email: address}); err != nil {
		return "", "", err
	}
	at := strings.LastIndex(address, "@")
	if at <= 0 || at == len(address)-1 {
		return "", "", fmt.Errorf("address must contain local part and domain")
	}
	return strings.ToLower(address), strings.ToLower(address[at+1:]), nil
}

func normalizeSMTPMailboxDomain(domain string) (string, error) {
	domain = strings.ToLower(strings.TrimSpace(domain))
	if domain == "" {
		return "", fmt.Errorf("domain is required")
	}
	if containsControl(domain) || containsWhitespace(domain) || strings.ContainsAny(domain, "@<>") {
		return "", fmt.Errorf("domain contains invalid characters")
	}
	if _, _, err := normalizeSMTPMailboxAddress("postmaster@" + domain); err != nil {
		return "", fmt.Errorf("domain is invalid: %v", err)
	}
	return domain, nil
}

func normalizeSMTPServerTLSMode(mode SMTPServerTLSMode) (SMTPServerTLSMode, bool) {
	value := strings.ToLower(strings.TrimSpace(string(mode)))
	switch value {
	case "", "none", "off", "disabled":
		return SMTPServerTLSNone, true
	case "starttls", "start_tls":
		return SMTPServerTLSStartTLS, true
	case "implicit", "implicit_tls", "tls":
		return SMTPServerTLSImplicit, true
	default:
		return SMTPServerTLSMode(value), false
	}
}

func normalizeSMTPServerAuthMode(mode SMTPServerAuthMode) (SMTPServerAuthMode, bool) {
	value := strings.ToLower(strings.TrimSpace(string(mode)))
	switch value {
	case "", "none", "off", "disabled":
		return SMTPServerAuthNone, true
	case "optional":
		return SMTPServerAuthOptional, true
	case "required", "require":
		return SMTPServerAuthRequired, true
	default:
		return SMTPServerAuthMode(value), false
	}
}

func smtpMailboxRouteKey(route SMTPMailboxRoute) string {
	switch {
	case route.Recipient != "":
		return "recipient:" + route.Recipient
	case route.Domain != "":
		return "domain:" + route.Domain
	default:
		return "catch_all"
	}
}

func smtpServerInvalidf(format string, args ...any) error {
	return fmt.Errorf("%w: "+format, append([]any{ErrInvalidSMTPServerDescriptor}, args...)...)
}
