package lazuli

import (
	"lazuli.dev/runtime/lazuli/encryption"
)

// encryptColumnValues walks the resolved binding columns and replaces
// each value whose column is declared `@cap.Encrypted` / `@cap.E2ee` on
// the resource with its AES-256-GCM ciphertext.
//
// `cols` and `values` are parallel slices: index `i` of `cols` names the
// column whose resolved binding sits at `values[i]`. The function mutates
// `values` in place so SQL placeholder ordering stays stable. Returns
// the first encryption error, or nil when every column passes (including
// the no-encrypted-fields fast path).
//
// Wire-thin: zero crypto knowledge here. The byte work happens in
// `encryption.Cipher.Encrypt`; this function is the SQL-side glue that
// threads `encryption.ForCtx` through every binding the resource flagged
// at codegen time.
func encryptColumnValues(ctx *Ctx, res *resourceErased, cols []string, values []any) error {
	if res == nil || len(res.EncryptedColumns) == 0 {
		return nil
	}
	for i, col := range cols {
		scope, ok := res.EncryptedColumns[unquoteIdent(col)]
		if !ok {
			continue
		}
		// Empty / nil bindings bypass the cipher — they round-trip as
		// SQL NULL / empty bytes and the corresponding decrypt-side
		// guard in the generated `Decrypt<Resource>` helper handles
		// the symmetric case. Mirrors the optional-field rule in
		// `docs/proposals/encryption-vocab.md` §Codegen rule 4.
		plaintext, isBytes := bindingBytes(values[i])
		if !isBytes || len(plaintext) == 0 {
			continue
		}
		cipher, err := encryption.ForCtx(ctx, scope, "")
		if err != nil {
			return err
		}
		ct, err := cipher.Encrypt(plaintext)
		if err != nil {
			return err
		}
		values[i] = ct
	}
	return nil
}

// bindingBytes normalises a resolved binding value into a byte slice
// when the column carries encryption. The two surfaces a generated
// binding might carry are `[]byte` (the canonical `lazuli.EncryptedRef`
// underlying type) and `*[]byte` (when the field is optional and the
// binding hands a pointer through). Anything else short-circuits the
// caller — column metadata claimed the column was encrypted but the
// binding holds a non-bytes value, which is a generator bug rather
// than a runtime concern.
func bindingBytes(v any) ([]byte, bool) {
	switch b := v.(type) {
	case []byte:
		return b, true
	case *[]byte:
		if b == nil {
			return nil, true
		}
		return *b, true
	}
	return nil, false
}

// decryptScannedRow invokes the resource's typed decrypt callback on a
// freshly-scanned row, if the resource declares one. `row` must already
// be a pointer to the scanned value (`&out` style); the callback type
// asserts to the typed pointer the generated helper expects.
//
// Resources without `@cap.Encrypted` fields (or whose encrypted fields
// are all `@cap.E2ee`, see `Decrypt<Resource>`'s skip rule) carry a nil
// callback; the function returns immediately in that case.
func decryptScannedRow(ctx *Ctx, res *resourceErased, row any) error {
	if res == nil || res.Decrypt == nil {
		return nil
	}
	return res.Decrypt(ctx, row)
}

// unquoteIdent reverses `quoteIdent` so the column-name lookup against
// `EncryptedColumns` (keyed by raw column names) matches. Doubles as a
// no-op when the caller already passed the unquoted form.
func unquoteIdent(name string) string {
	if len(name) >= 2 && name[0] == '"' && name[len(name)-1] == '"' {
		return name[1 : len(name)-1]
	}
	return name
}
