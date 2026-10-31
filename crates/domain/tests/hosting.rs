use domain::{Hoster, web_link};

fn url(remote: &str, branch: Option<&str>) -> Option<String> {
    web_link(remote, branch).map(|link| link.url)
}

#[test]
fn https_ssh_and_scp_forms_lead_to_the_same_page() {
    let page = Some("https://github.com/owner/repo".to_owned());
    for remote in [
        "https://github.com/owner/repo.git",
        "https://github.com/owner/repo",
        "https://github.com/owner/repo/",
        "https://user:token@github.com/owner/repo.git",
        "http://github.com/owner/repo.git",
        "ssh://git@github.com/owner/repo.git",
        "ssh://git@ssh.github.com:443/owner/repo.git",
        "git@github.com:owner/repo.git",
        "github.com:owner/repo",
        "git://github.com/owner/repo.git",
    ] {
        assert_eq!(url(remote, None), page, "{remote}");
    }
}

#[test]
fn every_hoster_has_its_branch_page() {
    let cases = [
        (
            "git@github.com:o/r.git",
            Hoster::GitHub,
            "https://github.com/o/r/tree/main",
        ),
        (
            "git@gitlab.com:o/r.git",
            Hoster::GitLab,
            "https://gitlab.com/o/r/-/tree/main",
        ),
        (
            "git@bitbucket.org:o/r.git",
            Hoster::Bitbucket,
            "https://bitbucket.org/o/r/src/main",
        ),
        (
            "https://codeberg.org/o/r.git",
            Hoster::Codeberg,
            "https://codeberg.org/o/r/src/branch/main",
        ),
    ];
    for (remote, hoster, page) in cases {
        let link = web_link(remote, Some("main")).unwrap();
        assert_eq!(link.hoster, hoster);
        assert_eq!(link.url, page);
    }
}

#[test]
fn gitlab_subgroups_are_kept() {
    assert_eq!(
        url("git@gitlab.com:group/sub/repo.git", None).as_deref(),
        Some("https://gitlab.com/group/sub/repo")
    );
}

#[test]
fn branch_names_are_encoded_but_keep_slashes() {
    assert_eq!(
        url("git@github.com:o/r.git", Some("feature/ä #1")).as_deref(),
        Some("https://github.com/o/r/tree/feature/%C3%A4%20%231")
    );
}

#[test]
fn unknown_hosts_and_odd_urls_have_no_page() {
    for remote in [
        "git@example.com:o/r.git",
        "https://gitea.example.org/o/r.git",
        "/home/me/repos/r.git",
        "file:///home/me/repos/r.git",
        "C:/repos/r.git",
        "https://github.com/onlyowner",
        "https://github.com/",
        "",
    ] {
        assert_eq!(url(remote, None), None, "{remote}");
    }
}

#[test]
fn hosters_have_display_names() {
    assert_eq!(Hoster::GitHub.to_string(), "GitHub");
    assert_eq!(Hoster::Codeberg.to_string(), "Codeberg");
}
