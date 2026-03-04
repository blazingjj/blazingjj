# BlazingJJ Governance

## Principles

We aim to be friendly and welcoming to users and contributors. We value
collaboration, so PRs and significant changes should be discussed and reviewed
before merging. We also try to keep the project backlog healthy — it's better
to close or reject stale issues and PRs than to let them accumulate
indefinitely.

## Roles

### Users
Anyone using BlazingJJ. Can open issues and participate in discussions.

### Contributors
Anyone who has had a pull request merged. Can do everything Users can, and can
also submit PRs and participate in project discussions.

### Collaborators
Experienced contributors granted GitHub Collaborator status. Can push to
non-protected branches, manage issues and labels, and provide binding PR
reviews. Cannot merge into `main` directly.

The current list of Collaborators can be found in the [project's GitHub member
list](https://github.com/orgs/blazingjj/people).

### Maintainers
Trusted contributors with merge access. All Maintainers are members of
@blazingjj/core, which can be mentioned to notify the whole team.

Maintainers can:
- Merge pull requests
- Triage and close issues
- Vote on project decisions
- Invite new Collaborators and Maintainers

## Decision Making

Day-to-day decisions (bug fixes, minor features, docs) are made by any
Maintainer via normal PR review and merge.

Significant decisions (breaking changes, new dependencies, major features,
governance changes) require **lazy consensus**: a proposal is posted in GitHub
Discussions and is approved after 7 days with no objections from any
Maintainer. Any Maintainer can call for an explicit vote, which passes by
simple majority.

Deadlocks are resolved by the project lead (@dotdash).

## Contributing

1. Fork the repo and open a pull request against `main`.
2. PRs require at least one Maintainer approval before merging. Due to the
   current small team size, Maintainers may use their judgement to merge
   straightforward PRs without a formal review.
3. Follow the coding style and ensure tests pass.

## Path to Maintainer

A Contributor may be nominated for **Collaborator** status by an existing
Maintainer after demonstrating:
- A sustained record of quality contributions (typically 5+ merged PRs)
- Helpful participation in issues and reviews
- Familiarity with the project's goals and codebase

A Collaborator may be nominated for **Maintainer** status after demonstrating
continued reliability, good judgment in reviews, and active involvement in
project direction.

Nomination is posted in a private Maintainer discussion. Approval requires
unanimous agreement from current Maintainers. The nominee is then invited and
added to this file.

## Number of Maintainers

The project should always have enough Maintainers to keep things moving. With
only one, any absence can stall the project entirely. Two is workable but still
fragile — if one is unavailable, the other may have to self-approve PRs. Three
or more is the healthy target: reviews can proceed normally even if someone is
away, and decisions are less likely to deadlock. If the number of Maintainers
drops below that, the team should prioritize nominating new ones to get back to
a healthy state.

## Stepping Down & Removal

Maintainers may step down at any time by notifying the team. A Maintainer who
is inactive for 6+ months or acts against the project's interests may be
removed by unanimous agreement of the remaining Maintainers.
