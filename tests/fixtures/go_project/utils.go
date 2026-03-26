package main

import "strings"

// FormatName formats a name with proper casing.
func FormatName(first, last string) string {
	return strings.Title(first) + " " + strings.Title(last)
}

func internalFormat(s string) string {
	return strings.TrimSpace(s)
}

// Version is the application version.
const Version = "1.0.0"
