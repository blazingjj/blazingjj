/*! Turning a git remote URL into the page to open in a browser to put
up a pull request against it.

Only GitHub is understood for now. A remote on another host is left for
the caller to report as such, rather than guessed at.
*/

/// The URL to open in a browser to put up a pull request for `bookmark`
/// against `remote_url`, or `None` when the remote is not one this
/// knows how to build that URL for, or `bookmark` is not safe to put
/// in one unescaped.
pub fn pull_request_url(remote_url: &str, bookmark: &str) -> Option<String> {
    let repo = github_repo(remote_url)?;
    if !is_safe_path_segment(&repo) || !is_safe_path_segment(bookmark) {
        return None;
    }

    Some(format!("https://github.com/{repo}/pull/new/{bookmark}"))
}

/// Whether `segment` is safe to put unescaped both in a URL path and,
/// on Windows, in the command line `cmd /C start` re-parses: a bookmark
/// pushed by someone else is otherwise free to hold shell metacharacters
/// a git ref name allows but a shell does not, like `&` or `|`.
fn is_safe_path_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
}

/// The `owner/repo` a GitHub remote URL names, in whichever form jj
/// reports it: `git@github.com:owner/repo.git`,
/// `ssh://git@github.com/owner/repo.git` or
/// `https://github.com/owner/repo.git`.
fn github_repo(remote_url: &str) -> Option<String> {
    let rest = remote_url
        .strip_prefix("git@github.com:")
        .or_else(|| remote_url.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote_url.strip_prefix("https://github.com/"))
        .or_else(|| remote_url.strip_prefix("http://github.com/"))?;

    Some(rest.strip_suffix(".git").unwrap_or(rest).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_request_url_from_ssh_shorthand() {
        assert_eq!(
            pull_request_url("git@github.com:owner/repo.git", "feature"),
            Some("https://github.com/owner/repo/pull/new/feature".to_owned())
        );
    }

    #[test]
    fn pull_request_url_from_ssh_url() {
        assert_eq!(
            pull_request_url("ssh://git@github.com/owner/repo.git", "feature"),
            Some("https://github.com/owner/repo/pull/new/feature".to_owned())
        );
    }

    #[test]
    fn pull_request_url_from_https() {
        assert_eq!(
            pull_request_url("https://github.com/owner/repo.git", "feature"),
            Some("https://github.com/owner/repo/pull/new/feature".to_owned())
        );
    }

    #[test]
    fn pull_request_url_from_https_without_git_suffix() {
        assert_eq!(
            pull_request_url("https://github.com/owner/repo", "feature"),
            Some("https://github.com/owner/repo/pull/new/feature".to_owned())
        );
    }

    #[test]
    fn pull_request_url_is_none_for_an_unknown_host() {
        assert_eq!(
            pull_request_url("git@gitlab.com:owner/repo.git", "feature"),
            None
        );
    }

    #[test]
    fn pull_request_url_is_none_for_a_bookmark_holding_shell_metacharacters() {
        assert_eq!(
            pull_request_url("git@github.com:owner/repo.git", "feature; rm -rf /"),
            None
        );
    }
}
