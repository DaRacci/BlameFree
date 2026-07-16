#[test]
fn test_cacheable_derive_compile_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/fixtures/01-basic-struct.rs");
    t.pass("tests/fixtures/02-cache-key-struct.rs");
    t.pass("tests/fixtures/03-cache-ref-struct.rs");
}

#[test]
fn test_cacheable_derive_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/fixtures/04-enum-fails.rs");
    t.compile_fail("tests/fixtures/05-tuple-struct-fails.rs");
}
