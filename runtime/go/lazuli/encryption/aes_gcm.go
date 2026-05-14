// Package encryption provides AES-256-GCM symmetric encryption for
// at-rest field protection. Pair with the `@cap.Secret` lzi surface.
package encryption

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"errors"
	"io"
)

// ErrKeySize is returned when the supplied key is not 32 bytes.
var ErrKeySize = errors.New("encryption: key must be 32 bytes (AES-256)")

// Cipher wraps an AES-256-GCM AEAD. Construct with NewCipher; reuse across
// goroutines (cipher.AEAD is safe for concurrent use).
type Cipher struct {
	aead cipher.AEAD
}

// NewCipher returns a Cipher initialised with a 32-byte key. The key
// material is the caller's responsibility (env, KMS, vault).
func NewCipher(key []byte) (*Cipher, error) {
	if len(key) != 32 {
		return nil, ErrKeySize
	}
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, err
	}
	aead, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}
	return &Cipher{aead: aead}, nil
}

// Encrypt returns nonce || ciphertext || tag. Output is len(plaintext)+28 bytes
// (12 nonce + 16 GCM tag). Plaintext nil is treated as empty.
func (c *Cipher) Encrypt(plaintext []byte) ([]byte, error) {
	nonce := make([]byte, c.aead.NonceSize())
	if _, err := io.ReadFull(rand.Reader, nonce); err != nil {
		return nil, err
	}
	// Seal appends to dst; using nonce as dst makes the sealed output self-contained.
	return c.aead.Seal(nonce, nonce, plaintext, nil), nil
}

// Decrypt parses nonce || ciphertext || tag and returns plaintext.
// Returns an error if the input is too short or authentication fails.
func (c *Cipher) Decrypt(ciphertext []byte) ([]byte, error) {
	ns := c.aead.NonceSize()
	if len(ciphertext) < ns {
		return nil, errors.New("encryption: ciphertext too short")
	}
	nonce, body := ciphertext[:ns], ciphertext[ns:]
	return c.aead.Open(nil, nonce, body, nil)
}
