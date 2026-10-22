# SDK conformance

Every SDK runs the same scenarios against a fresh dev server (`rill dev`,
or the Rust SDK's in-process `testing::DevServer`). Each scenario uses its
own repository, `conf-<random>`, in the default org, so the scenarios run
in any order. A scenario passes when every step below holds.

| # | Scenario | Steps |
| --- | --- | --- |
| 1 | Resources and errors | `CreateRepo(parent "orgs/default", repo_id)`; `GetRepo(name)` returns the same `name`; `GetRepo` of a repository that does not exist fails `NOT_FOUND` with `ErrorInfo{reason: "NOT_FOUND", domain: "rill.dev"}` |
| 2 | Write and read | `ApplyEdits` on revision `trunk` setting `hello.txt` to `hello\n` returns `info.hash`; `ReadFile(revision = that hash in hex, path)` returns the bytes; `ListDirectory` at the same revision lists `hello.txt` |
| 3 | Idempotency | the same `ApplyEdits` again with the same `request_id` returns the same `info.hash`; the same `request_id` with another message fails `INVALID_ARGUMENT` with reason `REQUEST_ID_REUSED` |
| 4 | Pages | a second `ApplyEdits` on revision = the first hash; `ListHistory(revision = the second hash, page_size 1)` returns one version and a `next_page_token`; the next page returns the other version and an empty token |
| 5 | Landing | `SubmitChange(change)`; `Vote(review, "Verified", 1)` and `Vote(review, "Review", 2)`; `RequestLand(change)`; `GetLandRequest(name)` reaches `LAND_STATUS_LANDED` with `trunk_position` 1 within 30 s; `GetTrunkPosition(".../trunk/positions/head")` is position 1 |
| 6 | Filters | `ListReviews(parent, filter 'status = "landed"')` includes the landed review; a filter on a field lists do not have fails `INVALID_ARGUMENT` with reason `INVALID_FILTER` |
| 7 | Etags | `GetPolicy(".../policies/submit")` returns an `etag`; `UpdatePolicy` with the same text and another etag fails `ABORTED` with reason `ETAG_MISMATCH`; with the read etag it succeeds |
