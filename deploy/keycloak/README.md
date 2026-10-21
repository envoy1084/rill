# Keycloak for sign-in tests

A Keycloak realm (`rill`) that exercises Rill's OpenID Connect sign-in
against a real identity provider: a confidential client for the browser
flow (`rill-web`, secret `rill-web-secret`), a public client for the CLI's
device flow (`rill-cli`, which also allows the password grant so tests can
get an ID token without a browser), a `groups` claim, and users `alice`
(password `alice`, group `devs`) and `bob` (`bob`).

```sh
docker run -d --name rill-keycloak -p 8180:8080 \
  -e KC_BOOTSTRAP_ADMIN_USERNAME=admin -e KC_BOOTSTRAP_ADMIN_PASSWORD=admin \
  -v "$PWD/rill-realm.json:/opt/keycloak/data/import/rill-realm.json:ro" \
  quay.io/keycloak/keycloak:26.0 start-dev --import-realm
RILL_TEST_KEYCLOAK=http://localhost:8180/realms/rill \
  cargo test -p rill-api --test grpc keycloak
```

Point a dev server at it with
`rill admin idp add keycloak --issuer http://localhost:8180/realms/rill
--client-id rill-web --client-secret rill-web-secret --cli-client-id rill-cli
--groups-claim groups --auto-provision`, then `rill login`.
