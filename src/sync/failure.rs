/// A sync failure: the detail for a human, and the kind it was classified as.
/// Classification happens once, where the error is created, so callers branch
/// on [`Error::kind`] instead of parsing git's text again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: Failure,
    detail: String,
}

impl Error {
    /// A git failure: the detail is git's stderr, classified into a kind so
    /// callers can branch on the likely cause.
    pub fn new(detail: impl Into<String>) -> Self {
        let detail = detail.into();
        Self {
            kind: Failure::classify(&detail),
            detail,
        }
    }

    /// A non-git failure (filesystem, a process that could not be spawned, a
    /// stopped worker). Always `Other`, so a local I/O error that happens to say
    /// "permission denied" is never mistaken for a git authentication problem.
    pub fn io(detail: impl Into<String>) -> Self {
        Self {
            kind: Failure::Other,
            detail: detail.into(),
        }
    }

    pub fn kind(&self) -> Failure {
        self.kind
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

/// A short, human-readable reason a sync failed, inferred from git's stderr.
/// Used to lead with the likely cause instead of a raw error dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    NotFound,
    Auth,
    Network,
    Other,
}

impl Failure {
    pub fn classify(error: &str) -> Self {
        let error = error.to_ascii_lowercase();
        let has = |needle: &str| error.contains(needle);
        if has("repository not found")
            || has("does not appear to be a git repository")
            || (has("repository") && has("not found"))
        {
            Self::NotFound
        } else if has("authentication failed")
            || has("could not read username")
            || has("could not read password")
            || has("terminal prompts disabled")
            || has("permission denied")
            || has("403")
        {
            Self::Auth
        } else if has("could not resolve host")
            || has("could not resolve")
            || has("unable to access")
            || has("failed to connect")
            || has("timed out")
            || has("network is unreachable")
        {
            Self::Network
        } else {
            Self::Other
        }
    }

    /// A phrase naming the cause, for the `tasu remote` failure message.
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotFound => "repository not found",
            Self::Auth => "authentication failed",
            Self::Network => "can't reach the host",
            Self::Other => "sync failed",
        }
    }

    /// A phrase for the footer, paired with a `tasu sync` hint.
    pub fn footer(self) -> &'static str {
        match self {
            Self::NotFound => "remote not found",
            Self::Auth => "auth failed",
            Self::Network => "offline",
            Self::Other => "sync failed",
        }
    }

    /// One actionable line, for the `tasu remote` failure message.
    pub fn fix(self) -> &'static str {
        match self {
            Self::NotFound => {
                "check the owner/repo, or create the repository (private is fine), then retry"
            }
            Self::Auth => {
                "no stored credentials or no write access \u{2014} over HTTPS git needs a token, or use an SSH remote"
            }
            Self::Network => "check your network and the host name, then retry",
            Self::Other => "the git error is above",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Failure;

    #[test]
    fn classifies_common_git_failures() {
        assert_eq!(
            Failure::classify("remote: Repository not found."),
            Failure::NotFound
        );
        assert_eq!(
            Failure::classify(
                "fatal: could not read Username for 'https://github.com': terminal prompts disabled"
            ),
            Failure::Auth
        );
        assert_eq!(
            Failure::classify("fatal: unable to access 'x': Could not resolve host: github.com"),
            Failure::Network
        );
        assert_eq!(Failure::classify("something else"), Failure::Other);
    }

    #[test]
    fn io_errors_are_not_mistaken_for_git_auth() {
        // A local filesystem error that happens to mention "permission denied"
        // must not be classified as a git credential problem.
        let err = super::Error::io("could not create dir: Permission denied (os error 13)");
        assert_eq!(err.kind(), Failure::Other);
    }
}
