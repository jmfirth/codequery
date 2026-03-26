package main

import "fmt"

// Handler processes incoming requests.
type Handler struct {
	Name string
}

// Handle processes a single request.
func (h *Handler) Handle(request string) string {
	return fmt.Sprintf("[%s] handled: %s", h.Name, request)
}

// NewHandler creates a new handler with the given name.
func NewHandler(name string) *Handler {
	return &Handler{Name: name}
}

// MaxRequestSize is the maximum allowed request size.
const MaxRequestSize = 1024
