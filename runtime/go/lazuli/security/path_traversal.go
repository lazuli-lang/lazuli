package security

import (
	"errors"
	"fmt"
	"path"
	"path/filepath"
	"strings"
)

// ErrPathTraversalRejected is returned when a user-controlled path is not a
// safe relative path. Use errors.Is to classify wrapped rejection reasons.
var ErrPathTraversalRejected = errors.New("lazuli/security: path_traversal_rejected")

// NormalizeSlashPath returns a cleaned slash-separated relative path.
//
// It normalizes backslashes to slashes, rejects null bytes, absolute paths,
// Windows drive-prefixed paths, and any parent directory segment before
// cleaning. Empty and "." paths normalize to "".
func NormalizeSlashPath(name string) (string, error) {
	name = strings.ReplaceAll(name, "\\", "/")
	if strings.Contains(name, "\x00") {
		return "", pathTraversalReject("null byte")
	}
	if hasWindowsDrivePrefix(name) {
		return "", pathTraversalReject("windows drive path")
	}
	if path.IsAbs(name) {
		return "", pathTraversalReject("absolute path")
	}
	for _, segment := range strings.Split(name, "/") {
		if segment == ".." {
			return "", pathTraversalReject("parent segment")
		}
	}

	cleaned := path.Clean(name)
	if cleaned == "." {
		return "", nil
	}
	return cleaned, nil
}

// JoinUnderRoot joins name under root after NormalizeSlashPath validation.
// The returned path is absolute and is verified to remain within root after
// filepath joining.
func JoinUnderRoot(root, name string) (string, error) {
	if root == "" {
		return "", pathTraversalReject("empty root")
	}
	if strings.Contains(root, "\x00") {
		return "", pathTraversalReject("null byte in root")
	}

	normalized, err := NormalizeSlashPath(name)
	if err != nil {
		return "", err
	}

	absoluteRoot, err := filepath.Abs(root)
	if err != nil {
		return "", err
	}
	absoluteRoot = filepath.Clean(absoluteRoot)

	joined := filepath.Join(absoluteRoot, filepath.FromSlash(normalized))
	relative, err := filepath.Rel(absoluteRoot, joined)
	if err != nil {
		return "", err
	}
	if relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) || filepath.IsAbs(relative) {
		return "", pathTraversalReject("path escapes root")
	}
	return joined, nil
}

func hasWindowsDrivePrefix(name string) bool {
	return len(name) >= 2 && isASCIILetter(name[0]) && name[1] == ':'
}

func isASCIILetter(c byte) bool {
	return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z')
}

func pathTraversalReject(reason string) error {
	return fmt.Errorf("%w: %s", ErrPathTraversalRejected, reason)
}
