use application::CredentialProvider;
use domain::{Credential, CredentialRequest};
use infra_git::{AllowedCredentials, Attempt, CredentialChain, is_encrypted};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tempfile::TempDir;

const URL: &str = "ssh://git@example.com/repo.git";
const PLAIN_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAAAAAAAAA==
-----END OPENSSH PRIVATE KEY-----
";
const ENCRYPTED_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAEbm9uZQAAAAAAAAAAAAAAAA==
-----END OPENSSH PRIVATE KEY-----
";
const ENCRYPTED_PEM: &str = "-----BEGIN RSA PRIVATE KEY-----
Proc-Type: 4,ENCRYPTED
DEK-Info: AES-128-CBC,0123456789ABCDEF

AAAA
-----END RSA PRIVATE KEY-----
";

/// Answers prompts from a queue and records what was asked.
#[derive(Default)]
struct Prompts {
    answers: Mutex<Vec<Option<Credential>>>,
    asked: Mutex<Vec<CredentialRequest>>,
}

impl Prompts {
    fn answering(answers: Vec<Option<Credential>>) -> Self {
        Self {
            answers: Mutex::new(answers),
            ..Self::default()
        }
    }

    fn asked(&self) -> Vec<CredentialRequest> {
        self.asked.lock().unwrap().clone()
    }
}

impl CredentialProvider for Prompts {
    fn credentials(&self, request: &CredentialRequest) -> Option<Credential> {
        self.asked.lock().unwrap().push(request.clone());
        let mut answers = self.answers.lock().unwrap();
        if answers.is_empty() {
            None
        } else {
            answers.remove(0)
        }
    }
}

fn ssh_dir(keys: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (name, content) in keys {
        fs::write(dir.path().join(name), content).unwrap();
    }
    dir
}

const SSH: AllowedCredentials = AllowedCredentials {
    ssh_key: true,
    user_pass: false,
    username: false,
};
const HTTPS: AllowedCredentials = AllowedCredentials {
    ssh_key: false,
    user_pass: true,
    username: false,
};

fn key(dir: &Path, name: &str, passphrase: Option<&str>) -> Attempt {
    Attempt::SshKey {
        username: "git".into(),
        key: dir.join(name),
        passphrase: passphrase.map(str::to_owned),
    }
}

#[test]
fn ssh_tries_the_agent_then_the_default_keys_in_order() {
    let dir = ssh_dir(&[
        ("id_rsa", PLAIN_KEY),
        ("id_ed25519", PLAIN_KEY),
        ("other", PLAIN_KEY),
    ]);
    let mut chain = CredentialChain::new(Some(dir.path().to_path_buf()));
    let prompts = Prompts::default();
    let mut next = || chain.next(URL, Some("git"), SSH, &prompts);

    assert_eq!(
        next(),
        Attempt::SshAgent {
            username: "git".into()
        }
    );
    assert_eq!(next(), key(dir.path(), "id_ed25519", None));
    assert_eq!(next(), key(dir.path(), "id_rsa", None));
    assert_eq!(next(), Attempt::Exhausted);
    assert!(prompts.asked().is_empty());
}

#[test]
fn ssh_without_a_user_name_in_the_url_uses_git() {
    let mut chain = CredentialChain::new(None);
    let prompts = Prompts::default();
    let username = AllowedCredentials {
        username: true,
        ..AllowedCredentials::default()
    };
    assert_eq!(
        chain.next(URL, None, username, &prompts),
        Attempt::Username("git".into())
    );
    assert_eq!(
        chain.next(URL, None, SSH, &prompts),
        Attempt::SshAgent {
            username: "git".into()
        }
    );
}

#[test]
fn an_encrypted_key_asks_for_its_passphrase() {
    let dir = ssh_dir(&[("id_ed25519", ENCRYPTED_KEY)]);
    let mut chain = CredentialChain::new(Some(dir.path().to_path_buf()));
    let prompts = Prompts::answering(vec![Some(Credential::Passphrase("secret".into()))]);
    chain.next(URL, Some("git"), SSH, &prompts);
    assert_eq!(
        chain.next(URL, Some("git"), SSH, &prompts),
        key(dir.path(), "id_ed25519", Some("secret"))
    );
    assert_eq!(
        prompts.asked(),
        [CredentialRequest::SshPassphrase {
            url: URL.into(),
            key: dir.path().join("id_ed25519")
        }]
    );
}

#[test]
fn cancelling_the_passphrase_prompt_cancels() {
    let dir = ssh_dir(&[("id_ed25519", ENCRYPTED_KEY)]);
    let mut chain = CredentialChain::new(Some(dir.path().to_path_buf()));
    let prompts = Prompts::default();
    chain.next(URL, Some("git"), SSH, &prompts);
    assert_eq!(
        chain.next(URL, Some("git"), SSH, &prompts),
        Attempt::Cancelled
    );
}

#[test]
fn https_tries_the_credential_helper_then_asks_up_to_three_times() {
    let url = "https://example.com/repo.git";
    let mut chain = CredentialChain::new(None);
    let answer = |n: u8| {
        Some(Credential::UserPass {
            username: "ann".into(),
            password: format!("try{n}"),
        })
    };
    let prompts = Prompts::answering(vec![answer(1), answer(2), answer(3)]);
    let mut next = || chain.next(url, None, HTTPS, &prompts);

    assert_eq!(next(), Attempt::CredentialHelper);
    for n in 1..=3 {
        assert_eq!(
            next(),
            Attempt::UserPass {
                username: "ann".into(),
                password: format!("try{n}")
            }
        );
    }
    assert_eq!(next(), Attempt::Exhausted);
    assert_eq!(
        prompts.asked()[0],
        CredentialRequest::UserPass {
            url: url.into(),
            username: None
        }
    );
}

#[test]
fn cancelling_the_password_prompt_cancels() {
    let mut chain = CredentialChain::new(None);
    let prompts = Prompts::default();
    chain.next("https://x", None, HTTPS, &prompts);
    assert_eq!(
        chain.next("https://x", None, HTTPS, &prompts),
        Attempt::Cancelled
    );
}

#[test]
fn nothing_allowed_means_nothing_to_try() {
    let mut chain = CredentialChain::new(None);
    let allowed = AllowedCredentials::default();
    assert_eq!(
        chain.next(URL, None, allowed, &Prompts::default()),
        Attempt::Exhausted
    );
}

#[test]
fn encrypted_keys_are_recognized() {
    let dir = ssh_dir(&[
        ("plain", PLAIN_KEY),
        ("encrypted", ENCRYPTED_KEY),
        ("pem", ENCRYPTED_PEM),
    ]);
    let path = |name: &str| -> PathBuf { dir.path().join(name) };
    assert!(!is_encrypted(&path("plain")));
    assert!(is_encrypted(&path("encrypted")));
    assert!(is_encrypted(&path("pem")));
    assert!(!is_encrypted(&path("missing")));
}

#[test]
fn secrets_stay_out_of_debug_output() {
    let attempt = Attempt::UserPass {
        username: "ann".into(),
        password: "hunter2".into(),
    };
    assert!(!format!("{attempt:?}").contains("hunter2"));
    let credential = Credential::Passphrase("hunter2".into());
    assert!(!format!("{credential:?}").contains("hunter2"));
}
