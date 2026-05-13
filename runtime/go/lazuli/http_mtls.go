package lazuli

import (
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/asn1"
	"math/big"
	"net"
	"net/url"
	"strings"
)

// MTLSClientAuthMode is Lazuli's stable token for tls.ClientAuthType.
type MTLSClientAuthMode string

const (
	MTLSClientAuthNone             MTLSClientAuthMode = "none"
	MTLSClientAuthRequest          MTLSClientAuthMode = "request"
	MTLSClientAuthRequireAny       MTLSClientAuthMode = "require_any"
	MTLSClientAuthVerifyIfGiven    MTLSClientAuthMode = "verify_if_given"
	MTLSClientAuthRequireAndVerify MTLSClientAuthMode = "require_and_verify"
)

// TLSClientAuthType returns the crypto/tls client-auth mode for m.
func (m MTLSClientAuthMode) TLSClientAuthType() (tls.ClientAuthType, bool) {
	return MTLSClientAuthType(m)
}

// MTLSClientAuthType maps Lazuli's mTLS client-auth token to crypto/tls.
func MTLSClientAuthType(mode MTLSClientAuthMode) (tls.ClientAuthType, bool) {
	switch normalizeMTLSClientAuthMode(string(mode)) {
	case "", "none", "off", "disabled", "no_client_cert":
		return tls.NoClientCert, true
	case "request", "request_client_cert":
		return tls.RequestClientCert, true
	case "require_any", "require_any_client_cert":
		return tls.RequireAnyClientCert, true
	case "verify_if_given", "verify_client_cert_if_given":
		return tls.VerifyClientCertIfGiven, true
	case "require_and_verify", "require_and_verify_client_cert":
		return tls.RequireAndVerifyClientCert, true
	default:
		return tls.NoClientCert, false
	}
}

// MTLSServerPolicy configures the mTLS fields on a server tls.Config.
type MTLSServerPolicy struct {
	// ClientAuth selects the server's client-certificate requirement.
	ClientAuth MTLSClientAuthMode
	// ClientCAs is the certificate pool used to verify client certificates.
	ClientCAs *x509.CertPool
}

// MTLSServerTLSConfig returns a cloned TLS config with the mTLS server policy
// applied. The input config and CA pool are not mutated. ok is false when the
// client-auth mode is unknown.
func MTLSServerTLSConfig(config *tls.Config, policy MTLSServerPolicy) (*tls.Config, bool) {
	clientAuth, ok := policy.ClientAuth.TLSClientAuthType()
	if !ok {
		return nil, false
	}

	clone := CloneMTLSConfig(config)
	clone.ClientAuth = clientAuth
	if policy.ClientCAs != nil {
		clone.ClientCAs = cloneCertPool(policy.ClientCAs)
	}
	return clone, true
}

// CloneMTLSConfig returns a clone of config suitable for mTLS policy mutation.
// It includes CloneTLSConfig's isolation and also clones certificate slices,
// leaf certificate values, certificate maps, RootCAs, and ClientCAs. Opaque key
// values and callback functions are shared.
func CloneMTLSConfig(config *tls.Config) *tls.Config {
	clone := CloneTLSConfig(config)
	if config == nil {
		return clone
	}

	clone.Certificates = cloneTLSCertificates(config.Certificates)
	clone.NameToCertificate = cloneTLSCertificateMap(config.NameToCertificate)
	clone.RootCAs = cloneCertPool(config.RootCAs)
	clone.ClientCAs = cloneCertPool(config.ClientCAs)
	return clone
}

// MTLSCertificateMatcher reports whether a client certificate satisfies one
// mTLS identity predicate.
type MTLSCertificateMatcher func(*x509.Certificate) bool

// MatchMTLSCertificate reports whether cert satisfies every non-nil matcher.
// A non-nil certificate with no matchers is accepted.
func MatchMTLSCertificate(cert *x509.Certificate, matchers ...MTLSCertificateMatcher) bool {
	if cert == nil {
		return false
	}
	for _, matcher := range matchers {
		if matcher != nil && !matcher(cert) {
			return false
		}
	}
	return true
}

