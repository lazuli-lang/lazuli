package examplesbundle

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

// ValidationConfig configures bundle validation checks that are policy-driven
// rather than required by the Entry wire format.
type ValidationConfig struct {
	// RequiredTags lists tags every entry must carry after tag normalization.
	// Empty means no tag policy beyond the base requirement that each entry has
	// at least one tag.
	RequiredTags []string
	// MaxContentTokens caps TokenishContentLength per entry. Values less than or
	// equal to zero disable the budget check.
	MaxContentTokens int
}

// Validate checks final bundle entries for required fields, matching content
// hashes, and duplicate bundle keys.
func Validate(entries []Entry) error {
	return ValidateWithConfig(entries, ValidationConfig{})
}

// ValidateWithConfig checks final bundle entries with additional bundle policy.
func ValidateWithConfig(entries []Entry, config ValidationConfig) error {
	normalizedConfig, err := examplesBundleValidateNormalizeConfig(config)
	if err != nil {
		return err
	}

	var errs []error
	for i, entry := range entries {
		if err := examplesBundleValidateEntry(entry, normalizedConfig); err != nil {
			errs = append(errs, examplesBundleValidateIndexedErrors(i, err)...)
		}
	}
	errs = append(errs, examplesBundleValidateDuplicateErrors(entries)...)
	return errors.Join(errs...)
}

// ValidateEntry checks a final bundle entry for required fields and a matching
// content hash.
func ValidateEntry(entry Entry) error {
	return ValidateEntryWithConfig(entry, ValidationConfig{})
}

// ValidateEntryWithConfig checks one final bundle entry with additional bundle
// policy. Duplicate source paths and content hashes require ValidateWithConfig.
func ValidateEntryWithConfig(entry Entry, config ValidationConfig) error {
	normalizedConfig, err := examplesBundleValidateNormalizeConfig(config)
	if err != nil {
		return err
	}
	return examplesBundleValidateEntry(entry, normalizedConfig)
}

// TokenishContentLength returns a deterministic, tokenizer-free estimate of
// how much prompt content an entry contributes. It counts word-like runs as one
// token and non-space punctuation as one token.
func TokenishContentLength(entry Entry) int {
	total := 0
	for _, value := range []string{
		entry.Name,
		entry.Intent,
		entry.SourcePath,
		entry.LZISource,
		entry.IRSnippet,
	} {
		total += examplesBundleValidateTokenishLength(value)
	}
	for _, value := range entry.Tags {
		total += examplesBundleValidateTokenishLength(value)
	}
	for _, value := range entry.CommonErrors {
		total += examplesBundleValidateTokenishLength(value)
	}
	return total
}

type examplesBundleValidationConfig struct {
	requiredTags     []string
	maxContentTokens int
}

type examplesBundleValidationFailure struct {
	index      int
	fieldOrder int
	err        error
}

func examplesBundleValidateNormalizeConfig(config ValidationConfig) (examplesBundleValidationConfig, error) {
	var errs []error
	if config.MaxContentTokens < 0 {
		errs = append(errs, invalidField("max_content_tokens", "value must be non-negative"))
	}

	requiredTags, err := normalizeStringList("required_tags", config.RequiredTags, false)
	if err != nil {
		errs = append(errs, err)
	}

	if err := errors.Join(errs...); err != nil {
		return examplesBundleValidationConfig{}, err
	}
	return examplesBundleValidationConfig{
		requiredTags:     requiredTags,
		maxContentTokens: config.MaxContentTokens,
	}, nil
}

func examplesBundleValidateEntry(entry Entry, config examplesBundleValidationConfig) error {
	normalized, err := examplesBundleValidateNormalizeEntry(entry)
	if err != nil {
		return err
	}

	var errs []error
	hash := strings.TrimSpace(entry.ContentHash)
	if hash == "" {
		errs = append(errs, invalidField("content_hash", "value is required"))
	} else if hash != normalized.ContentHash {
		errs = append(errs, invalidField("content_hash", "hash does not match lzi_source"))
	}

	if err := examplesBundleValidateRequiredTags(normalized, config.requiredTags); err != nil {
		errs = append(errs, err)
	}

	if config.maxContentTokens > 0 {
		tokens := TokenishContentLength(normalized)
		if tokens > config.maxContentTokens {
			errs = append(errs, invalidField("content_tokens", fmt.Sprintf("token-ish content length %d exceeds limit %d", tokens, config.maxContentTokens)))
		}
	}

	return errors.Join(errs...)
}

