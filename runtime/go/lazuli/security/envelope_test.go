package security

import (
	"bytes"
	"errors"
	"testing"
)

func TestEncryptDecryptRoundTrip(t *testing.T) {
	t.Parallel()
	keyring := mustStaticKeyring(t, "key-v1", map[string][]byte{
		"key-v1": testAESKey(1),
	})
	plaintext := []byte("customer tax id")
	aad := []byte("Customer.tax_id:tenant-42")

	envelope, err := Encrypt(keyring, plaintext, aad)
	if err != nil {
		t.Fatalf("Encrypt: %v", err)
	}
	if envelope.KeyID != "key-v1" {
		t.Fatalf("KeyID = %q, want key-v1", envelope.KeyID)
	}
	if len(envelope.Nonce) != 12 {
		t.Fatalf("nonce length = %d, want 12", len(envelope.Nonce))
	}
	if !bytes.Equal(envelope.AAD, aad) {
		t.Fatalf("AAD = %q, want %q", envelope.AAD, aad)
	}
	if bytes.Equal(envelope.Ciphertext, plaintext) {
		t.Fatalf("ciphertext must not equal plaintext")
	}

	aad[0] ^= 0x01
	got, err := Decrypt(keyring, envelope, []byte("Customer.tax_id:tenant-42"))
	if err != nil {
		t.Fatalf("Decrypt: %v", err)
	}
	if !bytes.Equal(got, plaintext) {
		t.Fatalf("plaintext = %q, want %q", got, plaintext)
	}
}

func TestDecryptRejectsTamperedEnvelope(t *testing.T) {
	t.Parallel()
	keyring := mustStaticKeyring(t, "key-v1", map[string][]byte{
		"key-v1": testAESKey(1),
		"key-v2": testAESKey(2),
	})
	aad := []byte("Order.card:last4")
	envelope, err := Encrypt(keyring, []byte("4242"), aad)
	if err != nil {
		t.Fatalf("Encrypt: %v", err)
	}

	tests := []struct {
		name   string
		mutate func(*Ciphertext)
	}{
		{
			name: "ciphertext",
			mutate: func(envelope *Ciphertext) {
				envelope.Ciphertext[0] ^= 0x01
			},
		},
		{
			name: "nonce",
			mutate: func(envelope *Ciphertext) {
				envelope.Nonce[0] ^= 0x01
			},
		},
		{
			name: "key id",
			mutate: func(envelope *Ciphertext) {
				envelope.KeyID = "key-v2"
			},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			tampered := cloneEnvelope(envelope)
			tt.mutate(&tampered)

			_, err := Decrypt(keyring, tampered, aad)
			if !errors.Is(err, ErrInvalidCiphertext) {
				t.Fatalf("Decrypt error = %v, want ErrInvalidCiphertext", err)
			}
		})
	}
}

func TestDecryptRejectsAADMismatch(t *testing.T) {
	t.Parallel()
	keyring := mustStaticKeyring(t, "key-v1", map[string][]byte{
		"key-v1": testAESKey(1),
	})
	aad := []byte("Invoice.total:tenant-a")
	envelope, err := Encrypt(keyring, []byte("1499"), aad)
	if err != nil {
		t.Fatalf("Encrypt: %v", err)
	}

	if _, err := Decrypt(keyring, envelope, []byte("Invoice.total:tenant-b")); !errors.Is(err, ErrAADMismatch) {
		t.Fatalf("Decrypt error = %v, want ErrAADMismatch", err)
	}

	tampered := cloneEnvelope(envelope)
	tampered.AAD[0] ^= 0x01
	if _, err := Decrypt(keyring, tampered, aad); !errors.Is(err, ErrAADMismatch) {
		t.Fatalf("Decrypt error = %v, want ErrAADMismatch", err)
	}
	if _, err := Decrypt(keyring, tampered, tampered.AAD); !errors.Is(err, ErrInvalidCiphertext) {
		t.Fatalf("Decrypt error = %v, want ErrInvalidCiphertext", err)
	}
}

func TestDecryptAfterKeyRotation(t *testing.T) {
	t.Parallel()
	oldKey := testAESKey(1)
	newKey := testAESKey(2)
	oldKeyring := mustStaticKeyring(t, "key-v1", map[string][]byte{
		"key-v1": oldKey,
	})
	aad := []byte("Profile.ssn:user-7")
	oldEnvelope, err := Encrypt(oldKeyring, []byte("111-22-3333"), aad)
	if err != nil {
		t.Fatalf("Encrypt old: %v", err)
	}

	rotatedKeyring := mustStaticKeyring(t, "key-v2", map[string][]byte{
		"key-v1": oldKey,
		"key-v2": newKey,
	})
	got, err := Decrypt(rotatedKeyring, oldEnvelope, aad)
	if err != nil {
		t.Fatalf("Decrypt old after rotation: %v", err)
	}
	if string(got) != "111-22-3333" {
		t.Fatalf("plaintext = %q, want old value", got)
	}

	newEnvelope, err := Encrypt(rotatedKeyring, []byte("999-88-7777"), aad)
	if err != nil {
		t.Fatalf("Encrypt new: %v", err)
	}
	if newEnvelope.KeyID != "key-v2" {
		t.Fatalf("new KeyID = %q, want key-v2", newEnvelope.KeyID)
	}
}

func TestStaticKeyringRejectsInvalidKeys(t *testing.T) {
	t.Parallel()
	_, err := NewStaticKeyring("key-v1", map[string][]byte{
		"key-v1": []byte("too short"),
	})
	if !errors.Is(err, ErrInvalidKey) {
		t.Fatalf("NewStaticKeyring error = %v, want ErrInvalidKey", err)
	}
}

func mustStaticKeyring(t *testing.T, currentID string, keys map[string][]byte) *StaticKeyring {
	t.Helper()
	keyring, err := NewStaticKeyring(currentID, keys)
	if err != nil {
		t.Fatalf("NewStaticKeyring: %v", err)
	}
	return keyring
}

func testAESKey(seed byte) []byte {
	key := make([]byte, 32)
	for i := range key {
		key[i] = seed + byte(i)
	}
	return key
}

func cloneEnvelope(envelope Ciphertext) Ciphertext {
	return Ciphertext{
		KeyID:      envelope.KeyID,
		Nonce:      cloneBytes(envelope.Nonce),
		AAD:        cloneBytes(envelope.AAD),
		Ciphertext: cloneBytes(envelope.Ciphertext),
	}
}
