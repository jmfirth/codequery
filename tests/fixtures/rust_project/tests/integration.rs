use fixture_project::greet;

#[test]
fn test_greet() {
    assert_eq!(greet("world"), "Hello, world!");
}

#[test]
fn test_greet_empty() {
    assert_eq!(greet(""), "Hello, !");
}
