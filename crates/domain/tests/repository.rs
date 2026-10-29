use domain::RepositoryInfo;
use std::path::Path;

#[test]
fn name_is_last_path_component() {
    let repo = RepositoryInfo::new("/home/me/projects/my-repo");
    assert_eq!(repo.name(), "my-repo");
    assert_eq!(repo.root(), Path::new("/home/me/projects/my-repo"));
}

#[test]
fn name_falls_back_to_full_path_without_last_component() {
    let repo = RepositoryInfo::new("/");
    assert_eq!(repo.name(), "/");
}
