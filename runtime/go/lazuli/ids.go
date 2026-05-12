package lazuli

import (
	"crypto/rand"
	"fmt"
	"time"
)

// SortableID is a ULID-like, lexicographically sortable identifier encoded as
// 26 Crockford base32 characters. It stores a 48-bit Unix millisecond timestamp
// followed by 80 bits of entropy.
type SortableID string

const (
	// SortableIDLength is the encoded length of a SortableID.
	SortableIDLength = 26
	// SortableIDEntropyBytes is the amount of entropy encoded into a SortableID.
	SortableIDEntropyBytes = 10

	sortableIDTimestampChars = 10
	sortableIDMaxMillis      = uint64(1<<48 - 1)
	sortableIDAlphabet       = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
	sortableIDInvalidValue   = 0xff
)

var sortableIDDecode = func() [256]byte {
	var table [256]byte
	for i := range table {
		table[i] = sortableIDInvalidValue
	}
	for i := 0; i < len(sortableIDAlphabet); i++ {
		table[sortableIDAlphabet[i]] = byte(i)
	}
	return table
}()

// NewSortableID returns a fresh SortableID using the current wall clock time
// and crypto/rand entropy.
func NewSortableID() (SortableID, error) {
	return NewSortableIDAt(time.Now())
}

// NewSortableIDAt returns a fresh SortableID using t's Unix millisecond
// timestamp and crypto/rand entropy.
func NewSortableIDAt(t time.Time) (SortableID, error) {
	var entropy [SortableIDEntropyBytes]byte
	if _, err := rand.Read(entropy[:]); err != nil {
		return "", fmt.Errorf("lazuli: generate sortable id entropy: %w", err)
	}
	return NewSortableIDWithEntropy(t, entropy)
}

// NewSortableIDWithEntropy returns a SortableID for t with caller-supplied
// entropy. It is intended for deterministic tests and fixtures; production
// callers should prefer NewSortableID or NewSortableIDAt.
func NewSortableIDWithEntropy(t time.Time, entropy [SortableIDEntropyBytes]byte) (SortableID, error) {
	millis, err := sortableIDUnixMillis(t)
	if err != nil {
		return "", err
	}

	var out [SortableIDLength]byte
	encodeSortableIDTimestamp(millis, out[:sortableIDTimestampChars])
	encodeSortableIDEntropy(entropy[:], out[sortableIDTimestampChars:])
	return SortableID(string(out[:])), nil
}

// ParseSortableIDTimestamp returns the UTC timestamp encoded in id.
func ParseSortableIDTimestamp(id string) (time.Time, error) {
	millis, err := parseSortableIDMillis(id)
	if err != nil {
		return time.Time{}, err
	}
	return time.UnixMilli(int64(millis)).UTC(), nil
}

// String returns id as a plain string.
func (id SortableID) String() string {
	return string(id)
}

// Timestamp returns the UTC timestamp encoded in id.
func (id SortableID) Timestamp() (time.Time, error) {
	return ParseSortableIDTimestamp(string(id))
}

func sortableIDUnixMillis(t time.Time) (uint64, error) {
	millis := t.UTC().UnixMilli()
	if millis < 0 {
		return 0, fmt.Errorf("lazuli: sortable id timestamp before Unix epoch")
	}
	if uint64(millis) > sortableIDMaxMillis {
		return 0, fmt.Errorf("lazuli: sortable id timestamp exceeds 48 bits")
	}
	return uint64(millis), nil
}

func encodeSortableIDTimestamp(millis uint64, out []byte) {
	for i := sortableIDTimestampChars - 1; i >= 0; i-- {
		out[i] = sortableIDAlphabet[millis&0x1f]
		millis >>= 5
	}
}

func encodeSortableIDEntropy(entropy []byte, out []byte) {
	var acc uint32
	var bits uint
	n := 0
	for _, b := range entropy {
		acc = (acc << 8) | uint32(b)
		bits += 8
		for bits >= 5 {
			bits -= 5
			out[n] = sortableIDAlphabet[(acc>>bits)&0x1f]
			n++
			if bits == 0 {
				acc = 0
			} else {
				acc &= (1 << bits) - 1
			}
		}
	}
}

func parseSortableIDMillis(id string) (uint64, error) {
	if len(id) != SortableIDLength {
		return 0, fmt.Errorf("lazuli: invalid sortable id length %d", len(id))
	}

	var millis uint64
	for i := 0; i < SortableIDLength; i++ {
		value := sortableIDDecode[id[i]]
		if value == sortableIDInvalidValue {
			return 0, fmt.Errorf("lazuli: invalid sortable id character at offset %d", i)
		}
		if i == 0 && value > 7 {
			return 0, fmt.Errorf("lazuli: sortable id timestamp exceeds 48 bits")
		}
		if i < sortableIDTimestampChars {
			millis = (millis << 5) | uint64(value)
		}
	}
	return millis, nil
}
