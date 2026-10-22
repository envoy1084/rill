package rill

import (
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"strings"
)

// DefaultOrg is the org a bare repository name is in.
const DefaultOrg = "default"

// Repo is orgs/{org}/repos/{repo} for a bare name, org/name or a full name.
func Repo(spec string) string {
	if strings.HasPrefix(spec, "orgs/") {
		return spec
	}
	if org, name, ok := strings.Cut(spec, "/"); ok {
		return "orgs/" + org + "/repos/" + name
	}
	return "orgs/" + DefaultOrg + "/repos/" + spec
}

// Org is orgs/{org}; the default org when org is empty.
func Org(org string) string {
	if org == "" {
		org = DefaultOrg
	}
	return "orgs/" + org
}

// Change is {repo}/changes/ch_{hex} for a change id as messages carry it.
func Change(repo string, id []byte) string {
	return Repo(repo) + "/changes/ch_" + hex.EncodeToString(id)
}

// Review is the review of a change.
func Review(repo string, id []byte) string {
	return Change(repo, id) + "/review"
}

// Version is {repo}/versions/{hex} for a version hash.
func Version(repo string, hash []byte) string {
	return Repo(repo) + "/versions/" + hex.EncodeToString(hash)
}

// TrunkPosition is {repo}/trunk/positions/{n}.
func TrunkPosition(repo string, n uint64) string {
	return fmt.Sprintf("%s/trunk/positions/%d", Repo(repo), n)
}

// TrunkHead is {repo}/trunk/positions/head.
func TrunkHead(repo string) string {
	return Repo(repo) + "/trunk/positions/head"
}

// Policy is {repo}/policies/{policy}.
func Policy(repo, policy string) string {
	return Repo(repo) + "/policies/" + policy
}

// RequestID is a fresh idempotency key: a random UUID.
func RequestID() string {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		panic(err)
	}
	b[6] = b[6]&0x0f | 0x40
	b[8] = b[8]&0x3f | 0x80
	h := hex.EncodeToString(b[:])
	return h[0:8] + "-" + h[8:12] + "-" + h[12:16] + "-" + h[16:20] + "-" + h[20:]
}
