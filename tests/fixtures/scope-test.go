// Go file for testing tree-sitter scopes
package main

import (
	"fmt"
	"testing"
)

// COMMENT: hello in comment
const GREETING = "hello in string"

type User struct {
	Name string
	Age  int
}

func helperFunction(x int) int {
	return x * 2
}

func (u *User) Greet() {
	// Comment mentioning hello
	fmt.Printf("Hello, %s\n", u.Name)
}

func main() {
	result := helperFunction(42)
	user := User{Name: "Alice", Age: 30}
	user.Greet()
	fmt.Println("hello from main")
}

// Go test functions
func TestHelper(t *testing.T) {
	result := helperFunction(2)
	if result != 4 {
		t.Errorf("Expected 4, got %d", result)
	}
}

func BenchmarkHelper(b *testing.B) {
	for i := 0; i < b.N; i++ {
		helperFunction(i)
	}
}

func ExampleHello() {
	fmt.Println("hello example")
	// Output: hello example
}
