package main

import "fmt"

// MaxRetries is the maximum number of retries.
const MaxRetries = 3

const minRetries = 1

var GlobalCounter int

var localFlag bool

// Greet returns a greeting for the given name.
func Greet(name string) string {
	return fmt.Sprintf("Hello, %s!", name)
}

func helper() {
	fmt.Println("helper")
}

// UserID is an alias for int.
type UserID = int

func main() {
	fmt.Println(Greet("World"))
}
