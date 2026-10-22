package rill_test

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"os"
	"testing"
	"time"

	"connectrpc.com/connect"

	rillv1 "github.com/rill-vcs/rill/sdks/go/gen/rill/v1"
	"github.com/rill-vcs/rill/sdks/go/rill"
)

// The scenarios of sdks/conformance/README.md, against the server at
// RILL_ENDPOINT (`rill dev`); skipped when it is unset.

func setup(t *testing.T) (*rill.Client, string) {
	t.Helper()
	endpoint := os.Getenv("RILL_ENDPOINT")
	if endpoint == "" {
		t.Skip("RILL_ENDPOINT is not set")
	}
	c := rill.New(endpoint)
	var b [4]byte
	_, _ = rand.Read(b[:])
	id := "conf-" + hex.EncodeToString(b[:])
	_, err := c.Repos.CreateRepo(context.Background(), connect.NewRequest(&rillv1.CreateRepoRequest{
		RequestId: rill.RequestID(),
		Parent:    rill.Org(""),
		RepoId:    id,
	}))
	if err != nil {
		t.Fatal(err)
	}
	return c, rill.Repo(id)
}

func setFile(path, text string) *rillv1.Edit {
	return &rillv1.Edit{
		Path: path,
		Op: &rillv1.Edit_SetContent{SetContent: &rillv1.SetContent{
			Content: &rillv1.ContentRef{Ref: &rillv1.ContentRef_Inline{Inline: []byte(text)}},
			Kind:    rillv1.FileKind_FILE_KIND_REGULAR,
		}},
	}
}

func onRevision(revision string) *rillv1.EditBase {
	return &rillv1.EditBase{Base: &rillv1.EditBase_Revision{Revision: revision}}
}

func apply(t *testing.T, c *rill.Client, repo string, req *rillv1.ApplyEditsRequest) *rillv1.ApplyEditsResponse {
	t.Helper()
	req.Parent = repo
	resp, err := c.Changes.ApplyEdits(context.Background(), connect.NewRequest(req))
	if err != nil {
		t.Fatal(err)
	}
	return resp.Msg
}

func TestResourcesAndErrors(t *testing.T) {
	c, repo := setup(t)
	ctx := context.Background()
	got, err := c.Repos.GetRepo(ctx, connect.NewRequest(&rillv1.GetRepoRequest{Name: repo}))
	if err != nil || got.Msg.GetRepo().GetName() != repo {
		t.Fatalf("get repo: %v %v", got, err)
	}
	_, err = c.Repos.GetRepo(ctx, connect.NewRequest(&rillv1.GetRepoRequest{Name: rill.Repo("absent")}))
	if connect.CodeOf(err) != connect.CodeNotFound || !rill.Is(err, "NOT_FOUND") {
		t.Fatalf("missing repo: %v (reason %q)", err, rill.Reason(err))
	}
}

func TestWriteReadAndIdempotency(t *testing.T) {
	c, repo := setup(t)
	ctx := context.Background()
	id := rill.RequestID()
	first := apply(t, c, repo, &rillv1.ApplyEditsRequest{
		RequestId: id, Base: onRevision("trunk"), Message: "hello",
		Edits: []*rillv1.Edit{setFile("hello.txt", "hello\n")},
	})
	version := hex.EncodeToString(first.GetInfo().GetHash())
	file, err := c.Content.ReadFile(ctx, connect.NewRequest(&rillv1.ReadFileRequest{
		Repo: repo, Revision: version, Path: "hello.txt",
	}))
	if err != nil || !bytes.Equal(file.Msg.GetData(), []byte("hello\n")) {
		t.Fatalf("read file: %v %v", file, err)
	}
	dir, err := c.Content.ListDirectory(ctx, connect.NewRequest(&rillv1.ListDirectoryRequest{
		Repo: repo, Revision: version,
	}))
	if err != nil || len(dir.Msg.GetEntries()) != 1 || dir.Msg.GetEntries()[0].GetName() != "hello.txt" {
		t.Fatalf("list directory: %v %v", dir, err)
	}
	again := apply(t, c, repo, &rillv1.ApplyEditsRequest{
		RequestId: id, Base: onRevision("trunk"), Message: "hello",
		Edits: []*rillv1.Edit{setFile("hello.txt", "hello\n")},
	})
	if !bytes.Equal(again.GetInfo().GetHash(), first.GetInfo().GetHash()) {
		t.Fatal("a retried request made another version")
	}
	_, err = c.Changes.ApplyEdits(ctx, connect.NewRequest(&rillv1.ApplyEditsRequest{
		RequestId: id, Parent: repo, Base: onRevision("trunk"), Message: "another",
		Edits: []*rillv1.Edit{setFile("hello.txt", "hello\n")},
	}))
	if connect.CodeOf(err) != connect.CodeInvalidArgument || !rill.Is(err, "REQUEST_ID_REUSED") {
		t.Fatalf("reused request id: %v", err)
	}
}

