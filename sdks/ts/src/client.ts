// A client holding every rill.v1 service over one Connect transport, with
// an interceptor that authenticates and identifies each call.

import { createClient, type Client, type Interceptor } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-node";

import { AccessService } from "./gen/rill/v1/access_pb.js";
import { AdminService } from "./gen/rill/v1/admin_pb.js";
import { AuditService } from "./gen/rill/v1/audit_pb.js";
import { ChangeService } from "./gen/rill/v1/changes_pb.js";
import { ChecksService } from "./gen/rill/v1/checks_pb.js";
import { ContentService } from "./gen/rill/v1/content_pb.js";
import { EventService } from "./gen/rill/v1/events_pb.js";
import { IdentityService } from "./gen/rill/v1/identity_pb.js";
import { IntentService } from "./gen/rill/v1/intent_pb.js";
import { LandingService } from "./gen/rill/v1/landing_pb.js";
import { OperationService } from "./gen/rill/v1/operations_pb.js";
import { OrgService } from "./gen/rill/v1/orgs_pb.js";
import { PolicyService } from "./gen/rill/v1/policy_pb.js";
import { ProtectionService } from "./gen/rill/v1/protection_pb.js";
import { RepoService } from "./gen/rill/v1/repos_pb.js";
import { ReviewService } from "./gen/rill/v1/review_pb.js";
import { SecretService } from "./gen/rill/v1/secrets_pb.js";
import { SessionService } from "./gen/rill/v1/session_pb.js";
import { TrunkService } from "./gen/rill/v1/trunk_pb.js";
import { WorkspaceService } from "./gen/rill/v1/workspace_pb.js";
import { requestId } from "./names.js";

/** How to reach and authenticate to a server. */
export interface Options {
  /** Sent as a bearer credential on every call. */
  token?: string;
  /** Names the caller in the User-Agent header. */
  userAgent?: string;
}

/** A generated client for every rill.v1 service. */
export interface RillClient {
  repos: Client<typeof RepoService>;
  content: Client<typeof ContentService>;
  changes: Client<typeof ChangeService>;
  events: Client<typeof EventService>;
  workspaces: Client<typeof WorkspaceService>;
  reviews: Client<typeof ReviewService>;
  landing: Client<typeof LandingService>;
  trunk: Client<typeof TrunkService>;
  operations: Client<typeof OperationService>;
  intents: Client<typeof IntentService>;
  checks: Client<typeof ChecksService>;
  policies: Client<typeof PolicyService>;
  orgs: Client<typeof OrgService>;
  admin: Client<typeof AdminService>;
  access: Client<typeof AccessService>;
  identity: Client<typeof IdentityService>;
  sessions: Client<typeof SessionService>;
  audit: Client<typeof AuditService>;
  protection: Client<typeof ProtectionService>;
  secrets: Client<typeof SecretService>;
}

/** A client for the server at `endpoint` (`http://host:port`). */
export function createRillClient(endpoint: string, options: Options = {}): RillClient {
  const headers: Interceptor = (next) => (req) => {
    if (options.token) {
      req.header.set("Authorization", `Bearer ${options.token}`);
    }
    req.header.set("X-Request-Id", requestId());
    req.header.set("User-Agent", options.userAgent ?? "rill-sdk-ts");
    return next(req);
  };
  const transport = createConnectTransport({
    baseUrl: endpoint,
    httpVersion: "1.1",
    interceptors: [headers],
  });
  return {
    repos: createClient(RepoService, transport),
    content: createClient(ContentService, transport),
    changes: createClient(ChangeService, transport),
    events: createClient(EventService, transport),
    workspaces: createClient(WorkspaceService, transport),
    reviews: createClient(ReviewService, transport),
    landing: createClient(LandingService, transport),
    trunk: createClient(TrunkService, transport),
    operations: createClient(OperationService, transport),
    intents: createClient(IntentService, transport),
    checks: createClient(ChecksService, transport),
    policies: createClient(PolicyService, transport),
    orgs: createClient(OrgService, transport),
    admin: createClient(AdminService, transport),
    access: createClient(AccessService, transport),
    identity: createClient(IdentityService, transport),
    sessions: createClient(SessionService, transport),
    audit: createClient(AuditService, transport),
    protection: createClient(ProtectionService, transport),
    secrets: createClient(SecretService, transport),
  };
}
