"""Respostas de exemplo das APIs de pull requests, usadas pelos testes dos plugins.

Cada função devolve um dicionário novo, para que os testes possam alterá-lo.
"""

VIEWER_LOGIN = "eu"
OTHER_LOGIN = "outra"


def github_response():
    return {
        "data": {
            "viewer": {"login": VIEWER_LOGIN},
            "review": {
                "nodes": [
                    _github_pull(10, "acme/api", "2026-10-10T08:00:00Z",
                                 reviews=[(OTHER_LOGIN, "COMMENTED", "2026-10-12T08:00:00Z")],
                                 review_decision="REVIEW_REQUIRED"),
                    _github_pull(11, "acme/api", "2026-10-11T08:00:00Z",
                                 reviews=[(VIEWER_LOGIN, "CHANGES_REQUESTED", "2026-10-20T10:00:00Z")],
                                 last_commit_at="2026-10-21T09:00:00Z"),
                    _github_pull(12, "acme/web", "2026-10-12T08:00:00Z",
                                 reviews=[(VIEWER_LOGIN, "APPROVED", "2026-10-20T10:00:00Z")],
                                 last_commit_at="2026-10-19T09:00:00Z"),
                    _github_pull(13, "acme/api", "2026-10-13T08:00:00Z", is_draft=True),
                ],
            },
            "reviewed": {
                "nodes": [
                    _github_pull(11, "acme/api", "2026-10-11T08:00:00Z",
                                 reviews=[(VIEWER_LOGIN, "CHANGES_REQUESTED", "2026-10-20T10:00:00Z")],
                                 last_commit_at="2026-10-21T09:00:00Z"),
                    _github_pull(14, "acme/lib", "2026-10-14T08:00:00Z",
                                 reviews=[(VIEWER_LOGIN, "CHANGES_REQUESTED", "2026-10-21T10:00:00Z")],
                                 last_commit_at="2026-10-20T10:00:00Z"),
                    _github_pull(15, "acme/lib", "2026-10-15T08:00:00Z",
                                 reviews=[(VIEWER_LOGIN, "COMMENTED", "2026-10-21T10:00:00Z")]),
                ],
            },
            "mine": {
                "nodes": [
                    _github_pull(20, "acme/api", "2026-10-16T08:00:00Z", mergeable="CONFLICTING"),
                    _github_pull(21, "acme/api", "2026-10-17T08:00:00Z",
                                 review_decision="CHANGES_REQUESTED"),
                    _github_pull(22, "acme/api", "2026-10-18T08:00:00Z", review_decision="APPROVED"),
                    _github_pull(23, "acme/api", "2026-10-19T08:00:00Z",
                                 review_decision="REVIEW_REQUIRED"),
                    _github_pull(24, "acme/api", "2026-10-19T12:00:00Z", author=None,
                                 review_decision="REVIEW_REQUIRED"),
                ],
            },
        }
    }


def _github_pull(number, repository, created_at, *, author="ana", is_draft=False,
                 mergeable="MERGEABLE", review_decision=None, reviews=(),
                 last_commit_at="2026-10-09T09:00:00Z"):
    return {
        "number": number,
        "title": f"Título do PR {number}",
        "url": f"https://github.com/{repository}/pull/{number}",
        "createdAt": created_at,
        "isDraft": is_draft,
        "author": {"login": author} if author is not None else None,
        "repository": {"nameWithOwner": repository},
        "baseRefName": "main",
        "headRefOid": f"sha{number}",
        "mergeable": mergeable,
        "reviewDecision": review_decision,
        "commits": {"nodes": [{"commit": {"committedDate": last_commit_at}}]},
        "latestReviews": {
            "nodes": [_github_review(login, state, submitted_at) for login, state, submitted_at in reviews],
        },
    }


def _github_review(login, state, submitted_at):
    return {"author": {"login": login}, "state": state, "submittedAt": submitted_at}


AZURE_VIEWER_ID = "me-1"
AZURE_OTHER_ID = "outra-2"


def azure_connection_data():
    return {"authenticatedUser": {"id": AZURE_VIEWER_ID}}


