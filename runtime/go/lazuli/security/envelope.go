// Package security provides provider-neutral helpers for encrypted Lazuli
// values. Adapter packages own key storage; this package owns the stable
// ciphertext envelope format and AES-GCM sealing logic.
package security

import (
	"bytes"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"errors"
	"fmt"
)

// Keyring resolves AES keys by id and identifies the current key used for
// new ciphertexts. Implementations may back this with KMS, Vault, process
// config, or any other provider-specific secret source.
type Keyring interface {
	// CurrentKey returns the key id and raw AES key used for new encryptions.
	CurrentKey() (keyID string, key []byte, err error)
	// Key returns the raw AES key for keyID so older ciphertexts remain
	// decryptable after key rotation.
	Key(keyID string) ([]byte, error)
}

// Ciphertext is the persisted AES-GCM envelope for an encrypted value. AAD is
// authenticated but not encrypted; callers should pass the expected AAD to
// Decrypt so values remain bound to their intended resource, field, tenant, or
// other domain context.
type Ciphertext struct {
	KeyID      string `json:"key_id"`
	Nonce      []byte `json:"nonce"`
	AAD        []byte `json:"aad,omitempty"`
	Ciphertext []byte `json:"ciphertext"`
}

var (
	// ErrKeyringRequired is returned when Encrypt or Decrypt is called without
	// a keyring.
	ErrKeyringRequired = errors.New("lazuli/security: keyring_required")
	// ErrKeyNotFound is returned when a key id cannot be resolved.
	ErrKeyNotFound = errors.New("lazuli/security: key_not_found")
	// ErrInvalidKey is returned when a resolved key is not a valid AES key.
	ErrInvalidKey = errors.New("lazuli/security: invalid_key")
	// ErrInvalidCiphertext is returned when an envelope is malformed or fails
	// AES-GCM authentication.
	ErrInvalidCiphertext = errors.New("lazuli/security: invalid_ciphertext")
	// ErrAADMismatch is returned when the envelope AAD does not equal the AAD
	// expected by the caller.
	ErrAADMismatch = errors.New("lazuli/security: aad_mismatch")
)

// StaticKeyring is an in-memory Keyring implementation for tests and local
// development. NewStaticKeyring copies all key material so caller mutations do
// not affect future encryption or decryption.
type StaticKeyring struct {
	currentID string
	keys      map[string][]byte
}

// NewStaticKeyring returns a static keyring whose currentID is used for new
// encryptions. Keys must be 16, 24, or 32 bytes for AES-128, AES-192, or
// AES-256.
func NewStaticKeyring(currentID string, keys map[string][]byte) (*StaticKeyring, error) {
	copied := make(map[string][]byte, len(keys))
	for keyID, key := range keys {
		if keyID == "" {
			return nil, fmt.Errorf("%w: empty key id", ErrInvalidKey)
		}
		if err := validateAESKey(key); err != nil {
			return nil, fmt.Errorf("%w: %s", err, keyID)
		}
		copied[keyID] = cloneBytes(key)
	}
	if _, ok := copied[currentID]; !ok {
		return nil, fmt.Errorf("%w: %s", ErrKeyNotFound, currentID)
	}
	return &StaticKeyring{
		currentID: currentID,
		keys:      copied,
	}, nil
}

// CurrentKey returns a copy of the current AES key.
func (k *StaticKeyring) CurrentKey() (string, []byte, error) {
	if k == nil {
		return "", nil, ErrKeyringRequired
	}
	key, ok := k.keys[k.currentID]
	if !ok {
		return "", nil, fmt.Errorf("%w: %s", ErrKeyNotFound, k.currentID)
	}
	return k.currentID, cloneBytes(key), nil
}

// Key returns a copy of the AES key for keyID.
func (k *StaticKeyring) Key(keyID string) ([]byte, error) {
	if k == nil {
		return nil, ErrKeyringRequired
	}
	key, ok := k.keys[keyID]
	if !ok {
		return nil, fmt.Errorf("%w: %s", ErrKeyNotFound, keyID)
	}
	return cloneBytes(key), nil
}

// Encrypt seals plaintext with the keyring's current key and returns a
// ciphertext envelope. The returned envelope owns copies of generated nonce,
// AAD, and ciphertext bytes.
func Encrypt(keyring Keyring, plaintext, aad []byte) (Ciphertext, error) {
	if keyring == nil {
		return Ciphertext{}, ErrKeyringRequired
	}
	keyID, key, err := keyring.CurrentKey()
	if err != nil {
		return Ciphertext{}, err
	}
	if keyID == "" {
		return Ciphertext{}, fmt.Errorf("%w: empty current key id", ErrKeyNotFound)
	}
	aead, err := newAEAD(key)
	if err != nil {
		return Ciphertext{}, err
	}

	nonce := make([]byte, aead.NonceSize())
	if _, err := rand.Read(nonce); err != nil {
		return Ciphertext{}, err
	}

	aadCopy := cloneBytes(aad)
	return Ciphertext{
		KeyID:      keyID,
		Nonce:      nonce,
		AAD:        aadCopy,
		Ciphertext: aead.Seal(nil, nonce, plaintext, aadCopy),
	}, nil
}

// Decrypt opens envelope with the key identified by envelope.KeyID. The
// expectedAAD must match envelope.AAD and is used as the AES-GCM additional
// authenticated data.
func Decrypt(keyring Keyring, envelope Ciphertext, expectedAAD []byte) ([]byte, error) {
	if keyring == nil {
		return nil, ErrKeyringRequired
	}
	if envelope.KeyID == "" || len(envelope.Nonce) == 0 || len(envelope.Ciphertext) == 0 {
		return nil, ErrInvalidCiphertext
	}
	if !bytes.Equal(envelope.AAD, expectedAAD) {
		return nil, ErrAADMismatch
	}

	key, err := keyring.Key(envelope.KeyID)
	if err != nil {
		return nil, err
	}
	aead, err := newAEAD(key)
	if err != nil {
		return nil, err
	}
	if len(envelope.Nonce) != aead.NonceSize() {
		return nil, fmt.Errorf("%w: nonce size", ErrInvalidCiphertext)
	}

	plaintext, err := aead.Open(nil, envelope.Nonce, envelope.Ciphertext, expectedAAD)
	if err != nil {
		return nil, fmt.Errorf("%w: authentication failed", ErrInvalidCiphertext)
	}
	return plaintext, nil
}

func newAEAD(key []byte) (cipher.AEAD, error) {
	if err := validateAESKey(key); err != nil {
		return nil, err
	}
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInvalidKey, err)
	}
	aead, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}
	return aead, nil
}

func validateAESKey(key []byte) error {
	switch len(key) {
	case 16, 24, 32:
		return nil
	default:
		return fmt.Errorf("%w: length %d", ErrInvalidKey, len(key))
	}
}

func cloneBytes(in []byte) []byte {
	if len(in) == 0 {
		return nil
	}
	out := make([]byte, len(in))
	copy(out, in)
	return out
}
