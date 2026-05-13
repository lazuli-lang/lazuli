package featureflags

import "unicode"

func hasSpaceOrControl(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func hasControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func boolSummary(value bool) string {
	if value {
		return "true"
	}
	return "false"
}