// MTLSCertificateSANMatcher returns a matcher for DNS or IP subjectAltName
// values using x509.Certificate.VerifyHostname semantics.
func MTLSCertificateSANMatcher(names ...string) MTLSCertificateMatcher {
	names = normalizeMTLSStrings(names)
	return func(cert *x509.Certificate) bool {
		for _, name := range names {
			if MatchCertificateSAN(cert, name) {
				return true
			}
		}
		return false
	}
}

// MTLSCertificateURIMatcher returns a matcher for exact URI subjectAltName
// values.
func MTLSCertificateURIMatcher(rawURIs ...string) MTLSCertificateMatcher {
	rawURIs = normalizeMTLSStrings(rawURIs)
	return func(cert *x509.Certificate) bool {
		for _, rawURI := range rawURIs {
			if MatchCertificateURI(cert, rawURI) {
				return true
			}
		}
		return false
	}
}

// MTLSCertificateSPIFFEIDMatcher returns a matcher for exact SPIFFE-style URI
// subjectAltName values.
func MTLSCertificateSPIFFEIDMatcher(trustDomain string, paths ...string) MTLSCertificateMatcher {
	trustDomain, trustDomainOK := normalizeSPIFFETrustDomain(trustDomain)
	paths = normalizeMTLSStrings(paths)
	return func(cert *x509.Certificate) bool {
		if !trustDomainOK {
			return false
		}
		for _, path := range paths {
			if MatchCertificateSPIFFEID(cert, trustDomain, path) {
				return true
			}
		}
		return false
	}
}

// MTLSCertificateSPIFFEPathPrefixMatcher returns a matcher for SPIFFE-style URI
// subjectAltName values under trustDomain and one of the path prefixes.
func MTLSCertificateSPIFFEPathPrefixMatcher(trustDomain string, prefixes ...string) MTLSCertificateMatcher {
	trustDomain, trustDomainOK := normalizeSPIFFETrustDomain(trustDomain)
	prefixes = normalizeMTLSStrings(prefixes)
	return func(cert *x509.Certificate) bool {
		if !trustDomainOK {
			return false
		}
		for _, prefix := range prefixes {
			if MatchCertificateSPIFFEPathPrefix(cert, trustDomain, prefix) {
				return true
			}
		}
		return false
	}
}

// MatchCertificateSAN reports whether cert contains a DNS or IP subjectAltName
// matching name. Common Name is intentionally not used.
func MatchCertificateSAN(cert *x509.Certificate, name string) bool {
	if cert == nil {
		return false
	}
	name = strings.TrimSpace(name)
	if name == "" {
		return false
	}
	return cert.VerifyHostname(name) == nil
}

// MatchCertificateURI reports whether cert contains an exact URI subjectAltName
// matching rawURI.
func MatchCertificateURI(cert *x509.Certificate, rawURI string) bool {
	if cert == nil {
		return false
	}
	want, ok := parseAbsoluteCertificateURI(rawURI)
	if !ok {
		return false
	}

	for _, got := range cert.URIs {
		if certificateURIsEqual(got, want) {
			return true
		}
	}
	return false
}

// MatchCertificateSPIFFEID reports whether cert contains an exact
// SPIFFE-style URI subjectAltName of spiffe://trustDomain/path.
func MatchCertificateSPIFFEID(cert *x509.Certificate, trustDomain, path string) bool {
	if cert == nil {
		return false
	}
	trustDomain, ok := normalizeSPIFFETrustDomain(trustDomain)
	if !ok {
		return false
	}
	path, ok = normalizeSPIFFEPath(path)
	if !ok {
		return false
	}

	for _, uri := range cert.URIs {
		gotTrustDomain, gotPath, ok := spiffeURIFields(uri)
		if ok && gotTrustDomain == trustDomain && gotPath == path {
			return true
		}
	}
	return false
}