func TestPages(t *testing.T) {
	c, repo := setup(t)
	ctx := context.Background()
	first := apply(t, c, repo, &rillv1.ApplyEditsRequest{
		Base: onRevision("trunk"), Message: "one", Edits: []*rillv1.Edit{setFile("a.txt", "1\n")},
	})
	second := apply(t, c, repo, &rillv1.ApplyEditsRequest{
		Base:    onRevision(hex.EncodeToString(first.GetInfo().GetHash())),
		Message: "two", Edits: []*rillv1.Edit{setFile("a.txt", "2\n")},
	})
	req := &rillv1.ListHistoryRequest{
		Repo: repo, Revision: hex.EncodeToString(second.GetInfo().GetHash()), PageSize: 1,
	}
	page, err := c.Content.ListHistory(ctx, connect.NewRequest(req))
	if err != nil || len(page.Msg.GetVersions()) != 1 || page.Msg.GetNextPageToken() == "" {
		t.Fatalf("first page: %v %v", page, err)
	}
	req.PageToken = page.Msg.GetNextPageToken()
	rest, err := c.Content.ListHistory(ctx, connect.NewRequest(req))
	if err != nil || !bytes.Equal(rest.Msg.GetVersions()[0].GetHash(), first.GetInfo().GetHash()) ||
		rest.Msg.GetNextPageToken() != "" {
		t.Fatalf("second page: %v %v", rest, err)
	}
}

func TestLandingAndFilters(t *testing.T) {
	c, repo := setup(t)
	ctx := context.Background()
	change := apply(t, c, repo, &rillv1.ApplyEditsRequest{
		Base: onRevision("trunk"), Message: "hello", Edits: []*rillv1.Edit{setFile("hello.txt", "hello\n")},
	}).GetInfo().GetChangeId()
	if _, err := c.Reviews.SubmitChange(ctx, connect.NewRequest(&rillv1.SubmitChangeRequest{
		Change: rill.Change(repo, change),
	})); err != nil {
		t.Fatal(err)
	}
	for _, v := range []struct {
		label string
		value int32
	}{{"Verified", 1}, {"Review", 2}} {
		if _, err := c.Reviews.Vote(ctx, connect.NewRequest(&rillv1.VoteRequest{
			Review: rill.Review(repo, change), Label: v.label, Value: v.value,
		})); err != nil {
			t.Fatal(err)
		}
	}
	land, err := c.Landing.RequestLand(ctx, connect.NewRequest(&rillv1.RequestLandRequest{
		Change: rill.Change(repo, change),
	}))
	if err != nil {
		t.Fatal(err)
	}
	name := land.Msg.GetLandRequest().GetName()
	deadline := time.Now().Add(30 * time.Second)
	for {
		got, err := c.Landing.GetLandRequest(ctx, connect.NewRequest(&rillv1.GetLandRequestRequest{Name: name}))
		if err != nil {
			t.Fatal(err)
		}
		if got.Msg.GetLandRequest().GetStatus() == rillv1.LandStatus_LAND_STATUS_LANDED {
			if got.Msg.GetLandRequest().GetTrunkPosition() != 1 {
				t.Fatalf("landed at %d", got.Msg.GetLandRequest().GetTrunkPosition())
			}
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("did not land: %v", got.Msg)
		}
		time.Sleep(100 * time.Millisecond)
	}
	head, err := c.Repos.GetTrunkPosition(ctx, connect.NewRequest(&rillv1.GetTrunkPositionRequest{
		Name: rill.TrunkHead(repo),
	}))
	if err != nil || head.Msg.GetLanding().GetPosition() != 1 {
		t.Fatalf("trunk head: %v %v", head, err)
	}
	reviews, err := c.Reviews.ListReviews(ctx, connect.NewRequest(&rillv1.ListReviewsRequest{
		Parent: repo, Filter: `status = "landed"`,
	}))
	if err != nil || len(reviews.Msg.GetReviews()) != 1 ||
		!bytes.Equal(reviews.Msg.GetReviews()[0].GetChangeId(), change) {
		t.Fatalf("filtered reviews: %v %v", reviews, err)
	}
	_, err = c.Reviews.ListReviews(ctx, connect.NewRequest(&rillv1.ListReviewsRequest{
		Parent: repo, Filter: `colour = "red"`,
	}))
	if connect.CodeOf(err) != connect.CodeInvalidArgument || !rill.Is(err, "INVALID_FILTER") {
		t.Fatalf("bad filter: %v", err)
	}
}

func TestEtags(t *testing.T) {
	c, repo := setup(t)
	ctx := context.Background()
	got, err := c.Policies.GetPolicy(ctx, connect.NewRequest(&rillv1.GetPolicyRequest{
		Name: rill.Policy(repo, "submit"),
	}))
	if err != nil || got.Msg.GetPolicy().GetEtag() == "" {
		t.Fatalf("get policy: %v %v", got, err)
	}
	policy := got.Msg.GetPolicy()
	update := func(etag string) error {
		_, err := c.Policies.UpdatePolicy(ctx, connect.NewRequest(&rillv1.UpdatePolicyRequest{
			Policy: &rillv1.Policy{Name: policy.GetName(), Text: policy.GetText()},
			Etag:   etag,
		}))
		return err
	}
	if err := update("stale"); connect.CodeOf(err) != connect.CodeAborted || !rill.Is(err, "ETAG_MISMATCH") {
		t.Fatalf("stale etag: %v", err)
	}
	if err := update(policy.GetEtag()); err != nil {
		t.Fatal(err)
	}
}

func TestRetryGivesUpOnErrorsThatAreNotTransient(t *testing.T) {
	calls := 0
	err := rill.Retry(context.Background(), func(context.Context) error {
		calls++
		return connect.NewError(connect.CodeNotFound, nil)
	})
	if connect.CodeOf(err) != connect.CodeNotFound || calls != 1 {
		t.Fatalf("%v after %d calls", err, calls)
	}
}
