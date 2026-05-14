package encryption

import (
	"bytes"
	"crypto/rand"
	"errors"
	"testing"
)

func TestRoundTrip(t *testing.T) {
	c := testCipher(t)
	tests := []struct {
		name      string
		plaintext []byte
	}{
		{name: "empty", plaintext: nil},
		{name: "sixteen bytes", plaintext: []byte("sixteen byte txt")},
		{name: "one kilobyte", plaintext: bytes.Repeat([]byte("x"), 1024)},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			encrypted, err := c.Encrypt(tt.plaintext)
			if err != nil {
				t.Fatalf("Encrypt() error = %v", err)
			}
			decrypted, err := c.Decrypt(encrypted)
			if err != nil {
				t.Fatalf("Decrypt() error = %v", err)
			}
			if !bytes.Equal(decrypted, tt.plaintext) {
				t.Fatalf("Decrypt() = %q, want %q", decrypted, tt.plaintext)
			}
		})
	}
}

func TestTamperDetection(t *testing.T) {
	c := testCipher(t)
	encrypted, err := c.Encrypt([]byte("sensitive value"))
	if err != nil {
		t.Fatalf("Encrypt() error = %v", err)
	}
	encrypted[len(encrypted)-1] ^= 0x01

	if _, err := c.Decrypt(encrypted); err == nil {
		t.Fatal("Decrypt() error = nil, want authentication failure")
	}
}

func TestWrongKey(t *testing.T) {
	a := testCipher(t)
	b := testCipher(t)
	encrypted, err := a.Encrypt([]byte("sensitive value"))
	if err != nil {
		t.Fatalf("Encrypt() error = %v", err)
	}

	if _, err := b.Decrypt(encrypted); err == nil {
		t.Fatal("Decrypt() error = nil, want authentication failure")
	}
}

func TestNewCipherKeySize(t *testing.T) {
	for _, size := range []int{31, 33} {
		if _, err := NewCipher(make([]byte, size)); !errors.Is(err, ErrKeySize) {
			t.Fatalf("NewCipher(%d-byte key) error = %v, want %v", size, err, ErrKeySize)
		}
	}
}

func TestEncryptUniqueNonce(t *testing.T) {
	c := testCipher(t)
	plaintext := []byte("same plaintext")
	first, err := c.Encrypt(plaintext)
	if err != nil {
		t.Fatalf("Encrypt() first error = %v", err)
	}
	second, err := c.Encrypt(plaintext)
	if err != nil {
		t.Fatalf("Encrypt() second error = %v", err)
	}
	if bytes.Equal(first, second) {
		t.Fatal("Encrypt() returned identical ciphertexts; want unique nonce")
	}
}

func testCipher(t *testing.T) *Cipher {
	t.Helper()
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		t.Fatalf("rand.Read() error = %v", err)
	}
	c, err := NewCipher(key)
	if err != nil {
		t.Fatalf("NewCipher() error = %v", err)
	}
	return c
}