// MatchCertificateSPIFFEPathPrefix reports whether cert contains a
// SPIFFE-style URI subjectAltName under trustDomain and pathPrefix.
func MatchCertificateSPIFFEPathPrefix(cert *x509.Certificate, trustDomain, pathPrefix string) bool {
	if cert == nil {
		return false
	}
	trustDomain, ok := normalizeSPIFFETrustDomain(trustDomain)
	if !ok {
		return false
	}
	pathPrefix, ok = normalizeSPIFFEPath(pathPrefix)
	if !ok {
		return false
	}

	for _, uri := range cert.URIs {
		gotTrustDomain, gotPath, ok := spiffeURIFields(uri)
		if ok && gotTrustDomain == trustDomain && spiffePathHasPrefix(gotPath, pathPrefix) {
			return true
		}
	}
	return false
}

func normalizeMTLSClientAuthMode(value string) string {
	value = strings.ToLower(strings.TrimSpace(value))
	value = strings.NewReplacer("-", "_", " ", "_").Replace(value)
	for strings.Contains(value, "__") {
		value = strings.ReplaceAll(value, "__", "_")
	}
	return value
}

func normalizeMTLSStrings(values []string) []string {
	normalized := make([]string, 0, len(values))
	seen := make(map[string]struct{}, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		normalized = append(normalized, value)
	}
	return normalized
}

func parseAbsoluteCertificateURI(rawURI string) (*url.URL, bool) {
	rawURI = strings.TrimSpace(rawURI)
	if rawURI == "" {
		return nil, false
	}
	uri, err := url.Parse(rawURI)
	if err != nil || uri.Scheme == "" {
		return nil, false
	}
	return uri, true
}

func certificateURIsEqual(a, b *url.URL) bool {
	if a == nil || b == nil {
		return false
	}

	left := *a
	right := *b
	left.Scheme = strings.ToLower(left.Scheme)
	right.Scheme = strings.ToLower(right.Scheme)
	left.Host = strings.ToLower(left.Host)
	right.Host = strings.ToLower(right.Host)
	return left.String() == right.String()
}

func spiffeURIFields(uri *url.URL) (string, string, bool) {
	if uri == nil || !strings.EqualFold(uri.Scheme, "spiffe") || uri.Opaque != "" ||
		uri.User != nil || uri.RawQuery != "" || uri.Fragment != "" || uri.Port() != "" {
		return "", "", false
	}
	trustDomain, ok := normalizeSPIFFETrustDomain(uri.Host)
	if !ok {
		return "", "", false
	}
	path, ok := normalizeSPIFFEPath(uri.Path)
	if !ok {
		return "", "", false
	}
	return trustDomain, path, true
}

func normalizeSPIFFETrustDomain(value string) (string, bool) {
	value = strings.ToLower(strings.TrimSpace(value))
	if value == "" || strings.ContainsAny(value, " \t\r\n/\\:@?#[]") {
		return "", false
	}
	return value, true
}

func normalizeSPIFFEPath(value string) (string, bool) {
	value = strings.TrimSpace(value)
	if value == "" || !strings.HasPrefix(value, "/") || strings.ContainsAny(value, " \t\r\n?#") {
		return "", false
	}
	return value, true
}

func spiffePathHasPrefix(path, prefix string) bool {
	if prefix == "/" {
		return true
	}
	if path == prefix {
		return true
	}
	if strings.HasSuffix(prefix, "/") {
		return strings.HasPrefix(path, prefix)
	}
	return strings.HasPrefix(path, prefix+"/")
}

func cloneTLSCertificates(certs []tls.Certificate) []tls.Certificate {
	if certs == nil {
		return nil
	}
	cloned := make([]tls.Certificate, len(certs))
	for i, cert := range certs {
		cloned[i] = cloneTLSCertificate(cert)
	}
	return cloned
}

func cloneTLSCertificateMap(certs map[string]*tls.Certificate) map[string]*tls.Certificate {
	if certs == nil {
		return nil
	}
	cloned := make(map[string]*tls.Certificate, len(certs))
	for name, cert := range certs {
		if cert == nil {
			cloned[name] = nil
			continue
		}
		certClone := cloneTLSCertificate(*cert)
		cloned[name] = &certClone
	}
	return cloned
}

