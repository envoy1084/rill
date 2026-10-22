"""The scenarios of sdks/conformance/README.md, against the server at
RILL_ENDPOINT (``rill dev``); skipped when it is unset."""

from __future__ import annotations

import os
import secrets
import time

import pytest
from connectrpc.code import Code
from connectrpc.errors import ConnectError

from rill.v1 import (
    changes_pb2,
    content_pb2,
    landing_pb2,
    policy_pb2,
    repos_pb2,
    review_pb2,
    types_pb2,
)
from rill_sdk import connect, is_reason, names, retry

ENDPOINT = os.environ.get("RILL_ENDPOINT")
needs_server = pytest.mark.skipif(not ENDPOINT, reason="RILL_ENDPOINT is not set")


def setup():
    client = connect(ENDPOINT)
    repo_id = f"conf-{secrets.token_hex(4)}"
    client.repos.create_repo(
        repos_pb2.CreateRepoRequest(request_id=names.request_id(), parent=names.org(), repo_id=repo_id)
    )
    return client, names.repo(repo_id)


def set_file(path: str, text: str) -> changes_pb2.Edit:
    return changes_pb2.Edit(
        path=path,
        set_content=changes_pb2.SetContent(
            content=types_pb2.ContentRef(inline=text.encode()),
            kind=types_pb2.FILE_KIND_REGULAR,
        ),
    )


def on_revision(revision: str) -> changes_pb2.EditBase:
    return changes_pb2.EditBase(revision=revision)


@needs_server
def test_resources_and_errors():
    client, repo = setup()
    got = client.repos.get_repo(repos_pb2.GetRepoRequest(name=repo))
    assert got.repo.name == repo
    with pytest.raises(ConnectError) as err:
        client.repos.get_repo(repos_pb2.GetRepoRequest(name=names.repo("absent")))
    assert err.value.code == Code.NOT_FOUND
    assert is_reason(err.value, "NOT_FOUND")


@needs_server
def test_writes_read_back_and_retries_are_idempotent():
    client, repo = setup()
    request_id = names.request_id()

    def write(message: str):
        return client.changes.apply_edits(
            changes_pb2.ApplyEditsRequest(
                request_id=request_id,
                parent=repo,
                base=on_revision("trunk"),
                message=message,
                edits=[set_file("hello.txt", "hello\n")],
            )
        )

    first = write("hello")
    revision = first.info.hash.hex()
    file = client.content.read_file(content_pb2.ReadFileRequest(repo=repo, revision=revision, path="hello.txt"))
    assert file.data == b"hello\n"
    listing = client.content.list_directory(content_pb2.ListDirectoryRequest(repo=repo, revision=revision))
    assert [e.name for e in listing.entries] == ["hello.txt"]
    assert write("hello").info.hash == first.info.hash
    with pytest.raises(ConnectError) as err:
        write("another")
    assert err.value.code == Code.INVALID_ARGUMENT
    assert is_reason(err.value, "REQUEST_ID_REUSED")


@needs_server
def test_histories_page():
    client, repo = setup()
    first = client.changes.apply_edits(
        changes_pb2.ApplyEditsRequest(
            parent=repo, base=on_revision("trunk"), message="one", edits=[set_file("a.txt", "1\n")]
        )
    )
    second = client.changes.apply_edits(
        changes_pb2.ApplyEditsRequest(
            parent=repo,
            base=on_revision(first.info.hash.hex()),
            message="two",
            edits=[set_file("a.txt", "2\n")],
        )
    )
    request = content_pb2.ListHistoryRequest(repo=repo, revision=second.info.hash.hex(), page_size=1)
    page = client.content.list_history(request)
    assert len(page.versions) == 1 and page.next_page_token
    request.page_token = page.next_page_token
    rest = client.content.list_history(request)
    assert rest.versions[0].hash == first.info.hash
    assert rest.next_page_token == ""


@needs_server
def test_a_reviewed_change_lands_and_filters_find_it():
    client, repo = setup()
    change = client.changes.apply_edits(
        changes_pb2.ApplyEditsRequest(
            parent=repo, base=on_revision("trunk"), message="hello", edits=[set_file("hello.txt", "hello\n")]
        )
    ).info.change_id
    client.reviews.submit_change(review_pb2.SubmitChangeRequest(change=names.change(repo, change)))
    for label, value in (("Verified", 1), ("Review", 2)):
        client.reviews.vote(review_pb2.VoteRequest(review=names.review(repo, change), label=label, value=value))
    land = retry(lambda: client.landing.request_land(landing_pb2.RequestLandRequest(change=names.change(repo, change))))
    deadline = time.monotonic() + 30
    while True:
        got = client.landing.get_land_request(landing_pb2.GetLandRequestRequest(name=land.land_request.name))
        if got.land_request.status == landing_pb2.LAND_STATUS_LANDED:
            assert got.land_request.trunk_position == 1
            break
        assert time.monotonic() < deadline, "did not land"
        time.sleep(0.1)
    head = client.repos.get_trunk_position(repos_pb2.GetTrunkPositionRequest(name=names.trunk_position(repo, "head")))
    assert head.landing.position == 1
    landed = client.reviews.list_reviews(review_pb2.ListReviewsRequest(parent=repo, filter='status = "landed"'))
    assert [r.change_id for r in landed.reviews] == [change]
    with pytest.raises(ConnectError) as err:
        client.reviews.list_reviews(review_pb2.ListReviewsRequest(parent=repo, filter='colour = "red"'))
    assert err.value.code == Code.INVALID_ARGUMENT
    assert is_reason(err.value, "INVALID_FILTER")


@needs_server
def test_etags_guard_updates():
    client, repo = setup()
    policy = client.policies.get_policy(policy_pb2.GetPolicyRequest(name=names.policy(repo, "submit"))).policy
    assert policy.etag

    def update(etag: str):
        return client.policies.update_policy(
            policy_pb2.UpdatePolicyRequest(policy=policy_pb2.Policy(name=policy.name, text=policy.text), etag=etag)
        )

    with pytest.raises(ConnectError) as err:
        update("stale")
    assert err.value.code == Code.ABORTED
    assert is_reason(err.value, "ETAG_MISMATCH")
    update(policy.etag)


def test_retry_gives_up_on_errors_that_are_not_transient():
    calls = []

    def call():
        calls.append(1)
        raise ConnectError(Code.NOT_FOUND, "gone")

    with pytest.raises(ConnectError) as err:
        retry(call)
    assert err.value.code == Code.NOT_FOUND
    assert len(calls) == 1
