use thiserror::Error;

/// Why a name cannot be a branch name, following `git check-ref-format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidBranchName {
    #[error("the name is empty")]
    Empty,
    #[error("the name must not be HEAD")]
    Head,
    #[error("the name must not start with '-'")]
    LeadingDash,
    #[error("the name must not start or end with '/' or contain '//'")]
    EmptyComponent,
    #[error("no part of the name may start with '.'")]
    LeadingDot,
    #[error("no part of the name may end with '.lock'")]
    LockSuffix,
    #[error("the name must not end with '.'")]
    TrailingDot,
    #[error("the name must not contain '..'")]
    DoubleDot,
    #[error("the name must not contain '@{{' or be '@'")]
    AtBrace,
    #[error("the name must not contain {0:?}")]
    Forbidden(char),
}

/// Checks that `name` (e.g. `feature/login`) is a valid local branch name.
pub fn validate_branch_name(name: &str) -> Result<(), InvalidBranchName> {
    if name.is_empty() {
        return Err(InvalidBranchName::Empty);
    }
    if name == "HEAD" {
        return Err(InvalidBranchName::Head);
    }
    if name == "@" || name.contains("@{") {
        return Err(InvalidBranchName::AtBrace);
    }
    if name.starts_with('-') {
        return Err(InvalidBranchName::LeadingDash);
    }
    if let Some(c) = name
        .chars()
        .find(|c| c.is_ascii_control() || " ~^:?*[\\".contains(*c))
    {
        return Err(InvalidBranchName::Forbidden(c));
    }
    if name.contains("..") {
        return Err(InvalidBranchName::DoubleDot);
    }
    if name.ends_with('.') {
        return Err(InvalidBranchName::TrailingDot);
    }
    for component in name.split('/') {
        if component.is_empty() {
            return Err(InvalidBranchName::EmptyComponent);
        }
        if component.starts_with('.') {
            return Err(InvalidBranchName::LeadingDot);
        }
        if component.ends_with(".lock") {
            return Err(InvalidBranchName::LockSuffix);
        }
    }
    Ok(())
}

/// Checks that `name` (e.g. `v1.2.0`) is a valid tag name. Tags follow the
/// same rules as branches.
pub fn validate_tag_name(name: &str) -> Result<(), InvalidBranchName> {
    validate_branch_name(name)
}