func cloneTLSCertificate(cert tls.Certificate) tls.Certificate {
	cert.Certificate = cloneByteSlices(cert.Certificate)
	cert.OCSPStaple = cloneByteSlice(cert.OCSPStaple)
	cert.SignedCertificateTimestamps = cloneByteSlices(cert.SignedCertificateTimestamps)
	cert.Leaf = cloneX509Certificate(cert.Leaf)
	return cert
}

func cloneCertPool(pool *x509.CertPool) *x509.CertPool {
	if pool == nil {
		return nil
	}
	return pool.Clone()
}

func cloneX509Certificate(cert *x509.Certificate) *x509.Certificate {
	if cert == nil {
		return nil
	}

	clone := *cert
	clone.Raw = cloneByteSlice(cert.Raw)
	clone.RawTBSCertificate = cloneByteSlice(cert.RawTBSCertificate)
	clone.RawSubjectPublicKeyInfo = cloneByteSlice(cert.RawSubjectPublicKeyInfo)
	clone.RawSubject = cloneByteSlice(cert.RawSubject)
	clone.RawIssuer = cloneByteSlice(cert.RawIssuer)
	clone.Signature = cloneByteSlice(cert.Signature)
	clone.SerialNumber = cloneBigInt(cert.SerialNumber)
	clone.Issuer = clonePKIXName(cert.Issuer)
	clone.Subject = clonePKIXName(cert.Subject)
	clone.Extensions = clonePKIXExtensions(cert.Extensions)
	clone.ExtraExtensions = clonePKIXExtensions(cert.ExtraExtensions)
	clone.UnhandledCriticalExtensions = cloneObjectIdentifiers(cert.UnhandledCriticalExtensions)
	clone.ExtKeyUsage = cloneExtKeyUsages(cert.ExtKeyUsage)
	clone.UnknownExtKeyUsage = cloneObjectIdentifiers(cert.UnknownExtKeyUsage)
	clone.SubjectKeyId = cloneByteSlice(cert.SubjectKeyId)
	clone.AuthorityKeyId = cloneByteSlice(cert.AuthorityKeyId)
	clone.OCSPServer = cloneStringSlice(cert.OCSPServer)
	clone.IssuingCertificateURL = cloneStringSlice(cert.IssuingCertificateURL)
	clone.DNSNames = cloneStringSlice(cert.DNSNames)
	clone.EmailAddresses = cloneStringSlice(cert.EmailAddresses)
	clone.IPAddresses = cloneIPs(cert.IPAddresses)
	clone.URIs = cloneURLs(cert.URIs)
	clone.PermittedDNSDomains = cloneStringSlice(cert.PermittedDNSDomains)
	clone.ExcludedDNSDomains = cloneStringSlice(cert.ExcludedDNSDomains)
	clone.PermittedIPRanges = cloneIPNets(cert.PermittedIPRanges)
	clone.ExcludedIPRanges = cloneIPNets(cert.ExcludedIPRanges)
	clone.PermittedEmailAddresses = cloneStringSlice(cert.PermittedEmailAddresses)
	clone.ExcludedEmailAddresses = cloneStringSlice(cert.ExcludedEmailAddresses)
	clone.PermittedURIDomains = cloneStringSlice(cert.PermittedURIDomains)
	clone.ExcludedURIDomains = cloneStringSlice(cert.ExcludedURIDomains)
	clone.CRLDistributionPoints = cloneStringSlice(cert.CRLDistributionPoints)
	clone.PolicyIdentifiers = cloneObjectIdentifiers(cert.PolicyIdentifiers)
	clone.Policies = cloneX509OIDs(cert.Policies)
	clone.PolicyMappings = clonePolicyMappings(cert.PolicyMappings)
	return &clone
}

