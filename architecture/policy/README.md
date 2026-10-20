# Policy

`rill-policy` decides who may do what and how risky a change is. It holds five
independent mechanisms that the server composes per request: principal
resolution, Biscuit capability tokens, ReBAC relationship checks, risk
classification plus Cedar submit rules, and Ed25519 object signatures. The
crate performs no I/O except the ReBAC authorizer's tuple scan; every other
evaluation is a pure function of facts the caller assembles. The invariant
that ties the pages together: a request must pass both the token it carries
(if any) and the relations stored on the repository, and neither layer can
widen the other.

| Page | Topic | Source |
| --- | --- | --- |
| [identity-and-authentication](identity-and-authentication.md) | `Principal`, `AuthContext`, the static and Biscuit authenticators, the gRPC auth layer and HTTP middleware, `Permit` | `identity.rs`, `rill-server/src/auth/` |
| [capability-tokens](capability-tokens.md) | `Capability`, `Issuer`, `Verifier`, Datalog checks, offline attenuation, `IssueToken` | `tokens/` |
| [permissions](permissions.md) | Relationship tuples, `owner ⊇ writer ⊇ reader`, directory inheritance, `PathFilter`, admin RPCs | `rebac/` |
| [submit-policy](submit-policy.md) | `ChangeFacts`, `classify`, `RiskConfig`, the Cedar schema and default policy, `SubmitPolicy::evaluate`, `SetPolicy` | `risk/`, `submit/`, `rill-server/src/state/policy.rs` |
| [signing](signing.md) | `Signer`, `verify`, `KeyFile`, detached signatures on change versions | `signing.rs` |

Shared plumbing: [`error.rs`](../../crates/rill-policy/src/error.rs) defines
`Error` (`InvalidGlob`, `PolicyParse`, `PolicyValidation`, `PolicyEvaluation`,
`Token`, `TokenRejected`, `TokenExpired`, `Widening`, `InvalidKey`,
`Unauthenticated`, `Store`, `Types`) and the `Result<T>` alias;
[`lib.rs`](../../crates/rill-policy/src/lib.rs) re-exports the public surface.
The server maps these to gRPC codes in
[`policy_code`](../../crates/rill-server/src/error.rs): token and
authentication errors to `UNAUTHENTICATED`, `TokenRejected` to
`PERMISSION_DENIED`, parse and validation errors to `INVALID_ARGUMENT`,
`PolicyEvaluation` to `INTERNAL`.
