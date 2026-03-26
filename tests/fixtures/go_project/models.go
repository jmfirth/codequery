package main

// User represents a user in the system.
type User struct {
	Name string
	Age  int
}

type config struct {
	debug bool
}

// Stringer can produce a string representation.
type Stringer interface {
	String() string
}

type validator interface {
	Validate() error
}

// FullName returns the user's full name.
func (u *User) FullName() string {
	return u.Name
}

func (u *User) greetSelf() string {
	return "Hello, " + u.Name
}

// GetAge returns the user's age.
func (u User) GetAge() int {
	return u.Age
}