func clonePKIXName(name pkix.Name) pkix.Name {
	name.Country = cloneStringSlice(name.Country)
	name.Organization = cloneStringSlice(name.Organization)
	name.OrganizationalUnit = cloneStringSlice(name.OrganizationalUnit)
	name.Locality = cloneStringSlice(name.Locality)
	name.Province = cloneStringSlice(name.Province)
	name.StreetAddress = cloneStringSlice(name.StreetAddress)
	name.PostalCode = cloneStringSlice(name.PostalCode)
	name.Names = cloneAttributeTypeAndValues(name.Names)
	name.ExtraNames = cloneAttributeTypeAndValues(name.ExtraNames)
	return name
}

func cloneAttributeTypeAndValues(values []pkix.AttributeTypeAndValue) []pkix.AttributeTypeAndValue {
	if values == nil {
		return nil
	}
	cloned := make([]pkix.AttributeTypeAndValue, len(values))
	for i, value := range values {
		cloned[i] = value
		cloned[i].Type = cloneObjectIdentifier(value.Type)
	}
	return cloned
}

func clonePKIXExtensions(values []pkix.Extension) []pkix.Extension {
	if values == nil {
		return nil
	}
	cloned := make([]pkix.Extension, len(values))
	for i, value := range values {
		cloned[i] = value
		cloned[i].Id = cloneObjectIdentifier(value.Id)
		cloned[i].Value = cloneByteSlice(value.Value)
	}
	return cloned
}

func cloneObjectIdentifiers(values []asn1.ObjectIdentifier) []asn1.ObjectIdentifier {
	if values == nil {
		return nil
	}
	cloned := make([]asn1.ObjectIdentifier, len(values))
	for i, value := range values {
		cloned[i] = cloneObjectIdentifier(value)
	}
	return cloned
}

func cloneObjectIdentifier(value asn1.ObjectIdentifier) asn1.ObjectIdentifier {
	if value == nil {
		return nil
	}
	cloned := make(asn1.ObjectIdentifier, len(value))
	copy(cloned, value)
	return cloned
}

func cloneExtKeyUsages(values []x509.ExtKeyUsage) []x509.ExtKeyUsage {
	if values == nil {
		return nil
	}
	cloned := make([]x509.ExtKeyUsage, len(values))
	copy(cloned, values)
	return cloned
}

func cloneX509OIDs(values []x509.OID) []x509.OID {
	if values == nil {
		return nil
	}
	cloned := make([]x509.OID, len(values))
	copy(cloned, values)
	return cloned
}

func clonePolicyMappings(values []x509.PolicyMapping) []x509.PolicyMapping {
	if values == nil {
		return nil
	}
	cloned := make([]x509.PolicyMapping, len(values))
	copy(cloned, values)
	return cloned
}

func cloneBigInt(value *big.Int) *big.Int {
	if value == nil {
		return nil
	}
	return new(big.Int).Set(value)
}

func cloneByteSlices(values [][]byte) [][]byte {
	if values == nil {
		return nil
	}
	cloned := make([][]byte, len(values))
	for i, value := range values {
		cloned[i] = cloneByteSlice(value)
	}
	return cloned
}

func cloneByteSlice(value []byte) []byte {
	if value == nil {
		return nil
	}
	cloned := make([]byte, len(value))
	copy(cloned, value)
	return cloned
}

func cloneIPs(values []net.IP) []net.IP {
	if values == nil {
		return nil
	}
	cloned := make([]net.IP, len(values))
	for i, value := range values {
		cloned[i] = cloneByteSlice(value)
	}
	return cloned
}

func cloneIPNets(values []*net.IPNet) []*net.IPNet {
	if values == nil {
		return nil
	}
	cloned := make([]*net.IPNet, len(values))
	for i, value := range values {
		if value == nil {
			continue
		}
		cloned[i] = &net.IPNet{
			IP:   cloneByteSlice(value.IP),
			Mask: cloneByteSlice(value.Mask),
		}
	}
	return cloned
}

func cloneURLs(values []*url.URL) []*url.URL {
	if values == nil {
		return nil
	}
	cloned := make([]*url.URL, len(values))
	for i, value := range values {
		if value == nil {
			continue
		}
		uri := *value
		cloned[i] = &uri
	}
	return cloned
}
