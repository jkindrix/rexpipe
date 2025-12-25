// Rust file for testing all tree-sitter scopes

// IMPORT SCOPE - use statements
use std::collections::HashMap;
use crate::utils::helper;
use super::parent_module;

// TYPE SCOPE - type annotations
type MyAlias = Vec<String>;
struct User {
    name: String,
    age: u32,
}

// FUNCTION SCOPE - function definitions
fn helper_function(x: i32) -> i32 {
    x * 2
}

impl User {
    fn new(name: String, age: u32) -> Self {
        Self { name, age }
    }

    fn greet(&self) {
        // COMMENT SCOPE
        // This is a comment mentioning helper
        println!("Hello, {}", self.name);
    }
}

// FUNCTION_CALLS SCOPE
fn main() {
    let result = helper_function(42);
    let user = User::new("Alice".to_string(), 30);
    user.greet();
    println!("Result: {}", result);
}

// MACRO SCOPE - macro invocations
macro_rules! my_macro {
    ($x:expr) => {
        println!("Macro: {}", $x);
    };
}

fn use_macros() {
    my_macro!("hello");
    println!("Using println macro");
    vec![1, 2, 3];
}

// CONTROL_FLOW SCOPE - if/for/while/match
fn control_flow_examples() {
    let x = 5;

    if x > 0 {
        println!("positive");
    } else {
        println!("non-positive");
    }

    for i in 0..10 {
        println!("{}", i);
    }

    while x > 0 {
        break;
    }

    match x {
        0 => println!("zero"),
        1..=5 => println!("small"),
        _ => println!("large"),
    }
}

// IDENTIFIERS SCOPE - variable and function names
fn identifier_examples() {
    let my_variable = 42;
    let another_var = "hello";
    let computed = my_variable + 1;
}

// TESTS SCOPE - test functions
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helper() {
        assert_eq!(helper_function(2), 4);
    }

    #[tokio::test]
    async fn test_async_helper() {
        let result = helper_function(3);
        assert_eq!(result, 6);
    }
}

// STRING SCOPE
const GREETING: &str = "hello world";
fn string_examples() {
    let s1 = "hello in string literal";
    let s2 = String::from("another hello string");
}
