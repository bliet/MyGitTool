use domain::{InvalidBranchName, validate_branch_name};

#[test]
fn ordinary_names_are_valid() {
    for name in [
        "main",
        "feature/login",
        "fix-123",
        "user/ann/wip",
        "v1.2",
        "ümlaut",
    ] {
        assert_eq!(validate_branch_name(name), Ok(()), "{name}");
    }
}

#[test]
fn names_that_git_rejects_are_invalid() {
    use InvalidBranchName::*;
    let cases = [
        ("", Empty),
        ("HEAD", Head),
        ("-x", LeadingDash),
        ("a b", Forbidden(' ')),
        ("a~1", Forbidden('~')),
        ("a^", Forbidden('^')),
        ("a:b", Forbidden(':')),
        ("a?", Forbidden('?')),
        ("a*", Forbidden('*')),
        ("a[b", Forbidden('[')),
        ("a\\b", Forbidden('\\')),
        ("a\tb", Forbidden('\t')),
        ("a..b", DoubleDot),
        ("a.", TrailingDot),
        ("/a", EmptyComponent),
        ("a/", EmptyComponent),
        ("a//b", EmptyComponent),
        (".a", LeadingDot),
        ("a/.b", LeadingDot),
        ("a.lock", LockSuffix),
        ("a.lock/b", LockSuffix),
        ("@", AtBrace),
        ("a@{1}", AtBrace),
    ];
    for (name, reason) in cases {
        assert_eq!(validate_branch_name(name), Err(reason), "{name:?}");
    }
}

#[test]
fn reasons_read_as_sentences() {
    assert_eq!(
        InvalidBranchName::Forbidden(' ').to_string(),
        "the name must not contain ' '"
    );
}
