// Package rill is a thin layer over the generated rill.v1 Connect clients:
// one Client holding every service, an interceptor that authenticates and
// identifies each call, resource-name helpers, error reasons and retries.
package rill

import (
	"context"
	"net/http"

	"connectrpc.com/connect"

	"github.com/rill-vcs/rill/sdks/go/gen/rill/v1/rillv1connect"
)

// Client holds a generated client for every rill.v1 service.
type Client struct {
	Repos      rillv1connect.RepoServiceClient
	Content    rillv1connect.ContentServiceClient
	Changes    rillv1connect.ChangeServiceClient
	Events     rillv1connect.EventServiceClient
	Workspaces rillv1connect.WorkspaceServiceClient
	Reviews    rillv1connect.ReviewServiceClient
	Landing    rillv1connect.LandingServiceClient
	Trunk      rillv1connect.TrunkServiceClient
	Operations rillv1connect.OperationServiceClient
	Intents    rillv1connect.IntentServiceClient
	Checks     rillv1connect.ChecksServiceClient
	Policies   rillv1connect.PolicyServiceClient
	Orgs       rillv1connect.OrgServiceClient
	Admin      rillv1connect.AdminServiceClient
	Access     rillv1connect.AccessServiceClient
	Identity   rillv1connect.IdentityServiceClient
	Sessions   rillv1connect.SessionServiceClient
	Audit      rillv1connect.AuditServiceClient
	Protection rillv1connect.ProtectionServiceClient
	Secrets    rillv1connect.SecretServiceClient
}

// Option configures a Client.
type Option func(*options)

type options struct {
	token      string
	userAgent  string
	httpClient connect.HTTPClient
}

// WithToken sends token as a bearer credential on every call.
func WithToken(token string) Option {
	return func(o *options) { o.token = token }
}

// WithUserAgent names the caller in the User-Agent header.
func WithUserAgent(agent string) Option {
	return func(o *options) { o.userAgent = agent }
}

// WithHTTPClient sends calls through c instead of http.DefaultClient.
func WithHTTPClient(c connect.HTTPClient) Option {
	return func(o *options) { o.httpClient = c }
}

// New returns a Client for the server at endpoint (http://host:port or
// https://host:port), speaking the Connect protocol.
func New(endpoint string, opts ...Option) *Client {
	o := options{userAgent: "rill-sdk-go", httpClient: http.DefaultClient}
	for _, opt := range opts {
		opt(&o)
	}
	h, u := o.httpClient, endpoint
	co := connect.WithInterceptors(headers(o))
	return &Client{
		Repos:      rillv1connect.NewRepoServiceClient(h, u, co),
		Content:    rillv1connect.NewContentServiceClient(h, u, co),
		Changes:    rillv1connect.NewChangeServiceClient(h, u, co),
		Events:     rillv1connect.NewEventServiceClient(h, u, co),
		Workspaces: rillv1connect.NewWorkspaceServiceClient(h, u, co),
		Reviews:    rillv1connect.NewReviewServiceClient(h, u, co),
		Landing:    rillv1connect.NewLandingServiceClient(h, u, co),
		Trunk:      rillv1connect.NewTrunkServiceClient(h, u, co),
		Operations: rillv1connect.NewOperationServiceClient(h, u, co),
		Intents:    rillv1connect.NewIntentServiceClient(h, u, co),
		Checks:     rillv1connect.NewChecksServiceClient(h, u, co),
		Policies:   rillv1connect.NewPolicyServiceClient(h, u, co),
		Orgs:       rillv1connect.NewOrgServiceClient(h, u, co),
		Admin:      rillv1connect.NewAdminServiceClient(h, u, co),
		Access:     rillv1connect.NewAccessServiceClient(h, u, co),
		Identity:   rillv1connect.NewIdentityServiceClient(h, u, co),
		Sessions:   rillv1connect.NewSessionServiceClient(h, u, co),
		Audit:      rillv1connect.NewAuditServiceClient(h, u, co),
		Protection: rillv1connect.NewProtectionServiceClient(h, u, co),
		Secrets:    rillv1connect.NewSecretServiceClient(h, u, co),
	}
}

// headers sets the credential, a fresh x-request-id and the user agent on
// every call, unary or streaming.
func headers(o options) connect.Interceptor {
	set := func(h http.Header) {
		if o.token != "" {
			h.Set("Authorization", "Bearer "+o.token)
		}
		h.Set("X-Request-Id", RequestID())
		h.Set("User-Agent", o.userAgent)
	}
	return interceptor{set: set}
}

type interceptor struct{ set func(http.Header) }

func (i interceptor) WrapUnary(next connect.UnaryFunc) connect.UnaryFunc {
	return func(ctx context.Context, req connect.AnyRequest) (connect.AnyResponse, error) {
		i.set(req.Header())
		return next(ctx, req)
	}
}

func (i interceptor) WrapStreamingClient(next connect.StreamingClientFunc) connect.StreamingClientFunc {
	return func(ctx context.Context, spec connect.Spec) connect.StreamingClientConn {
		conn := next(ctx, spec)
		i.set(conn.RequestHeader())
		return conn
	}
}

func (i interceptor) WrapStreamingHandler(next connect.StreamingHandlerFunc) connect.StreamingHandlerFunc {
	return next
}
