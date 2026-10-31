use domain::Commit;

/// A search of the history. Every word of the query has to match, each in
/// the summary, the author's name or email, or the start of the hash.
/// Letter case does not matter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitQuery {
    terms: Vec<String>,
}

impl CommitQuery {
    /// The query for `text`, or `None` if it has no words.
    pub fn new(text: &str) -> Option<Self> {
        let terms: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
        (!terms.is_empty()).then_some(Self { terms })
    }

    pub fn matches(&self, commit: &Commit) -> bool {
        let summary = commit.summary.to_lowercase();
        let name = commit.author.name.to_lowercase();
        let email = commit.author.email.to_lowercase();
        let hash = commit.id.to_hex();
        self.terms.iter().all(|term| {
            summary.contains(term.as_str())
                || name.contains(term.as_str())
                || email.contains(term.as_str())
                || hash.starts_with(term.as_str())
        })
    }
}