func examplesBundleValidateNormalizeEntry(entry Entry) (Entry, error) {
	return NewEntry(Example{
		Name:         entry.Name,
		Intent:       entry.Intent,
		SourcePath:   entry.SourcePath,
		Tags:         entry.Tags,
		LZISource:    entry.LZISource,
		IRSnippet:    entry.IRSnippet,
		CommonErrors: entry.CommonErrors,
	})
}

func examplesBundleValidateRequiredTags(entry Entry, requiredTags []string) error {
	if len(requiredTags) == 0 {
		return nil
	}

	present := make(map[string]struct{}, len(entry.Tags))
	for _, tag := range entry.Tags {
		present[tag] = struct{}{}
	}

	missing := make([]string, 0, len(requiredTags))
	for _, tag := range requiredTags {
		if _, ok := present[tag]; !ok {
			missing = append(missing, tag)
		}
	}
	if len(missing) == 0 {
		return nil
	}
	return invalidField("tags", "missing required tags: "+strings.Join(missing, ", "))
}

func validateUniqueBundleEntries(entries []Entry) error {
	return errors.Join(examplesBundleValidateDuplicateErrors(entries)...)
}

func examplesBundleValidateDuplicateErrors(entries []Entry) []error {
	failures := append(
		examplesBundleValidateDuplicateSourcePaths(entries),
		examplesBundleValidateDuplicateContentHashes(entries)...,
	)
	sort.SliceStable(failures, func(i, j int) bool {
		if failures[i].index != failures[j].index {
			return failures[i].index < failures[j].index
		}
		return failures[i].fieldOrder < failures[j].fieldOrder
	})

	errs := make([]error, 0, len(failures))
	for _, failure := range failures {
		errs = append(errs, failure.err)
	}
	return errs
}

func examplesBundleValidateIndexedErrors(index int, err error) []error {
	if err == nil {
		return nil
	}
	if joined, ok := err.(interface{ Unwrap() []error }); ok {
		children := joined.Unwrap()
		errs := make([]error, 0, len(children))
		for _, child := range children {
			errs = append(errs, examplesBundleValidateIndexedErrors(index, child)...)
		}
		return errs
	}
	return []error{indexedError(index, err)}
}

func examplesBundleValidateDuplicateSourcePaths(entries []Entry) []examplesBundleValidationFailure {
	const sourcePathFieldOrder = 0

	seen := make(map[string]int, len(entries))
	var failures []examplesBundleValidationFailure
	for i, entry := range entries {
		sourcePath, err := cleanSourcePath(entry.SourcePath)
		if err != nil {
			continue
		}
		if first, ok := seen[sourcePath]; ok {
			failures = append(failures, examplesBundleValidationFailure{
				index:      i,
				fieldOrder: sourcePathFieldOrder,
				err: indexedError(i, invalidField(
					"source_path",
					fmt.Sprintf("duplicate source path %q also appears at entry[%d]", sourcePath, first),
				)),
			})
			continue
		}
		seen[sourcePath] = i
	}
	return failures
}

func examplesBundleValidateDuplicateContentHashes(entries []Entry) []examplesBundleValidationFailure {
	const contentHashFieldOrder = 1

	seen := make(map[string]int, len(entries))
	var failures []examplesBundleValidationFailure
	for i, entry := range entries {
		hash := strings.TrimSpace(entry.ContentHash)
		if hash == "" {
			continue
		}
		if first, ok := seen[hash]; ok {
			failures = append(failures, examplesBundleValidationFailure{
				index:      i,
				fieldOrder: contentHashFieldOrder,
				err: indexedError(i, invalidField(
					"content_hash",
					fmt.Sprintf("duplicate content hash %q also appears at entry[%d]", hash, first),
				)),
			})
			continue
		}
		seen[hash] = i
	}
	return failures
}

func examplesBundleValidateTokenishLength(value string) int {
	tokens := 0
	inWord := false
	for _, r := range value {
		switch {
		case unicode.IsLetter(r) || unicode.IsDigit(r) || r == '_':
			if !inWord {
				tokens++
			}
			inWord = true
		case unicode.IsSpace(r):
			inWord = false
		default:
			tokens++
			inWord = false
		}
	}
	return tokens
}
