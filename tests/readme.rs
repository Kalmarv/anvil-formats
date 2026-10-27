//! The README's example is the program in examples/forge_listing.rs, which `cargo test` compiles.

#[test]
fn readme_example_is_the_checked_example() {
    let readme = include_str!("../README.md").replace("\r\n", "\n");
    let example = include_str!("../examples/forge_listing.rs").replace("\r\n", "\n");
    assert!(
        readme.contains(&format!("```rust\n{example}```")),
        "README.md's example differs from examples/forge_listing.rs"
    );
}
