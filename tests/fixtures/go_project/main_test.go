package main

import "testing"

func TestGreet(t *testing.T) {
	got := Greet("World")
	if got != "Hello, World!" {
		t.Errorf("Greet(World) = %s, want Hello, World!", got)
	}
}

func TestHelper(t *testing.T) {
	// just exercise helper
	helper()
}

func BenchmarkGreet(b *testing.B) {
	for i := 0; i < b.N; i++ {
		Greet("World")
	}
}
