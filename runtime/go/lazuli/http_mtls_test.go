package lazuli

import (
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"net"
	"net/url"
	"slices"
	"testing"
)

func TestMTLSClientAuthTypeMapsModes(t *testing.T) {
	tests := []struct {
		mode MTLSClientAuthMode
		want tls.ClientAuthType
		ok   bool
	}{
		{mode: "", want: tls.NoClientCert, ok: true},
		{mode: MTLSClientAuthNone, want: tls.NoClientCert, ok: true},
		{mode: "disabled", want: tls.NoClientCert, ok: true},
		{mode: MTLSClientAuthRequest, want: tls.RequestClientCert, ok: true},
		{mode: "require-any-client-cert", want: tls.RequireAnyClientCert, ok: true},
		{mode: MTLSClientAuthVerifyIfGiven, want: tls.VerifyClientCertIfGiven, ok: true},
		{mode: "verify client cert if given", want: tls.VerifyClientCertIfGiven, ok: true},
		{mode: MTLSClientAuthRequireAndVerify, want: tls.RequireAndVerifyClientCert, ok: true},
		{mode: "unknown", want: tls.NoClientCert, ok: false},
	}

	for _, tt := range tests {
		t.Run(string(tt.mode), func(t *testing.T) {
			got, ok := MTLSClientAuthType(tt.mode)
			if ok != tt.ok {
				t.Fatalf("ok = %v, want %v", ok, tt.ok)
			}
			if got != tt.want {
				t.Fatalf("client auth = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestMTLSServerTLSConfigAppliesPolicyWithoutMutatingInputs(t *testing.T) {
	basePool := testMTLSCertPool("base")
	policyPool := testMTLSCertPool("policy")
	base := &tls.Config{
		ServerName: "api.example.test",
		ClientCAs:  basePool,
	}

	config, ok := MTLSServerTLSConfig(base, MTLSServerPolicy{
		ClientAuth: MTLSClientAuthRequireAndVerify,
		ClientCAs:  policyPool,
	})

	if !ok {
		t.Fatal("MTLSServerTLSConfig returned ok=false")
	}
	if config == base {
		t.Fatal("MTLSServerTLSConfig returned the input config")
	}
	if config.ClientAuth != tls.RequireAndVerifyClientCert {
		t.Fatalf("ClientAuth = %v, want %v", config.ClientAuth, tls.RequireAndVerifyClientCert)
	}
	if config.ServerName != base.ServerName {
		t.Fatalf("ServerName = %q, want %q", config.ServerName, base.ServerName)
	}
	if config.ClientCAs == policyPool {
		t.Fatal("ClientCAs reused the policy pool")
	}
	config.ClientCAs.AddCert(testMTLSCertificate("added"))

	if base.ClientAuth != tls.NoClientCert {
		t.Fatalf("base ClientAuth = %v, want zero value", base.ClientAuth)
	}
	if got := len(basePool.Subjects()); got != 1 {
		t.Fatalf("base pool subjects = %d, want 1", got)
	}
	if got := len(policyPool.Subjects()); got != 1 {
		t.Fatalf("policy pool subjects = %d, want 1", got)
	}
}

func TestMTLSServerTLSConfigRejectsUnknownClientAuthMode(t *testing.T) {
	config, ok := MTLSServerTLSConfig(&tls.Config{}, MTLSServerPolicy{ClientAuth: "bogus"})

	if ok {
		t.Fatal("ok = true, want false")
	}
	if config != nil {
		t.Fatalf("config = %#v, want nil", config)
	}
}

func TestCloneMTLSConfigClonesMutableCertificateState(t *testing.T) {
	mapped := tls.Certificate{
		Certificate: [][]byte{{7, 8, 9}},
		Leaf: &x509.Certificate{
			DNSNames: []string{"mapped.example.test"},
		},
	}
	base := &tls.Config{
		Certificates: []tls.Certificate{{
			Certificate:                 [][]byte{{1, 2, 3}},
			OCSPStaple:                  []byte{4, 5},
			SignedCertificateTimestamps: [][]byte{{6}},
			Leaf: &x509.Certificate{
				DNSNames:    []string{"api.example.test"},
				IPAddresses: []net.IP{net.ParseIP("10.0.0.7")},
				URIs:        []*url.URL{mustParseMTLSURL(t, "spiffe://example.test/ns/default/sa/api")},
			},
		}},
		NameToCertificate: map[string]*tls.Certificate{
			"mapped.example.test": &mapped,
		},
		RootCAs:   testMTLSCertPool("root"),
		ClientCAs: testMTLSCertPool("client"),
	}

	clone := CloneMTLSConfig(base)

	clone.Certificates[0].Certificate[0][0] = 99
	clone.Certificates[0].OCSPStaple[0] = 99
	clone.Certificates[0].SignedCertificateTimestamps[0][0] = 99
	clone.Certificates[0].Leaf.DNSNames[0] = "changed.example.test"
	clone.Certificates[0].Leaf.IPAddresses[0][0] = 99
	clone.Certificates[0].Leaf.URIs[0].Host = "changed.example.test"
	clone.NameToCertificate["mapped.example.test"].Certificate[0][0] = 99
	clone.NameToCertificate["mapped.example.test"].Leaf.DNSNames[0] = "changed.example.test"
	clone.RootCAs.AddCert(testMTLSCertificate("root-added"))
	clone.ClientCAs.AddCert(testMTLSCertificate("client-added"))

	if got := base.Certificates[0].Certificate[0][0]; got != 1 {
		t.Fatalf("base certificate byte = %d, want 1", got)
	}
	if got := base.Certificates[0].OCSPStaple[0]; got != 4 {
		t.Fatalf("base OCSP byte = %d, want 4", got)
	}
	if got := base.Certificates[0].SignedCertificateTimestamps[0][0]; got != 6 {
		t.Fatalf("base SCT byte = %d, want 6", got)
	}
	if got := base.Certificates[0].Leaf.DNSNames[0]; got != "api.example.test" {
		t.Fatalf("base leaf DNSName = %q, want api.example.test", got)
	}
	if got := base.Certificates[0].Leaf.IPAddresses[0].String(); got != "10.0.0.7" {
		t.Fatalf("base leaf IP = %q, want 10.0.0.7", got)
	}
	if got := base.Certificates[0].Leaf.URIs[0].Host; got != "example.test" {
		t.Fatalf("base leaf URI host = %q, want example.test", got)
	}
	if got := mapped.Certificate[0][0]; got != 7 {
		t.Fatalf("mapped certificate byte = %d, want 7", got)
	}
	if got := mapped.Leaf.DNSNames[0]; got != "mapped.example.test" {
		t.Fatalf("mapped leaf DNSName = %q, want mapped.example.test", got)
	}
	if got := len(base.RootCAs.Subjects()); got != 1 {
		t.Fatalf("base RootCAs subjects = %d, want 1", got)
	}
	if got := len(base.ClientCAs.Subjects()); got != 1 {
		t.Fatalf("base ClientCAs subjects = %d, want 1", got)
	}
}

func TestMatchCertificateSANUsesX509HostnameRules(t *testing.T) {
	cert := &x509.Certificate{
		DNSNames:    []string{"*.example.test"},
		IPAddresses: []net.IP{net.ParseIP("10.0.0.5")},
	}

	if !MatchCertificateSAN(cert, "api.example.test") {
		t.Fatal("wildcard DNS SAN did not match subdomain")
	}
	if !MatchCertificateSAN(cert, "10.0.0.5") {
		t.Fatal("IP SAN did not match IP hostname")
	}
	if MatchCertificateSAN(cert, "example.test") {
		t.Fatal("wildcard DNS SAN matched apex domain")
	}
	if MatchCertificateSAN(&x509.Certificate{Subject: pkixName("legacy.example.test")}, "legacy.example.test") {
		t.Fatal("Common Name matched without SAN")
	}
}

func TestMatchCertificateURI(t *testing.T) {
	cert := &x509.Certificate{
		URIs: []*url.URL{
			mustParseMTLSURL(t, "spiffe://example.test/ns/default/sa/api"),
			mustParseMTLSURL(t, "urn:example:client"),
		},
	}

	if !MatchCertificateURI(cert, "SPIFFE://EXAMPLE.TEST/ns/default/sa/api") {
		t.Fatal("URI SAN did not match case-insensitive scheme and host")
	}
	if !MatchCertificateURI(cert, "urn:example:client") {
		t.Fatal("URN SAN did not match exact URI")
	}
	if MatchCertificateURI(cert, "spiffe://example.test/ns/default/sa/other") {
		t.Fatal("different URI SAN matched")
	}
	if MatchCertificateURI(cert, "/relative") {
		t.Fatal("relative URI matched")
	}
}

func TestMatchCertificateSPIFFEIDAndPathPrefix(t *testing.T) {
	cert := &x509.Certificate{
		URIs: []*url.URL{
			mustParseMTLSURL(t, "spiffe://example.test/ns/default/sa/api"),
			mustParseMTLSURL(t, "spiffe://example.test/ns/defaultish/sa/api"),
			mustParseMTLSURL(t, "spiffe://example.test/ns/query?bad=true"),
		},
	}

	if !MatchCertificateSPIFFEID(cert, "EXAMPLE.TEST", "/ns/default/sa/api") {
		t.Fatal("SPIFFE ID did not match exact trust domain and path")
	}
	if MatchCertificateSPIFFEID(cert, "example.test", "/ns/default") {
		t.Fatal("partial SPIFFE ID matched exact helper")
	}
	if !MatchCertificateSPIFFEPathPrefix(cert, "example.test", "/ns/default") {
		t.Fatal("SPIFFE path prefix did not match child path")
	}
	if MatchCertificateSPIFFEPathPrefix(&x509.Certificate{
		URIs: []*url.URL{mustParseMTLSURL(t, "spiffe://example.test/ns/defaultish/sa/api")},
	}, "example.test", "/ns/default") {
		t.Fatal("SPIFFE path prefix matched sibling path segment")
	}
	if MatchCertificateSPIFFEPathPrefix(cert, "example.test", "/ns/default/sa/other") {
		t.Fatal("unrelated SPIFFE path prefix matched")
	}
	if MatchCertificateSPIFFEPathPrefix(cert, "example.test", "/ns/query") {
		t.Fatal("SPIFFE URI with query matched")
	}
}

func TestMTLSCertificateMatchers(t *testing.T) {
	cert := &x509.Certificate{
		DNSNames: []string{"api.example.test"},
		URIs: []*url.URL{
			mustParseMTLSURL(t, "spiffe://example.test/ns/default/sa/api"),
			mustParseMTLSURL(t, "urn:example:client"),
		},
	}

	if !MatchMTLSCertificate(cert,
		MTLSCertificateSANMatcher("api.example.test"),
		MTLSCertificateURIMatcher("urn:example:client"),
		MTLSCertificateSPIFFEPathPrefixMatcher("example.test", "/ns/default"),
	) {
		t.Fatal("certificate did not satisfy all matchers")
	}
	if MatchMTLSCertificate(cert, MTLSCertificateSPIFFEIDMatcher("example.test", "/ns/default/sa/other")) {
		t.Fatal("certificate satisfied mismatched SPIFFE ID matcher")
	}
	if MatchMTLSCertificate(nil, MTLSCertificateSANMatcher("api.example.test")) {
		t.Fatal("nil certificate matched")
	}
}

func TestNormalizeMTLSStringsDropsEmptyAndDuplicates(t *testing.T) {
	got := normalizeMTLSStrings([]string{" api.example.test ", "", "api.example.test", "other.example.test"})
	want := []string{"api.example.test", "other.example.test"}

	if !slices.Equal(got, want) {
		t.Fatalf("normalizeMTLSStrings = %v, want %v", got, want)
	}
}

func testMTLSCertPool(label string) *x509.CertPool {
	pool := x509.NewCertPool()
	pool.AddCert(testMTLSCertificate(label))
	return pool
}

func testMTLSCertificate(label string) *x509.Certificate {
	return &x509.Certificate{
		Raw:        []byte("cert-" + label),
		RawSubject: []byte("subject-" + label),
	}
}

func mustParseMTLSURL(t *testing.T, rawURL string) *url.URL {
	t.Helper()
	uri, err := url.Parse(rawURL)
	if err != nil {
		t.Fatalf("parse URL %q: %v", rawURL, err)
	}
	return uri
}

func pkixName(commonName string) pkix.Name {
	return pkix.Name{CommonName: commonName}
}
