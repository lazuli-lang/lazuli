package security

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path"
	"strings"
	"unicode"
	"unicode/utf8"
)

var (
	// ErrInvalidFileServePath is returned when a requested file path is empty,
	// absolute, contains traversal, or otherwise cannot be safely opened from a
	// filesystem root.
	ErrInvalidFileServePath = errors.New("lazuli/security: invalid_fileserve_path")
	// ErrFileServeDotfileDenied is returned when the cleaned path contains a
	// dot-prefixed path segment and the dotfile policy denies dotfiles.
	ErrFileServeDotfileDenied = errors.New("lazuli/security: fileserve_dotfile_denied")
	// ErrFileServeExtensionDenied is returned when an extension allowlist is set
	// and the cleaned path's final extension is not allowed.
	ErrFileServeExtensionDenied = errors.New("lazuli/security: fileserve_extension_denied")
	// ErrInvalidFileServeOptions is returned when file serving options contain
	// an unknown dotfile policy or malformed extension allowlist entry.
	ErrInvalidFileServeOptions = errors.New("lazuli/security: invalid_fileserve_options")
	// ErrFileServeRootRequired is returned when OpenFileFromRoot is called
	// without a filesystem root.
	ErrFileServeRootRequired = errors.New("lazuli/security: fileserve_root_required")
)

// DotfilePolicy controls whether dot-prefixed path segments are accepted.
type DotfilePolicy int

const (
	// DenyDotfiles rejects paths such as ".env", ".git/config", and
	// "assets/.manifest.json". This is the zero-value policy.
	DenyDotfiles DotfilePolicy = iota
	// AllowDotfiles permits dot-prefixed path segments.
	AllowDotfiles
)

// FileServeOptions configures CleanFileServePath and OpenFileFromRoot.
type FileServeOptions struct {
	// AllowedExtensions optionally lists allowed final file extensions, such as
	// ".css" or ".png". Empty allows all extensions. Matching is
	// case-insensitive.
	AllowedExtensions []string

	// Dotfiles controls whether dot-prefixed path segments are allowed.
	Dotfiles DotfilePolicy
}

// CleanFileServePath returns a slash-separated relative path safe to pass to an
// fs.FS rooted at static assets. Leading slashes are accepted for URL paths, but
// the returned path is always relative. Traversal, backslashes, drive paths,
// query strings, fragments, control characters, and empty paths are rejected.
func CleanFileServePath(name string, options FileServeOptions) (string, error) {
	allowedExtensions, err := normalizeFileServeExtensions(options.AllowedExtensions)
	if err != nil {
		return "", err
	}
	if options.Dotfiles != DenyDotfiles && options.Dotfiles != AllowDotfiles {
		return "", fmt.Errorf("%w: unknown dotfile policy %d", ErrInvalidFileServeOptions, options.Dotfiles)
	}

	cleaned, err := cleanFileServePath(name)
	if err != nil {
		return "", err
	}
	if options.Dotfiles == DenyDotfiles && fileServeHasDotfile(cleaned) {
		return "", fmt.Errorf("%w: %s", ErrFileServeDotfileDenied, cleaned)
	}
	if len(allowedExtensions) > 0 {
		ext := strings.ToLower(path.Ext(cleaned))
		if _, ok := allowedExtensions[ext]; !ok {
			return "", fmt.Errorf("%w: %s", ErrFileServeExtensionDenied, cleaned)
		}
	}
	return cleaned, nil
}

// OpenFileFromRoot cleans name with CleanFileServePath, then opens it from root
// using os.DirFS. The returned path is the cleaned relative path used for the
// open. Directories are treated as missing files so callers do not accidentally
// expose directory listings.
//
// os.DirFS does not sandbox symlinks. Do not place untrusted symlinks under
// roots served through this helper.
func OpenFileFromRoot(root, name string, options FileServeOptions) (fs.File, string, error) {
	if strings.TrimSpace(root) == "" {
		return nil, "", ErrFileServeRootRequired
	}
	cleaned, err := CleanFileServePath(name, options)
	if err != nil {
		return nil, "", err
	}

	file, err := os.DirFS(root).Open(cleaned)
	if err != nil {
		return nil, cleaned, err
	}
	info, err := file.Stat()
	if err != nil {
		_ = file.Close()
		return nil, cleaned, err
	}
	if info.IsDir() {
		_ = file.Close()
		return nil, cleaned, fs.ErrNotExist
	}
	return file, cleaned, nil
}

func cleanFileServePath(name string) (string, error) {
	switch {
	case name == "":
		return "", fmt.Errorf("%w: empty path", ErrInvalidFileServePath)
	case strings.HasPrefix(name, "//"):
		return "", fmt.Errorf("%w: absolute path", ErrInvalidFileServePath)
	case strings.ContainsAny(name, "\x00\\:?#"):
		return "", fmt.Errorf("%w: disallowed character", ErrInvalidFileServePath)
	case !utf8.ValidString(name):
		return "", fmt.Errorf("%w: invalid utf-8", ErrInvalidFileServePath)
	}

	for _, r := range name {
		if unicode.IsControl(r) {
			return "", fmt.Errorf("%w: control character", ErrInvalidFileServePath)
		}
	}
	for _, segment := range strings.Split(name, "/") {
		if segment == ".." {
			return "", fmt.Errorf("%w: traversal", ErrInvalidFileServePath)
		}
	}

	cleaned := strings.TrimPrefix(path.Clean("/"+name), "/")
	if cleaned == "" || cleaned == "." || !fs.ValidPath(cleaned) {
		return "", fmt.Errorf("%w: %s", ErrInvalidFileServePath, name)
	}
	return cleaned, nil
}

func normalizeFileServeExtensions(extensions []string) (map[string]struct{}, error) {
	if len(extensions) == 0 {
		return nil, nil
	}

	allowed := make(map[string]struct{}, len(extensions))
	for _, ext := range extensions {
		ext = strings.TrimSpace(ext)
		if ext == "" || ext == "." || strings.ContainsAny(ext, "\x00/\\:?#") || !strings.HasPrefix(ext, ".") {
			return nil, fmt.Errorf("%w: invalid extension %q", ErrInvalidFileServeOptions, ext)
		}
		for _, r := range ext {
			if unicode.IsControl(r) {
				return nil, fmt.Errorf("%w: invalid extension %q", ErrInvalidFileServeOptions, ext)
			}
		}
		if ext != path.Ext("file"+ext) {
			return nil, fmt.Errorf("%w: invalid extension %q", ErrInvalidFileServeOptions, ext)
		}
		allowed[strings.ToLower(ext)] = struct{}{}
	}
	return allowed, nil
}

func fileServeHasDotfile(name string) bool {
	for _, segment := range strings.Split(name, "/") {
		if strings.HasPrefix(segment, ".") {
			return true
		}
	}
	return false
}