def azure_review_page():
    return [
        azure_pull_request(10, created="2026-10-10T08:00:00Z", reviewers=[(AZURE_OTHER_ID, 0)]),
        azure_pull_request(11, created="2026-10-11T08:00:00Z", reviewers=[(AZURE_VIEWER_ID, -5)]),
        azure_pull_request(12, created="2026-10-12T08:00:00Z", reviewers=[(AZURE_VIEWER_ID, 10)]),
        azure_pull_request(13, created="2026-10-13T08:00:00Z", is_draft=True),
        azure_pull_request(14, created="2026-10-14T08:00:00Z", creator=AZURE_VIEWER_ID),
        azure_pull_request(15, project="Outro", created="2026-10-15T08:00:00Z"),
    ]


def azure_mine_page():
    return [
        azure_pull_request(20, created="2026-10-16T08:00:00Z", creator=AZURE_VIEWER_ID,
                           merge_status="conflicts"),
        azure_pull_request(21, created="2026-10-17T08:00:00Z", creator=AZURE_VIEWER_ID,
                           reviewers=[(AZURE_OTHER_ID, -10)]),
        azure_pull_request(22, created="2026-10-18T08:00:00Z", creator=AZURE_VIEWER_ID,
                           reviewers=[(AZURE_OTHER_ID, 10)]),
        azure_pull_request(23, created="2026-10-19T08:00:00Z", creator=AZURE_VIEWER_ID),
    ]


def azure_pull_request(number, project="Proj", repository="api", created="2026-10-10T08:00:00Z", *,
                       creator=AZURE_OTHER_ID, is_draft=False, merge_status="succeeded", reviewers=()):
    return {
        "pullRequestId": number,
        "title": f"Título do PR {number}",
        "isDraft": is_draft,
        "creationDate": created,
        "createdBy": {"id": creator, "displayName": "Ana"},
        "repository": {"name": repository, "project": {"name": project}},
        "targetRefName": "refs/heads/main",
        "mergeStatus": merge_status,
        "lastMergeSourceCommit": {"commitId": f"c{number}"},
        "reviewers": [{"id": reviewer_id, "vote": vote} for reviewer_id, vote in reviewers],
    }


def gitlab_response():
    return {
        "data": {
            "currentUser": {
                "username": VIEWER_LOGIN,
                "reviewRequestedMergeRequests": {
                    "nodes": [
                        _gitlab_merge_request(10),
                        _gitlab_merge_request(11, reviewers=[(VIEWER_LOGIN, "REQUESTED_CHANGES")]),
                        _gitlab_merge_request(12, reviewers=[(VIEWER_LOGIN, "APPROVED")]),
                        _gitlab_merge_request(13, draft=True),
                    ],
                },
                "authoredMergeRequests": {
                    "nodes": [
                        _gitlab_merge_request(20, author=VIEWER_LOGIN, conflicts=True),
                        _gitlab_merge_request(21, author=VIEWER_LOGIN,
                                              reviewers=[(OTHER_LOGIN, "REQUESTED_CHANGES")]),
                        _gitlab_merge_request(22, author=VIEWER_LOGIN,
                                              reviewers=[(OTHER_LOGIN, "APPROVED")]),
                        _gitlab_merge_request(23, author=VIEWER_LOGIN),
                    ],
                },
            }
        }
    }


def _gitlab_merge_request(number, *, author="ana", draft=False, conflicts=False, reviewers=()):
    return {
        "iid": str(number),
        "title": f"Título do MR {number}",
        "webUrl": f"https://gitlab.com/grupo/api/-/merge_requests/{number}",
        "createdAt": f"2026-10-{number:02d}T08:00:00Z",
        "draft": draft,
        "author": {"username": author},
        "project": {"fullPath": "grupo/api"},
        "targetBranch": "main",
        "diffHeadSha": f"sha{number}",
        "conflicts": conflicts,
        "reviewers": {
            "nodes": [
                {"username": login, "mergeRequestInteraction": {"reviewState": state}}
                for login, state in reviewers
            ],
        },
    }
